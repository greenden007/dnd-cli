mod auth;
mod ui;
mod client;

use clap::{Parser, Subcommand};
use anyhow;
use crossterm::style::Stylize;
use archerdndsys::{push_load, check_setup_cmpl, REQ_FILES, SERVER};

#[derive(Parser)]
#[command(name = "archerdndsys", about = "A client for the Archer RPG System")]
#[command(version = "0.1.0",term_width = 80)]
#[command(author = "Lockie", long_about = "A client for the Archer RPG System. \n\nThis client allows you to manage your characters, campaigns, and other data for the Archer RPG System. It provides a command line interface to interact with the server and manage your data.\n\nWhen invoked with no flags\
, it will list all saved data the user has locally by filenames. \n\nTo get started, run `archerdndsys --setup` to initialize the client.")]
struct Cli {
    /// Setup the client with initial configuration
    #[arg(short, long)]
    setup: bool,

    /// Manually login to the client
    #[arg(short, long)]
    login: bool,

    /// Register a new user
    #[arg(long)]
    register: bool,

    /// Use auto login if user wants to
    #[arg(long)]
    auto_login: bool,

    /// Run the client as a tui, this is a private environment where the user can interact with the client
    #[arg(short, long)]
    run: bool,

    /// Check if setup is complete
    #[arg(short, long)]
    check_setup: bool,

    /// Logout of the client
    #[arg(long)]
    logout: bool,

    /// Push all server calls to the server and update the database
    #[arg(short, long)]
    push_load: bool,
    
    /// Calculate the total size of the cached objects
    #[arg(short='S', long)]
    cache_size: bool,
    
    /// Clear all saved data not accessed in the last [argument] days.
    /// If argument is 0 all cache will be cleared
    #[arg(short='X', value_name = "DAYS", long)]
    clear_cache: Option<u64>,
}

async fn client_init_startup() -> Result<(), clap::Error> {
    // 1. Check if ~/.archerdndsys/ exists, if not create it
    // 2. If ~/.archerdndsys/ exists, create, or check if following files exist:
    //    a. /session_calls.txt
    //    b. /saved_objs/
    //    c. /.auth_tokens.txt
    //    d. /.session_id.txt

    println!("{}", "[INFO] Initializing client...".yellow());

    let home_dir = dirs::home_dir().ok_or_else(|| clap::Error::raw(
        clap::error::ErrorKind::Io,
        "Could not find home directory",
    ))?;

    let archerdndsys_dir = home_dir.join(".archerdndsys");
    println!("{}", "[INFO] Checking for archerdndsys management directory: ".yellow());

    if !archerdndsys_dir.exists() {
        println!("{}", "[INFO] Directory not found. Creating archerdndsys management directory...".yellow());
        std::fs::create_dir_all(&archerdndsys_dir).map_err(|e| {
            println!("{} {}", "[ERROR] Failed to create directory: ".red(), e);
            clap::Error::raw(clap::error::ErrorKind::Io, format!("Failed to create directory: {}", e))
        })?;
    }

    println!("{}", "[INFO] Directory found.".green());
    println!("{}", "[INFO] Checking for required files...".yellow());

    for file in &REQ_FILES {
        let file_path = archerdndsys_dir.join(file);
        println!("{} {}", "[INFO] Checking for file: ".yellow(), file.bold());
        if !file_path.exists() {
            println!("{} {}", "[INFO] File not found. Creating: ".yellow(), file.bold());
            if file.ends_with('/') {
                println!("{} {}", "[INFO] Creating directory: ".yellow(), file.bold());
                std::fs::create_dir_all(&file_path).map_err(|e| {
                    println!("{} {}", "[ERROR] Failed to create directory: ".red(), e);
                    clap::Error::raw(clap::error::ErrorKind::Io, format!("Failed to create directory: {}", e))
                })?;
            } else {
                println!("{} {}", "[INFO] Creating file: ".yellow(), file.bold());
                std::fs::File::create(&file_path).map_err(|e| {
                    println!("{} {}", "[ERROR] Failed to create file: ".red(), e);
                    clap::Error::raw(clap::error::ErrorKind::Io, format!("Failed to create file: {}", e))
                })?;
            }
        }
    }

    println!("{}", "[INFO] Client initialization complete.".green());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Cli::parse();
    let no_flags = !(args.setup || args.login || args.register || args.auto_login || args.run || args.check_setup || args.logout || args.push_load || args.cache_size || args.clear_cache.is_some());
    if args.setup {
        if let Err(e) = client_init_startup().await {
            println!("{}: {}", "[ERROR] Client setup failed".red(), e);
            return Ok(());
        } else {
            println!("{}", "[INFO] Client setup complete.".green());
            return Ok(());
        }
    }

    if args.check_setup {
        if let Err(e) = check_setup_cmpl() {
            println!("{}: {}", "[ERROR] Setup incomplete or absent".red(), e);
            return Ok(());
        } else {
            println!("{}", "[INFO] Setup is complete.".green());
            return Ok(());
        }
    }

    if args.login {
        if let Err(e) = auth::manual_login().await {
            println!("{}: {}", "[ERROR] Manual login failed".red(), e);
            return Ok(());
        } else {
            println!("{}", "[INFO] Manual login successful.".green());
            return Ok(());
        }
    }

    if args.register {
        if let Err(e) = auth::register().await {
            println!("{}: {}", "[ERROR] Registration failed".red(), e);
            return Ok(());
        } else {
            println!("{}", "[INFO] Registration successful.".green());
            return Ok(());
        }
    }

    if args.auto_login {
        if let Err(e) = auth::auto_login().await {
            println!("{}: {}", "[ERROR] Auto login failed".red(), e);
            return Ok(());
        } else {
            println!("{}", "[INFO] Auto login successful.".green());
            return Ok(());
        }
    }

    if args.logout {
        if let Err(e) = auth::logout().await {
            println!("{}: {}", "[ERROR] Logout failed".red(), e);
            return Ok(());
        } else {
            println!("{}", "[INFO] Logout successful.".green());
            return Ok(());
        }
    }

    if args.push_load {
        if let Err(e) = push_load().await {
            println!("{}: {}", "[ERROR] Push and load failed".red(), e);
            return Ok(());
        } else {
            println!("{}", "[INFO] Push and load successful.".green());
            return Ok(());
        }
    }

    if args.run {
        // Check if signed in
        if !auth::is_signed_in().await {
            println!("{}", "[ERROR] You must be signed in to run the client.".red());
            return Ok(());
        }
        
        // Delete saved_objs/synced.txt if it exists, then create it
        let home_dir = dirs::home_dir().ok_or_else(|| clap::Error::raw(
            clap::error::ErrorKind::Io,
            "Could not find home directory",
        ))?;
        let synced_file = home_dir.join(".archerdndsys/saved_objs/synced.txt");
        if synced_file.exists() {
            std::fs::remove_file(&synced_file).map_err(|e| {
                println!("{}: {}", "[ERROR] Failed to remove synced.txt".red(), e);
                clap::Error::raw(clap::error::ErrorKind::Io, format!("Failed to remove synced.txt: {}", e))
            })?;
        }
        println!("{}", "[INFO] Deleted synced.txt file.".green());
        
        std::fs::File::create(&synced_file).map_err(|e| {
            println!("{}: {}", "[ERROR] Failed to create synced.txt".red(), e);
            clap::Error::raw(clap::error::ErrorKind::Io, format!("Failed to create synced.txt: {}", e))
        })?;
        println!("{}", "[INFO] Created synced.txt file.".green());
        
        // TODO: Implement the TUI client
    }
    
    if args.cache_size {
        // Calculate the total size of the cached objects
       let size = client::calculate_cache_size().await;
        match size {
            Ok(size) => {
                println!("{}: {} bytes", "[INFO] Total cache size".green(), size.to_string().bold());
            },
            Err(e) => {
                println!("{}: {}", "[ERROR] Failed to calculate cache size".red(), e);
            }
        }
        return Ok(());
    }
    
    if args.clear_cache.is_some() {
        if args.clear_cache.unwrap() == 0 {
            if let Err(e) = client::clear_all_cache().await {
                println!("{}: {}", "[ERROR] Cache clearing failed".red(), e);
                return Ok(())
            } else {
                println!("{}", "[INFO] All cache cleared.".green());
                return Ok(())
            }
        }

        // Clear all saved data not accessed in the last [argument] days
        if let Err(e) = client::clear_cache(args.clear_cache.unwrap()).await {
            println!("{}: {}", "[ERROR] Cache clearing failed".red(), e);
            return Ok(());
        } else {
            println!("{}", "[INFO] Cache clearing complete.".green());
            return Ok(());
        }
    }

    if no_flags {

        // List all saved data the user has locally by filenames
        check_setup_cmpl()?;
        let home_dir = dirs::home_dir().ok_or_else(|| clap::Error::raw(
            clap::error::ErrorKind::Io,
            "Could not find home directory",
        ))?;

        let saved_objs_dir = home_dir.join(".archerdndsys/saved_objs");
        if saved_objs_dir.exists() {
            println!("{}", "[INFO] Saved objects directory found.".green());
            // List all files in saved_objs subdirectories (there should be no direct files in this directory)
            // Divide by subdirectories (characters, campaigns, etc.)
            if let Ok(entries) = std::fs::read_dir(&saved_objs_dir) {
                println!("{}", "[INFO] Directory found, listing saved objects:".green());
                for entry_res in entries {
                    if let Ok(entry) = entry_res {
                        let path = entry.path();
                        if path.is_dir() {
                            // Print the directory name
                            let dir_name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("[unnamed]");
                            println!("{}", dir_name.bold().underlined());
                            // List all files in this directory
                            if let Ok(files) = std::fs::read_dir(&path) {
                                for file_res in files {
                                    if let Ok(file) = file_res {
                                        println!("  - {}", file.file_name().to_string_lossy());
                                    } else {
                                        println!("{}", "[ERROR] Could not read file".red());
                                    }
                                }
                            } else {
                                println!("{}", "[ERROR] Could not read directory".red());
                            }
                        } else {
                            println!("{}: {}", "[INFO] File".yellow(), path.display());
                        }
                    } else {
                        println!("{}", "[ERROR] Could not read entry in saved objects directory".red());
                    }
                }
            } else {
                println!("{}", "[ERROR] Could not read saved objects directory".red());
            }
        }
        
        return Ok(());
    }

    Ok(())
}
