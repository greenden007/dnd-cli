use std::collections::BinaryHeap;
use std::sync::Arc;
use crossterm::style::Stylize;
use reqwest::Client;
use tokio::sync::Semaphore;

pub mod auth;
pub mod ui;
pub mod client;

pub static LOCAL_ID: u32 = 0; // This is a placeholder for local ID management, can be used to track unsynced items
pub static mut CACHE: BinaryHeap<String> = BinaryHeap::new(); // This will be used to cache the local items

pub fn error_string(msg: &str) -> String {
    format!("{}: {}", "[ERROR]".red(), msg)
}

pub fn info_string(msg: &str) -> String {
    format!("{}: {}", "[INFO]".green(), msg)
}

pub fn warning_string(msg: &str) -> String {
    format!("{}: {}", "[WARN]".yellow(), msg)
}

pub fn debug_string(msg: &str) -> String {
    format!("{}: {}", "[DEBUG]".cyan(), msg)
}

pub fn trace_string(msg: &str) -> String {
    format!("{}: {}", "[TRACE]".grey(), msg)
}

pub const setup_string: &str = "Please run `archerdndsys setup` to initialize the client.";

pub const SERVER: &str = "https://archerdnd.tech/api";
pub const REQ_FILES: [&str; 18] = [
    "saved_objs/",
    ".auth_tokens.txt",
    ".auto_login.txt",
    "synced.txt",
    "saved_objs/Characters/",
    "saved_objs/Classes/",
    "saved_objs/Features/",
    "saved_objs/Items/",
    "saved_objs/Races/",
    "saved_objs/Spells/",
    "saved_objs/Subclasses/",
    "saved_objs/Characters/session_calls.txt",
    "saved_objs/Classes/session_calls.txt",
    "saved_objs/Features/session_calls.txt",
    "saved_objs/Items/session_calls.txt",
    "saved_objs/Races/session_calls.txt",
    "saved_objs/Spells/session_calls.txt",
    "saved_objs/Subclasses/session_calls.txt",
];

pub fn check_setup_cmpl() -> Result<(), clap::Error> {
    let home_dir = dirs::home_dir().ok_or_else(|| clap::Error::raw(
        clap::error::ErrorKind::Io,
        "Could not find home directory",
    ))?;
    let archerdndsys_dir = home_dir.join(".archerdndsys");

    if !archerdndsys_dir.exists() {
        return Err(clap::Error::raw(
            clap::error::ErrorKind::Io,
            setup_string,
        ));
    }

    for file in &REQ_FILES {
        let file_path = archerdndsys_dir.join(file);
        if !file_path.exists() {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::Io,
                format!("Required file '{}' not found. {}", file, setup_string),
            ));
        }
    }

    Ok(())
}

/** Formatting guidelines for local get/set
* filenames: saved_objs/(item type)/(_id).json
* if item has not been pushed to server it will be saved in "saved_objs/unsynced/(item type)_(x).json"
*                                  where x is the number of unsynced items of that type saved locally
*
**/

/**
 * session_calls.txt formatting line-by-line:
 * [OPERATION] (GET/POST/PUT/DELETE) [SERVER_ENDPOINT] [RESOURCES] (json data)
**/
pub async fn push_load() -> Result<(), anyhow::Error> {
    // First, clean all session calls
    for i in 11..REQ_FILES.len() {
        let file_path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".archerdndsys")
            .join(REQ_FILES[i]);
        client::clean_session_calls(file_path)?
    }

    // Preload authorization tokens

    let auth_tokens_path = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
        .join(".archerdndsys")
        .join(".auth_tokens.txt");
    if !auth_tokens_path.exists() {
        return Err(anyhow::anyhow!("Authorization tokens file not found. Please run `archerdndsys --setup` to initialize the client."));
    }

    let auth_tokens = client::load_auth_tokens().await?;

    // Initialize the HTTP client and semaphore
    // Open 4 threads to parse files and push to server
    // Local-only Files will be in form of {LOCAL_ID}.json, Files which are already on the server will be in form of _{MONGOOSE_ID}.json
    // Create map of {LOCAL_ID: MONGOOSE_ID} for each new value pushed to the server
    // Create list of remaining requests to be made
    // Keep making requests within the following parameters:
    // - IF the request contains no LOCAL_ID, it can be pushed immediately
    // - IF the request contains a (or multiple) LOCAL_ID, it must be checked against the map
    //   - IF the LOCAL_ID is not in the map, then add the request to the list of remaining requests. Then continue to next available entry.
    //   - IF the LOCAL_ID is in the map, then replace the LOCAL_ID with the MONGOOSE_ID and push the request immediately

    // Run as 6 threads with semaphore to limit concurrency
    // Each file is in the format [OPERATION] [SERVER_ENDPOINT] [RESOURCES] (json data)

    let client = Arc::new(Client::new());
    let semaphore = Arc::new(Semaphore::new(6));
    let mut tasks = Vec::new();
    for i in 0..REQ_FILES.len() {
        let file_path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".archerdndsys")
            .join(REQ_FILES[i]);

        if file_path.exists() {
            let client_clone = Arc::clone(&client);
            let semaphore_clone = Arc::clone(&semaphore);
            let auth_tokens_clone = auth_tokens.clone();
            let file_path_clone = file_path.clone();

            tasks.push(tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await.unwrap();
                let file_path_str = file_path_clone.display().to_string();
                match client::collect_session_calls(file_path_clone) {
                    Ok(calls) => {
                        for call in calls {
                            if let Err(e) = client::process_call(call, Arc::clone(&client_clone), auth_tokens_clone.clone()).await {
                                eprintln!("Error processing call from {}: {}", file_path_str, e);
                            }
                        }
                    },
                    Err(e) => eprintln!("Error reading session calls from {}: {}", file_path_str, e),
                }
            }))
        }
    }

    Ok(())
}

pub unsafe fn ready_cache() -> Result<(), anyhow::Error> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let sync_file_path = home_dir.join(".archerdndsys").join("saved_objs").join("synced.txt");
    if !sync_file_path.exists() {
        return Err(anyhow::anyhow!("Synced file not found. Please run `archerdndsys --setup` to initialize the client."));
    }

    // Load the cache from the synced file
    let synced_data = std::fs::read_to_string(sync_file_path)
        .map_err(|e| anyhow::anyhow!("Failed to read synced file: {}", e))?;

    let mut synced_items: Vec<String> = synced_data.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .collect();

    synced_items.remove(0);

    for item in synced_items.iter() {
        CACHE.push(item.clone());
    }

    Ok(())
}
