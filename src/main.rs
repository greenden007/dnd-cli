mod auth;
mod ui;
mod client;

use clap::{Parser, Subcommand};
use anyhow;
use crossterm::event::read;
use crossterm::style::Stylize;
use archerdndsys::{push_load, check_setup_cmpl, REQ_FILES, SERVER, error_string, info_string, warning_string, debug_string, trace_string, ready_cache, setup_string};

#[derive(Parser)]
#[command(name = "archerdndsys", about = "A client for the Archer RPG System")]
#[command(version = "0.1.0", term_width = 80)]
#[command(author = "Lockie", long_about = "A client for the Archer RPG System. \n\nThis client allows you to manage your characters, campaigns, and other data for the Archer RPG System. It provides a command line interface to interact with the server and manage your data.\n\nWhen invoked with no flags, it will list all saved data the user has locally by filenames. \n\nTo get started, run `archerdndsys setup` to initialize the client.")]
enum Cli {
    /// Setup the client with initial configuration
    Setup,

    /// Manually login to the client
    #[command(visible_alias = "l")]
    Login,

    /// Register a new user
    #[command(visible_alias = "r")]
    Register,

    /// Use auto login if user wants to
    #[command(visible_alias = "a")]
    AutoLogin,

    /// Logout of the client
    #[command(visible_alias = "L")]
    Logout,

    /// Run the TUI client with subcommands
    #[command(subcommand)]
    Run(RunCommands),

    /// Check if setup is complete
    #[command(visible_alias = "c")]
    CheckSetup,

    /// Push all server calls to the server and update the database
    #[command(visible_alias = "u")]
    PushLoad,

    /// Calculate the total size of the cached objects
    #[command(visible_alias = "S")]
    CacheSize,

    /// Clear all saved data not accessed in the last [argument] days.
    /// If argument is 0 all cache will be cleared.
    #[command(visible_alias = "X")]
    ClearCache {
        /// Number of days since last access; 0 clears all cached data
        #[arg(value_name = "DAYS", help = "Number of days since last access; 0 clears all cached data")]
        days: u64,
    },

    /// List all saved data the user has locally by filenames
    #[command(visible_alias = "ls")]
    ListLocal,
}

#[derive(Subcommand)]
enum RunCommands {

    /// Create a new object of whatever type
    #[command(name = "make", visible_aliases = &["-m"])]
    Make,

    /// Edit an existing object
    #[command(name = "edit", visible_aliases = &["-e"])]
    Edit,

    /// Delete an existing object
    #[command(name = "delete", visible_aliases = &["-d"])]
    Delete,

    /// List all objects of a type
    #[command(name = "list", visible_aliases = &["-l"])]
    List,

    /// Sync local changes with server
    #[command(name = "sync", visible_aliases = &["-s"])]
    Sync,

    /// Get an object by ID
    #[command(name = "get", visible_aliases = &["-g"])]
    Get,

    /// View an object
    #[command(name = "view", visible_aliases = &["-v"])]
    View,

    /// Push all server calls to the server and update the database
    #[command(name = "push", visible_aliases = &["-p"])]
    Push,
}

async fn client_init_startup() -> Result<(), clap::Error> {

    println!("{}", info_string("Initializing client..."));

    let home_dir = dirs::home_dir().ok_or_else(|| clap::Error::raw(
        clap::error::ErrorKind::Io,
        "Could not find home directory",
    ))?;

    let archerdndsys_dir = home_dir.join(".archerdndsys");
    println!("{}", info_string("Checking for archerdndsys management directory"));

    if !archerdndsys_dir.exists() {
        println!("{}", info_string("Directory not found. Creating archerdndsys management directory..."));
        std::fs::create_dir_all(&archerdndsys_dir).map_err(|e| {
            println!("{}", error_string(&format!("Failed to create directory: {}", e)));
            clap::Error::raw(clap::error::ErrorKind::Io, format!("Failed to create directory: {}", e))
        })?;
    }

    println!("{}", info_string("Directory found."));
    println!("{}", info_string("Checking for required files..."));

    for file in &REQ_FILES {
        let file_path = archerdndsys_dir.join(file);
        println!("{}", info_string(&format!("Checking for file: {}", file.bold())));
        if !file_path.exists() {
            println!("{}", info_string(&format!("File not found. Creating: {}", file.bold())));
            if file.ends_with('/') {
                println!("{}", info_string(&format!("Creating directory: {}", file.bold())));
                std::fs::create_dir_all(&file_path).map_err(|e| {
                    println!("{}", error_string(&format!("Failed to create directory: {}", e)));
                    clap::Error::raw(clap::error::ErrorKind::Io, format!("Failed to create directory: {}", e))
                })?;
            } else {
                println!("{}", info_string(&format!("Creating file: {}", file.bold())));
                std::fs::File::create(&file_path).map_err(|e| {
                    println!("{}", error_string(&format!("Failed to create file: {}", e)));
                    clap::Error::raw(clap::error::ErrorKind::Io, format!("Failed to create file: {}", e))
                })?;
            }
        }
    }

    println!("{}", info_string("Client initialization complete."));
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Cli::parse();

    match args {
        Cli::Setup => {
            if let Err(e) = client_init_startup().await {
                println!("{}", error_string(&format!("Client setup failed: {}", e)));
                return Ok(());
            } else {
                println!("{}", info_string("Client setup complete."));
                return Ok(());
            }
        }
        Cli::CheckSetup => {
            if let Err(e) = check_setup_cmpl() {
                println!("{}", error_string(&format!("Setup incomplete or absent: {}", e)));
                return Ok(());
            } else {
                println!("{}", info_string("Setup is complete."));
                return Ok(());
            }
        }
        Cli::Login => {
            if let Err(e) = auth::manual_login().await {
                println!("{}", error_string(&format!("Manual login failed: {}", e)));
                return Ok(());
            } else {
                println!("{}", info_string("Manual login successful."));
                return Ok(());
            }
        }
        Cli::Register => {
            if let Err(e) = auth::register().await {
                println!("{}", error_string(&format!("Registration failed: {}", e)));
                return Ok(());
            } else {
                println!("{}", info_string("Registration successful."));
                return Ok(());
            }
        }
        Cli::AutoLogin => {
            if let Err(e) = auth::auto_login().await {
                println!("{}", error_string(&format!("Auto login failed: {}", e)));
                return Ok(());
            } else {
                println!("{}", info_string("Auto login successful."));
                return Ok(());
            }
        }
        Cli::Logout => {
            if let Err(e) = auth::logout().await {
                println!("{}", error_string(&format!("Logout failed: {}", e)));
                return Ok(());
            } else {
                println!("{}", info_string("Logout successful."));
                return Ok(());
            }
        }
        Cli::PushLoad => {
            if let Err(e) = push_load().await {
                println!("{}", error_string(&format!("Push and load failed: {}", e)));
                return Ok(());
            } else {
                println!("{}", info_string("Push and load successful."));
                return Ok(());
            }
        }
        Cli::Run(subcmd) => unsafe {
            if !check_setup_cmpl().is_ok() {
                println!("{}", error_string("Setup is incomplete or absent. Please run `archerdndsys setup` to initialize the client."));
                return Ok(());
            }

            // Check if signed in
            if !auth::is_signed_in().await {
                println!("{}", error_string("You must be signed in to run the client."));
                return Ok(());
            }

            ready_cache()?;

            // TODO: Implement the TUI client
            ui::run_ui().await?;
        }
        Cli::CacheSize => {
            // Calculate the total size of the cached objects
            let size = client::calculate_cache_size().await;
            match size {
                Ok(size) => {
                    println!("{}", info_string(&format!("Total cache size: {} bytes", size.to_string().bold())));
                },
                Err(e) => {
                    println!("{}", error_string(&format!("Failed to calculate cache size: {}", e)));
                }
            }
            return Ok(());
        }
        Cli::ClearCache { days } => {
            if days == 0 {
                if let Err(e) = client::clear_all_cache().await {
                    println!("{}", error_string(&format!("Cache clearing failed: {}", e)));
                    return Ok(())
                } else {
                    println!("{}", info_string("All cache cleared."));
                    return Ok(())
                }
            }

            // Clear all saved data not accessed in the last [argument] days
            if let Err(e) = client::clear_cache(days).await {
                println!("{}", error_string(&format!("Cache clearing failed: {}", e)));
                return Ok(());
            } else {
                println!("{}", info_string("Cache clearing complete."));
                return Ok(());
            }
        }
        Cli::ListLocal => {
            // List all saved data the user has locally by filenames
            check_setup_cmpl()?;
            let home_dir = dirs::home_dir().ok_or_else(|| clap::Error::raw(
                clap::error::ErrorKind::Io,
                "Could not find home directory",
            ))?;

            let saved_objs_dir = home_dir.join(".archerdndsys/saved_objs");
            if saved_objs_dir.exists() {
                println!("{}", info_string("Saved objects directory found."));
                // List all files in saved_objs subdirectories (there should be no direct files in this directory)
                // Divide by subdirectories (characters, campaigns, etc.)
                if let Ok(entries) = std::fs::read_dir(&saved_objs_dir) {
                    println!("{}", info_string("Directory found, listing saved objects:"));
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
                                            println!("{}", error_string("Could not read file"));
                                        }
                                    }
                                } else {
                                    println!("{}", error_string("Could not read directory"));
                                }
                            } else {
                                println!("{}", info_string(&format!("File: {}", path.display())));
                            }
                        } else {
                            println!("{}", error_string("Could not read entry in saved objects directory"));
                        }
                    }
                } else {
                    println!("{}", error_string("Could not read saved objects directory"));
                }
            }

            return Ok(());
        }
    }

    Ok(())
}
