use crossterm::style::Stylize;
use crate::{SERVER, check_setup_cmpl};

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

        println!("{}", "[INFO] Login successful. Auth token saved.".green());
        println!("{}", "[INFO] Save info for auto login? (y/n)".yellow());

        // Save auto login info
        let mut auto_login_choice = String::new();
        std::io::stdin().read_line(&mut auto_login_choice).expect("Failed to read input");
        auto_login_choice = auto_login_choice.trim().to_string();
        if auto_login_choice.trim().eq_ignore_ascii_case("y") {
            let auto_login_data = format!("{},{}", username, password);
            let auto_login_fp = home_dir.join(".archerdndsys/.auto_login.txt");
            std::fs::write(auto_login_fp, auto_login_data)
                .map_err(|e| anyhow::anyhow!("Failed to save auto login token: {}", e))?;
            println!("{}", "[INFO] Auto login info saved.".green());
        } else {
            println!("{}", "[INFO] Auto login info not saved. Try again later.".yellow());
        }

        // Return the token and user id
        Ok((token, user_id))
    } else {
        let error_text = response.text().await?;
        println!("{}", "[ERROR] Login failed.".red());
        println!("{} {}", "[ERROR] Response: ".red(), error_text);
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
        println!("{}", "[ERROR] Invalid auth token format.".red());
        return Err(anyhow::anyhow!("Invalid auth token format."));
    }

    let parts: Vec<&str> = auth_data.trim().split(',').collect();
    if parts.len() != 2 {
        println!("{}", "[ERROR] Invalid auth token format.".red());
        return Err(anyhow::anyhow!("Invalid auth token format."));
    }

    println!("{}", "[INFO] Auto login found. Logging in...".green());
    base_login(&parts[0], &parts[1]).await
}

pub async fn manual_login() -> Result<(String, String), anyhow::Error> {
    check_setup_cmpl()?;
    println!("{}", "[INFO] Please enter your username and password to login.".yellow());

    let mut username = String::new();
    let mut password = String::new();

    println!("{}", "[INFO] Enter username: ".yellow());
    std::io::stdin().read_line(&mut username).expect("Failed to read username");
    username = username.trim().to_string();

    println!("{}", "[INFO] Enter password: ".yellow());
    std::io::stdin().read_line(&mut password).expect("Failed to read password");
    password = password.trim().to_string();

    base_login(&username, &password).await
}

pub async fn register() -> Result<(String, String), anyhow::Error> {
    let mut username = String::new();
    let mut email = String::new();
    let mut password = String::new();

    println!("{}", "[INFO] Please enter your username, email, and password to register.".yellow());
    print!("{}", "[INFO] Enter username: ".yellow());
    std::io::stdin().read_line(&mut username).expect("Failed to read username");
    username = username.trim().to_string();
    print!("{}", "[INFO] Enter email: ".yellow());
    std::io::stdin().read_line(&mut email).expect("Failed to read email");
    email = email.trim().to_string();
    print!("{}", "[INFO] Enter password: ".yellow());
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
        println!("{}", "[INFO] Registration successful. Auth token saved.".green());

        println!("{}", "[INFO] Save info for auto login?".yellow());
        // Save auto login info
        let mut auto_login_choice = String::new();
        std::io::stdin().read_line(&mut auto_login_choice).expect("Failed to read input");
        auto_login_choice = auto_login_choice.trim().to_string();
        if auto_login_choice.trim().eq_ignore_ascii_case("y") {
            let auto_login_data = format!("{},{}", username, password);
            let auto_login_fp = home_dir.join(".archerdndsys/.auto_login.txt");
            std::fs::write(auto_login_fp, auto_login_data)
                .map_err(|e| anyhow::anyhow!("Failed to save auto login token: {}", e))?;
            println!("{}", "[INFO] Auto login info saved.".green());
        } else {
            println!("{}", "[INFO] Auto login info not saved. Try again later.".yellow());
        }

        // Return the token and user id
        Ok((token, user_id))
    } else {
        let error_text = response.text().await?;
        println!("{}", "[ERROR] Registration failed.".red());
        println!("{} {}", "[ERROR] Response: ".red(), error_text);
        Err(anyhow::anyhow!("Registration failed: {}", error_text))
    }
}

pub async fn logout() -> Result<(), anyhow::Error> {
    check_setup_cmpl()?;
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let auth_file_path = home_dir.join(".archerdndsys/.auth_tokens.txt");

    println!("{}", "[INFO] Logging out...".yellow());
    // Send logout request to server
    let client = reqwest::Client::new();
    let response = client.post(format!("{}/auth/logout", SERVER))
        .send()
        .await?;
    if response.status().is_success() {
        println!("{}", "[INFO] Logout request sent successfully.".green());
    } else {
        let error_text = response.text().await?;
        println!("{}", "[ERROR] Logout failed.".red());
        println!("{} {}", "[ERROR] Response: ".red(), error_text);
        return Err(anyhow::anyhow!("Logout failed: {}", error_text));
    }

    if auth_file_path.exists() {
        std::fs::remove_file(auth_file_path)
            .map_err(|e| anyhow::anyhow!("Failed to remove auth token file: {}", e))?;
        println!("{}", "[INFO] Logout successful. Auth token removed.".green());
    } else {
        println!("{}", "[INFO] No auth token found. Already logged out.".yellow());
    }

    Ok(())
}

pub async fn is_signed_in() -> bool {
    let home_dir = dirs::home_dir().unwrap_or_else(|| {
        println!("{}", "[ERROR] Could not find home directory.".red());
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
                            println!("{}", "[INFO] User is signed in.".green());
                            true
                        } else {
                            println!("{}", "[ERROR] User is not signed in.".red());
                            false
                        }
                    } else {
                        println!("{}", "[ERROR] Invalid auth token format.".red());
                        false
                    }
                } else {
                    println!("{}", "[ERROR] Invalid auth token format.".red());
                    false
                }
            },
            Err(e) => {
                println!("{} {}", "[ERROR] Failed to read auth token file:".red(), e);
                false
            }
        }
    } else {
        false
    }
}
