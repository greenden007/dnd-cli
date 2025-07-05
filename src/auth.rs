use crossterm::style::Stylize;
use crate::{SERVER, check_setup_cmpl, error_string, info_string, warning_string, debug_string, trace_string, setup_string};

async fn base_login(username: &str, password: &str) -> Result<(String, String), anyhow::Error> {
    let client = reqwest::Client::new();
    let response = client.post(format!("{}/auth/login", SERVER))
        .json(&serde_json::json!({
            "username": username,
            "password": password
        }))
        .send()
        .await?;

    if response.status().is_success() {
        // Parse the response to get token and id
        let login_response = response.json::<serde_json::Value>().await?;

        // Extract token and user id
        let token = login_response["token"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Token not found in response"))?
            .to_string();
        let user_id = login_response["id"].as_str()
            .ok_or_else(|| anyhow::anyhow!("User ID not found in response"))?
            .to_string();

        // Save token to file
        let home_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let auth_file_path = home_dir.join(".archerdndsys/.auth_tokens.txt");

        // Format: token,user_id
        let auth_data = format!("{},{}", token, user_id);
        std::fs::write(auth_file_path, auth_data)
            .map_err(|e| anyhow::anyhow!("Failed to save auth token: {}", e))?;

        let synced_file_path = home_dir.join(".archerdndsys/synced.txt");
        let token_form = format!("{}\n", token);
        std::fs::write(synced_file_path, token_form)
            .map_err(|e| anyhow::anyhow!("Failed to save synced file: {}", e))?;

        println!("{}", info_string("Login successful. Auth token saved."));
        println!("{}", warning_string("Save info for auto login? (y/n)"));

        // Save auto login info
        let mut auto_login_choice = String::new();
        std::io::stdin().read_line(&mut auto_login_choice).expect("Failed to read input");
        auto_login_choice = auto_login_choice.trim().to_string();
        if auto_login_choice.trim().eq_ignore_ascii_case("y") {
            let auto_login_data = format!("{},{}", username, password);
            let auto_login_fp = home_dir.join(".archerdndsys/.auto_login.txt");
            std::fs::write(auto_login_fp, auto_login_data)
                .map_err(|e| anyhow::anyhow!("Failed to save auto login token: {}", e))?;
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
    let auth_fp = dirs::home_dir().unwrap().join(".archerdndsys/.auto_login.txt");
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

    let client = reqwest::Client::new();
    let response = client.post(format!("{}/auth/register", SERVER))
        .json(&serde_json::json!({
            "username": username,
            "email": email,
            "password": password
        }))
        .send()
        .await?;

    if response.status().is_success() {
        let register_response = response.json::<serde_json::Value>().await?;

        let token = register_response["token"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Token not found in response"))?
            .to_string();

        let user_id = register_response["id"].as_str()
            .ok_or_else(|| anyhow::anyhow!("User ID not found in response"))?
            .to_string();

        // Save token to file
        let home_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let auth_file_path = home_dir.join(".archerdndsys/.auth_tokens.txt");
        // Format: token,user_id
        let auth_data = format!("{},{}", token, user_id);
        std::fs::write(auth_file_path, auth_data)
            .map_err(|e| anyhow::anyhow!("Failed to save auth token: {}", e))?;

        let synced_file_path = home_dir.join(".archerdndsys/synced.txt");
        let token_form = format!("{}\n", token);
        std::fs::write(synced_file_path, token_form)
            .map_err(|e| anyhow::anyhow!("Failed to save synced file: {}", e))?;

        println!("{}", info_string("Registration successful. Auth token saved."));

        println!("{}", warning_string("Save info for auto login?"));
        // Save auto login info
        let mut auto_login_choice = String::new();
        std::io::stdin().read_line(&mut auto_login_choice).expect("Failed to read input");
        auto_login_choice = auto_login_choice.trim().to_string();
        if auto_login_choice.trim().eq_ignore_ascii_case("y") {
            let auto_login_data = format!("{},{}", username, password);
            let auto_login_fp = home_dir.join(".archerdndsys/.auto_login.txt");
            std::fs::write(auto_login_fp, auto_login_data)
                .map_err(|e| anyhow::anyhow!("Failed to save auto login token: {}", e))?;
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
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let auth_file_path = home_dir.join(".archerdndsys/.auth_tokens.txt");

    println!("{}", warning_string("Logging out..."));
    // Send logout request to server
    let client = reqwest::Client::new();
    let response = client.post(format!("{}/auth/logout", SERVER))
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

    Ok(())
}

pub async fn is_signed_in() -> bool {
    let home_dir = dirs::home_dir().unwrap_or_else(|| {
        println!("{}", error_string("Could not find home directory."));
        std::process::exit(1);
    });
    let auth_file_path = home_dir.join(".archerdndsys/.auth_tokens.txt");
    if auth_file_path.exists() {
        match std::fs::read_to_string(auth_file_path) {
            Ok(data) => {
                if data.contains(',') {
                    let parts: Vec<&str> = data.trim().split(',').collect();
                    if parts.len() == 2 {
                        let token = parts[0].trim();
                        let user_id = parts[1].trim();
                        let client = reqwest::Client::new();
                        let response = client.get(format!("{}/auth/is-logged-in", SERVER))
                            .json(&serde_json::json!({
                                "token": token,
                                "user_id": user_id
                            }))
                            .send()
                            .await;
                        if response.unwrap().status().is_success() {
                            println!("{}", info_string("User is signed in."));
                            true
                        } else {
                            println!("{}", warning_string("User is not signed in."));
                            false
                        }
                                            } else {
                        println!("{}", warning_string("Invalid auth token format."));
                        false
                                            }
                                        } else {
                                            println!("{}", warning_string("Invalid auth token format."));
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
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let auth_tokens_path = home_dir.join(".archerdndsys/.auth_tokens.txt");
    
    if !auth_tokens_path.exists() {
        return Err(anyhow::anyhow!("Authorization tokens file not found. {}", setup_string));
    }

    let auth_data = std::fs::read_to_string(auth_tokens_path)
        .map_err(|e| anyhow::anyhow!("Failed to read auth tokens: {}", e))?;

    if !auth_data.contains(',') {
        return Err(anyhow::anyhow!("Invalid auth token format."));
    }

    let parts: Vec<&str> = auth_data.trim().split(',').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("Invalid auth token format."));
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}
