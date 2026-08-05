use crossterm::style::Stylize;
use reqwest::Method;
use crate::{check_setup_cmpl, error_string, info_string, warning_string, setup_string};
use crate::config::{data_dir, load_server_profile};
use crate::transport::ApiClient;

const CLIENT_SESSION_FILE: &str = ".client_session_id";

fn new_client_session_id() -> String {
    format!(
        "{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    )
}

fn parse_token_response(response: &serde_json::Value) -> Result<(String, String, String), anyhow::Error> {
    let access_token = response["accessToken"]
        .as_str()
        .or_else(|| response["token"].as_str())
        .ok_or_else(|| anyhow::anyhow!("Access token not found in server response"))?
        .to_string();
    let refresh_token = response["refreshToken"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Refresh token not found in server response"))?
        .to_string();
    let user_id = response["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("User ID not found in server response"))?
        .to_string();
    Ok((access_token, refresh_token, user_id))
}

fn save_auth_tokens(access_token: &str, refresh_token: &str, user_id: &str) -> Result<(), anyhow::Error> {
    let data_root = data_dir()?;
    let auth_file_path = data_root.join(".auth_tokens.txt");
    std::fs::create_dir_all(auth_file_path.parent().unwrap())?;
    let auth_data = format!("{}\n{}\n{}\n", access_token, refresh_token, user_id);
    std::fs::write(auth_file_path, auth_data)
        .map_err(|e| anyhow::anyhow!("Failed to save auth tokens: {}", e))?;
    let synced_file_path = data_root.join("synced.txt");
    std::fs::write(synced_file_path, format!("{}\n", access_token))?;
    // A login starts a new client session. Cached files remain available for
    // offline inspection, but each resource is revalidated once in the new
    // session before subsequent reads use the local copy.
    std::fs::write(data_root.join(CLIENT_SESSION_FILE), new_client_session_id())?;
    Ok(())
}

pub fn current_client_session_id() -> Result<String, anyhow::Error> {
    let path = data_dir()?.join(CLIENT_SESSION_FILE);
    let session_id = std::fs::read_to_string(path)?.trim().to_string();
    if session_id.is_empty() {
        return Err(anyhow::anyhow!("Client session marker is empty"));
    }
    Ok(session_id)
}

fn keychain_entry() -> Result<keyring::Entry, anyhow::Error> {
    let profile = load_server_profile()?;
    Ok(keyring::Entry::new("archerdndsys", &profile.base_url)?)
}

fn save_auto_login_credentials(username: &str, password: &str) -> Result<(), anyhow::Error> {
    keychain_entry()?.set_password(&format!("{username}\n{password}"))?;
    Ok(())
}

fn load_auto_login_credentials() -> Result<(String, String), anyhow::Error> {
    let stored = keychain_entry()?.get_password()?;
    let mut parts = stored.splitn(2, '\n');
    let username = parts.next().unwrap_or_default().trim();
    let password = parts.next().unwrap_or_default();
    if username.is_empty() || password.is_empty() {
        return Err(anyhow::anyhow!("Stored auto-login credentials are invalid"));
    }
    Ok((username.to_string(), password.to_string()))
}

fn delete_auto_login_credentials() {
    if let Ok(entry) = keychain_entry() { let _ = entry.delete_credential(); }
}

async fn base_login(username: &str, password: &str) -> Result<(String, String), anyhow::Error> {
    let client = ApiClient::from_saved_profile()?;
    let response = client.request(Method::POST, "/auth/login")
        .json(&serde_json::json!({
            "username": username,
            "password": password
        }))
        .send()
        .await?;

    if response.status().is_success() {
        // Parse the response to get token and id
        let login_response = response.json::<serde_json::Value>().await?;

        let (token, refresh_token, user_id) = parse_token_response(&login_response)?;
        save_auth_tokens(&token, &refresh_token, &user_id)?;

        println!("{}", info_string("Login successful. Auth token saved."));
        println!("{}", warning_string("Save info for auto login? (y/n)"));

        // Save auto login info
        let mut auto_login_choice = String::new();
        std::io::stdin().read_line(&mut auto_login_choice).expect("Failed to read input");
        auto_login_choice = auto_login_choice.trim().to_string();
        if auto_login_choice.trim().eq_ignore_ascii_case("y") {
            save_auto_login_credentials(&username, &password)?;
            println!("{}", info_string("Auto login info saved."));
                    } else {
            println!("{}", warning_string("Auto login info not saved. Try again later."));
        }

        // Return the token and user id
        Ok((token, user_id))
    } else {
        let error_text = response.text().await?;
        println!("{}", error_string("Login failed."));
        println!("{}", error_string(&format!("Response: {}", error_text)));
        Err(anyhow::anyhow!("Login failed: {}", error_text))
    }
}

pub async fn auto_login() -> Result<(String, String), anyhow::Error> {
    check_setup_cmpl()?;
    if let Ok((username, password)) = load_auto_login_credentials() {
        println!("{}", info_string("Auto login found in the system keychain. Logging in..."));
        return base_login(&username, &password).await;
    }
    let auth_fp = data_dir()?.join(".auto_login.txt");
    if !auth_fp.exists() {
        return Err(anyhow::anyhow!("No auto login found. Please login manually first."));
    }

    let auth_data = std::fs::read_to_string(auth_fp)
        .map_err(|e| anyhow::anyhow!("Failed to read auth token file: {}", e))?;

    if !auth_data.contains(',') {
        println!("{}", error_string("Invalid auth token format."));
        return Err(anyhow::anyhow!("Invalid auth token format."));
    }

    let parts: Vec<&str> = auth_data.trim().split(',').collect();
    if parts.len() != 2 {
        println!("{}", error_string("Invalid auth token format."));
        return Err(anyhow::anyhow!("Invalid auth token format."));
    }

    println!("{}", info_string("Auto login found. Logging in..."));
    base_login(&parts[0], &parts[1]).await
}

pub async fn manual_login() -> Result<(String, String), anyhow::Error> {
    check_setup_cmpl()?;
    println!("{}", warning_string("Please enter your username and password to login."));

    let mut username = String::new();
    let mut password = String::new();

    println!("{}", warning_string("Enter username:"));
    std::io::stdin().read_line(&mut username).expect("Failed to read username");
    username = username.trim().to_string();

    println!("{}", warning_string("Enter password:"));
    std::io::stdin().read_line(&mut password).expect("Failed to read password");
    password = password.trim().to_string();

    base_login(&username, &password).await
}

pub async fn register() -> Result<(String, String), anyhow::Error> {
    let mut username = String::new();
    let mut email = String::new();
    let mut password = String::new();
    let mut beta_code = String::new();

    println!("{}", warning_string("Please enter your username, email, and password to register."));
    print!("{}", warning_string("Enter username:"));
    std::io::stdin().read_line(&mut username).expect("Failed to read username");
    username = username.trim().to_string();
    print!("{}", warning_string("Enter email:"));
    std::io::stdin().read_line(&mut email).expect("Failed to read email");
    email = email.trim().to_string();
    print!("{}", warning_string("Enter password:"));
    std::io::stdin().read_line(&mut password).expect("Failed to read password");
    password = password.trim().to_string();
    print!("{}", warning_string("Enter beta code: "));
    std::io::stdin().read_line(&mut beta_code).expect("Failed to read beta code");
    beta_code = beta_code.trim().to_string();

    let client = ApiClient::from_saved_profile()?;
    let response = client.request(Method::POST, "/auth/register")
        .json(&serde_json::json!({
            "username": username,
            "email": email,
            "password": password,
            "betaCode": beta_code
        }))
        .send()
        .await?;

    if response.status().is_success() {
        let register_response = response.json::<serde_json::Value>().await?;

        let (token, refresh_token, user_id) = parse_token_response(&register_response)?;
        save_auth_tokens(&token, &refresh_token, &user_id)?;

        println!("{}", info_string("Registration successful. Auth token saved."));

        println!("{}", warning_string("Save info for auto login?"));
        // Save auto login info
        let mut auto_login_choice = String::new();
        std::io::stdin().read_line(&mut auto_login_choice).expect("Failed to read input");
        auto_login_choice = auto_login_choice.trim().to_string();
        if auto_login_choice.trim().eq_ignore_ascii_case("y") {
            save_auto_login_credentials(&username, &password)?;
            println!("{}", info_string("Auto login info saved."));
                    } else {
            println!("{}", warning_string("Auto login info not saved. Try again later."));
        }

        // Return the token and user id
        Ok((token, user_id))
    } else {
        let error_text = response.text().await?;
        println!("{}", error_string("Registration failed."));
        println!("{}", error_string(&format!("Response: {}", error_text)));
        Err(anyhow::anyhow!("Registration failed: {}", error_text))
    }
}

pub async fn logout() -> Result<(), anyhow::Error> {
    check_setup_cmpl()?;
    let auth_file_path = data_dir()?.join(".auth_tokens.txt");

    println!("{}", warning_string("Logging out..."));
    // Send logout request to server
    let client = ApiClient::from_saved_profile()?;
    let (access_token, refresh_token) = read_auth_tokens()?;
    let response = client.request(Method::POST, "/auth/logout")
        .bearer_auth(access_token)
        .json(&serde_json::json!({ "refreshToken": refresh_token }))
        .send()
        .await?;
    if response.status().is_success() {
        println!("{}", info_string("Logout request sent successfully."));
    } else {
        let error_text = response.text().await?;
        println!("{}", error_string("Logout failed."));
        println!("{}", error_string(&format!("Response: {}", error_text)));
        return Err(anyhow::anyhow!("Logout failed: {}", error_text));
    }

    if auth_file_path.exists() {
        std::fs::remove_file(auth_file_path)
            .map_err(|e| anyhow::anyhow!("Failed to remove auth token file: {}", e))?;
        println!("{}", info_string("Logout successful. Auth token removed."));
            } else {
        println!("{}", warning_string("No auth token found. Already logged out."));
    }
    delete_auto_login_credentials();
    let _ = std::fs::remove_file(data_dir()?.join(CLIENT_SESSION_FILE));

    Ok(())
}

pub async fn is_signed_in() -> bool {
    let auth_file_path = match data_dir() {
        Ok(root) => root.join(".auth_tokens.txt"),
        Err(_) => return false,
    };
    if auth_file_path.exists() {
        match std::fs::read_to_string(auth_file_path) {
            Ok(data) => {
                let token = data.lines().next().map(str::trim).filter(|value| !value.is_empty())
                    .or_else(|| data.split(',').next().map(str::trim));
                let Some(token) = token else {
                    println!("{}", warning_string("Invalid auth token format."));
                    return false;
                };
                let client = match ApiClient::from_saved_profile() {
                    Ok(client) => client,
                    Err(_) => return false,
                };
                let response = client.request(Method::GET, "/auth/is-logged-in")
                    .bearer_auth(token)
                    .send()
                    .await;
                if response.map(|r| r.status().is_success()).unwrap_or(false) {
                    println!("{}", info_string("User is signed in."));
                    true
                } else {
                    println!("{}", warning_string("User is not signed in."));
                    false
                }
            },
                                    Err(e) => {
                                        println!("{}", error_string(&format!("Failed to read auth token file: {}", e)));
                false
            }
        }
    } else {
        false
    }
}

pub fn read_auth_tokens() -> Result<(String, String), anyhow::Error> {
    let auth_tokens_path = data_dir()?.join(".auth_tokens.txt");
    
    if !auth_tokens_path.exists() {
        return Err(anyhow::anyhow!("Authorization tokens file not found. {}", setup_string));
    }

    let auth_data = std::fs::read_to_string(auth_tokens_path)
        .map_err(|e| anyhow::anyhow!("Failed to read auth tokens: {}", e))?;

    let lines: Vec<&str> = auth_data.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    if lines.len() >= 3 {
        return Ok((lines[0].to_string(), lines[1].to_string()));
    }
    let parts: Vec<&str> = auth_data.trim().split(',').collect();
    if parts.len() == 2 {
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }
    Err(anyhow::anyhow!("Invalid auth token format."))
}
