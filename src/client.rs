use crate::check_setup_cmpl;
use crossterm::style::Stylize;
use tokio::sync::Semaphore;
use std::sync::Arc;
use futures::future::join_all;
use reqwest::Client;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use anyhow;

/// Parses a line like: "POST /api/resource {"key":"value"}"
pub fn parse_line(line: &str) -> Option<(String, String, Option<String>)> {
    let parts: Vec<&str> = line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return None;
    }
    let method = parts[0].to_string();
    let endpoint = parts[1].to_string();
    let data = if parts.len() == 3 {
        Some(parts[2].to_string())
    } else {
        None
    };
    Some((method, endpoint, data))
}


/**
 * Go through and delete any calls that are redundant
 * Examples:
 * Multiple updates on the same resource - keep last update only
 * Multiple deletes on the same resource - keep any delete
 * Multiple creates on the same resource - keep last create only
 * The following are in order possible combination operations on the same resource
 * Create + Update = Create with updated changes, remove update
 * Create + Delete = remove both
 * Delete + Create = keep both
 * Update + Delete = keep delete, remove update
 * Update + Create = error, stop and query user to resolve
 * etc.
 * Algo:
    * 1. Read session_calls.txt line by line
    * 2. Parse each line to identify operation type and resource
    * 3. Maintain a map of resources to their latest operation (map<resource, operation_stack>)
    * 4. For each new operation, check if it conflicts with the latest operation for that resource
    * 5. If it conflicts, resolve based on the rules above
    * 6. Write the resolved operations back to session_calls.txt
**/
pub fn clean_session_calls(session_calls: PathBuf) -> Result<(), anyhow::Error> {
    let session_calls_path = session_calls;
    if !session_calls_path.exists() {
        return Ok(()); // Nothing to clean if the file doesn't exist
    }

    let contents = fs::read_to_string(&session_calls_path)?;
    let mut ops_by_resource: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for line in contents.lines() {
        if let Some((method, endpoint, data)) = parse_line(line) {
            let key = endpoint.clone();
            let stack = ops_by_resource.entry(key).or_default();

            match method.as_str() {
                "GET" => continue, // Skip GETs as they don't modify state
                "POST" => {
                    if let Some((prev_op, _)) = stack.last() {
                        match prev_op.as_str() {
                            "POST" => { stack.pop(); } // keep latest create only
                            "DELETE" => {} // DELETE + POST = valid
                            "PUT" => {
                                return Err(anyhow::anyhow!(
                                    "Invalid sequence: UPDATE followed by CREATE for {}",
                                    endpoint
                                ));
                            }
                            _ => {}
                        }
                    }
                    stack.push((method, line.to_string()));
                }
                "PUT" => {
                    if let Some((prev_op, _)) = stack.last() {
                        match prev_op.as_str() {
                            "POST" => {
                                stack.pop(); // Create + Update = just Create with new data
                                stack.push((method, line.to_string()));
                            }
                            "PUT" => {
                                stack.pop(); // Keep only last update
                                stack.push((method, line.to_string()));
                            }
                            "DELETE" => {
                                stack.pop(); // DELETE + UPDATE = just DELETE
                                stack.push(("DELETE".to_string(), format!("DELETE {}", endpoint)));
                            }
                            _ => {
                                stack.push((method, line.to_string()));
                            }
                        }
                    } else {
                        stack.push((method, line.to_string()));
                    }
                }
                "DELETE" => {
                    stack.clear(); // Remove all previous ops and keep DELETE only
                    stack.push((method, line.to_string()));
                }
                _ => {}
            }
        }
    }

    let mut cleaned_lines = Vec::new();
    for stack in ops_by_resource.values() {
        for (_, line) in stack {
            cleaned_lines.push(line.clone());
        }
    }

    fs::write(&session_calls_path, cleaned_lines.join("\n"))?;

    Ok(())
}

pub fn collect_session_calls(fp: PathBuf) -> Result<Vec<Vec<String>>, anyhow::Error> {
    let session_calls_path = fp;
    if !session_calls_path.exists() && !session_calls_path.is_file() {
        return Err(anyhow::anyhow!("[ERROR] Session calls file does not exist: {}", session_calls_path.display()));
    }
    // Read file line by line
    let contents = fs::read_to_string(&session_calls_path)?;
    let mut calls: Vec<Vec<String>> = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if !line.is_empty() {
            let parts: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
            if parts.len() >= 2 {
                calls.push(parts);
            } else {
                return Err(anyhow::anyhow!("[ERROR] Invalid session call format: {}", line));
            }
        }
    }

    Ok(calls)
}

pub async fn calculate_cache_size() -> Result<(u64), anyhow::Error> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        anyhow::anyhow!("Could not find home directory")
    })?;
    let archerdndsys_dir = home_dir.join(".archerdndsys");
    let saved_objs_dir = archerdndsys_dir.join("saved_objs");
    
    if !saved_objs_dir.exists() {
        return Err(anyhow::anyhow!("[ERROR] Saved objects directory does not exist: {}", saved_objs_dir.display()));
    }
    
    let mut total_size = 0;
    for entry in fs::read_dir(saved_objs_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let subdir_path = entry.path();
            if subdir_path.is_dir() {
                for sub_entry in fs::read_dir(subdir_path)? {
                    let sub_entry = sub_entry?;
                    if sub_entry.file_type()?.is_file() {
                        total_size += sub_entry.metadata()?.len();
                        println!("[INFO] {} {}: {} bytes", "Found file:".green(), sub_entry.path().display().to_string().bold(), sub_entry.metadata()?.len());
                    }
                }
            }
        }
        if entry.file_type()?.is_file() {
            total_size += entry.metadata()?.len();
        }
    }
    
    Ok(total_size)
}

pub async fn clear_cache(days: u64) -> Result<(), anyhow::Error> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        anyhow::anyhow!("Could not find home directory")
    })?;
    let archerdndsys_dir = home_dir.join(".archerdndsys");
    let saved_objs_dir = archerdndsys_dir.join("saved_objs");

    if !saved_objs_dir.exists() {
        return Err(anyhow::anyhow!("[ERROR] {} {}", "Saved objects directory does not exist:".red(), saved_objs_dir.display().to_string().bold()));
    }

    let cutoff_time = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 24 * 60 * 60);
    
    // Check last modified/created/accessed time of each file
    for dir in fs::read_dir(saved_objs_dir)? {
        let dir = dir?;
        if dir.file_type()?.is_file() {
            let path = dir.path();
            fs::remove_file(&path)?;
            println!("[INFO] {} {}", "Deleted file:".green(), path.display().to_string().bold());
            continue;
        }
        for entry in fs::read_dir(dir.path())? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let modified_time = metadata.modified()?;
            let created_time = metadata.created()?;
            let accessed_time = metadata.accessed()?;
            let should_delete = modified_time < cutoff_time && created_time < cutoff_time && accessed_time < cutoff_time;

            if should_delete {
                let path = entry.path();
                if path.is_file() {
                    fs::remove_file(&path)?;
                    println!("[INFO] {} {}", "Deleted file:".green(), path.display().to_string().bold());
                } else if path.is_dir() {
                    println!("[ERROR] {} {}", "Unexpected directory in saved_objs:".red(), path.display().to_string().bold());
                }
            }
        }
    }
    
    Ok(())
}

pub async fn clear_all_cache() -> Result<(), anyhow::Error> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        anyhow::anyhow!("Could not find home directory")
    })?;
    
    let archerdndsys_dir = home_dir.join(".archerdndsys");
    let saved_objs_dir = archerdndsys_dir.join("saved_objs");
    
    if !saved_objs_dir.exists() {
        return Err(anyhow::anyhow!("[ERROR] {} {}", "Saved objects directory does not exist:".red(), saved_objs_dir.display().to_string().bold()));
    }
    
    for dir in fs::read_dir(saved_objs_dir)? {
        let dir = dir?;
        if !dir.file_type()?.is_dir() {
            let path = dir.path();
            fs::remove_file(&path)?;
            println!("[INFO] {} {}", "Deleted file:".green(), path.display().to_string().bold());
        }
        let dir_path = dir.path();
        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                fs::remove_file(&path)?;
                println!("[INFO] {} {}", "Deleted file:".green(), path.display().to_string().bold());
            } else if path.is_dir() {
                fs::remove_dir_all(&path)?;
                println!("[INFO] {} {}", "Deleted directory:".green(), path.display().to_string().bold());
            }
        }
    }
    
    Ok(())
}

pub async fn load_auth_tokens() -> Result<(String, String), anyhow::Error> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        anyhow::anyhow!("Could not find home directory")
    })?;
    let auth_tokens_path = home_dir.join(".archerdndsys/.auth_tokens.txt");


    if !auth_tokens_path.exists() {
        return Err(anyhow::anyhow!("[ERROR] Authorization tokens file not found. Please run `archerdndsys --setup` to initialize the client."));
    }

    let contents = fs::read_to_string(auth_tokens_path)?;
    let mut lines = contents.lines();

    let access_token = lines.next().ok_or_else(|| {
        anyhow::anyhow!("Access token not found in auth tokens file.")
    })?.to_string();

    let refresh_token = lines.next().ok_or_else(|| {
        anyhow::anyhow!("Refresh token not found in auth tokens file.")
    })?.to_string();

    Ok((access_token, refresh_token))
}

pub async fn process_call(call: Vec<String>, client: Arc<Client>, authTokens: (String, String)) -> Result<(), anyhow::Error> {
    if call.len() < 2 {
        return Err(anyhow::anyhow!("[ERROR] Invalid session call format: {:?}", call));
    }

    let method = call[0].clone();
    let endpoint = call[1].clone();

    match method.as_str() {
        "POST" => {
            let data = call.get(2).cloned().unwrap_or_default();
            let response = client.post(&endpoint)
                .bearer_auth(&authTokens.0)
                .json(&data)
                .send()
                .await?;
            if response.status().is_success() {
                println!("{} {} {}", "[INFO] POST request to".green(), endpoint.bold(), "succeeded.".green());
            } else {
                println!("{} {} {} {}", "[ERROR] POST request to".red(), endpoint.bold(), "failed with status:".red(), response.status());
            }
        },
        "PUT" => {
            let data = call.get(2).cloned().unwrap_or_default();
            let response = client.put(&endpoint)
                .bearer_auth(&authTokens.0)
                .json(&data)
                .send()
                .await?;
            if response.status().is_success() {
                println!("[INFO] PUT request to {} succeeded.", endpoint);
            } else {
                println!("[ERROR] PUT request to {} failed with status: {}", endpoint, response.status());
            }
        },
        "DELETE" => {
            let response = client.delete(&endpoint)
                .bearer_auth(&authTokens.0)
                .send()
                .await?;
            if response.status().is_success() {
                println!("[INFO] DELETE request to {} succeeded.", endpoint);
            } else {
                println!("[ERROR] DELETE request to {} failed with status: {}", endpoint, response.status());
            }
        },
        _ => return Err(anyhow::anyhow!("[ERROR] Unsupported HTTP method: {}", method))
    }

    Ok(())
}

// All Campaign Commands can be made directly to the server; rate limit to 15 per minute.
