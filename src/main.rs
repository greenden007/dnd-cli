mod auth;
mod ui;
mod client;
mod config;
mod transport;
mod cache;

use clap::{Parser, Subcommand, ValueEnum};
use anyhow;
use crossterm::style::Stylize;
use archerdndsys::{push_load, check_setup_cmpl, REQ_FILES, error_string, info_string, ready_cache};
pub use archerdndsys::{debug_string, setup_string, trace_string, warning_string};
use archerdndsys::config::{data_dir, load_profile_store, save_named_profile, set_active_profile, ServerProfile};

#[derive(Parser)]
#[command(name = "archerdndsys", about = "A client for the Archer RPG System")]
#[command(version = "0.1.0", term_width = 80)]
#[command(author = "Lockie", long_about = "A client for the Archer RPG System. \n\nThis client allows you to manage your characters, campaigns, and other data for the Archer RPG System. It provides a command line interface to interact with the server and manage your data.\n\nWhen invoked with no flags, it will list all saved data the user has locally by filenames. \n\nTo get started, run `archerdndsys setup` to initialize the client.")]
enum Cli {
    /// Setup the client with initial configuration
    Setup {
        /// Base URL for the server API, for example https://example.com/api
        #[arg(long)]
        server: Option<String>,
        /// Name of the server profile to activate
        #[arg(long, default_value = "default")]
        profile: String,
    },

    /// Manage configured server profiles
    #[command(subcommand)]
    Profile(ProfileCommand),

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

    /// Run the client in the terminal UI or command-line interface
    Run {
        /// Select the interaction mode. `auto` uses TUI with no command and CLI with a command.
        #[arg(long, value_enum, default_value_t = Interface::Auto)]
        interface: Interface,
        #[command(subcommand)]
        command: Option<RunCommands>,
    },

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

    #[command(visible_alias = "i")]
    InPersonEncounter,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum Interface {
    /// Choose TUI for an interactive session and CLI for a supplied subcommand.
    #[default]
    Auto,
    /// Run a single command and exit.
    Cli,
    /// Open the interactive terminal UI.
    Tui,
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

#[derive(Subcommand)]
enum ProfileCommand {
    /// List configured server profiles
    List,
    /// Activate a configured server profile
    Use { name: String },
}

async fn run_cli_command(command: RunCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        RunCommands::Make => ui::create_object(None).await?,
        RunCommands::Edit => ui::edit_object(None, None).await?,
        RunCommands::Delete => ui::delete_object(None, None).await?,
        RunCommands::List => ui::list_objects(None).await?,
        RunCommands::Sync => ui::sync_objects().await?,
        RunCommands::Get => ui::get_object(None, None).await?,
        RunCommands::View => ui::view_object(None, None).await?,
        RunCommands::Push => push_load().await?,
    }
    Ok(())
}

async fn client_init_startup(server_url: Option<String>, profile_name: String) -> Result<(), clap::Error> {

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

    let configured_url = match server_url {
        Some(url) => url,
        None => {
            print!("Server API URL [https://archerdnd.tech/api]: ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).map_err(|e| clap::Error::raw(
                clap::error::ErrorKind::Io,
                format!("Failed to read server URL: {e}"),
            ))?;
            let trimmed = input.trim();
            if trimmed.is_empty() { "https://archerdnd.tech/api".to_string() } else { trimmed.to_string() }
        }
    };
    let profile = ServerProfile::new(configured_url).map_err(|e| clap::Error::raw(
        clap::error::ErrorKind::InvalidValue,
        e.to_string(),
    ))?;
    save_named_profile(&profile_name, &profile, true).map_err(|e| clap::Error::raw(
        clap::error::ErrorKind::Io,
        format!("Failed to save server profile: {e}"),
    ))?;
    let data_root = data_dir().map_err(|e| clap::Error::raw(
        clap::error::ErrorKind::Io,
        format!("Failed to determine profile data directory: {e}"),
    ))?;
    std::fs::create_dir_all(&data_root).map_err(|e| clap::Error::raw(
        clap::error::ErrorKind::Io,
        format!("Failed to create profile data directory: {e}"),
    ))?;
    println!("{} Using server profile '{}' at {}", info_string("Server configured."), profile_name, profile.base_url);

    for file in &REQ_FILES {
        let file_path = data_root.join(file);
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
        Cli::Setup { server, profile } => {
            if let Err(e) = client_init_startup(server, profile).await {
                println!("{}", error_string(&format!("Client setup failed: {}", e)));
                return Ok(());
            } else {
                println!("{}", info_string("Client setup complete."));
                return Ok(());
            }
        }
        Cli::Profile(ProfileCommand::List) => {
            let store = load_profile_store()?;
            for (name, profile) in store.profiles {
                let marker = if name == store.active { "*" } else { " " };
                println!("{} {} -> {}", marker, name, profile.base_url);
            }
        }
        Cli::Profile(ProfileCommand::Use { name }) => {
            set_active_profile(&name)?;
            std::fs::create_dir_all(data_dir()?)?;
            println!("{} Active server profile: {}", info_string("Profile selected."), name);
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
        Cli::Run { interface, command } => {
            if !check_setup_cmpl().is_ok() {
                println!("{}", error_string("Setup is incomplete or absent. Please run `archerdndsys setup` to initialize the client."));
                return Ok(());
            }

            let open_tui = match interface {
                Interface::Tui => true,
                Interface::Cli => false,
                Interface::Auto => command.is_none(),
            };

            if open_tui {
                if command.is_some() {
                    println!("{}", error_string("The TUI interface does not accept a run subcommand."));
                    return Ok(());
                }
                if !auth::is_signed_in().await {
                    println!("{}", error_string("You must be signed in to run the client."));
                    return Ok(());
                }
                unsafe { ready_cache()?; }
                ui::run_ui().await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
            } else if let Some(command) = command {
                if matches!(command, RunCommands::Sync | RunCommands::Push) && !auth::is_signed_in().await {
                    println!("{}", error_string("You must be signed in to run this command."));
                    return Ok(());
                }
                run_cli_command(command).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
            } else {
                println!("{}", error_string("The CLI interface requires a run subcommand, for example `run --interface cli list`."));
            }
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

            let saved_objs_dir = data_dir()?.join("saved_objs");
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
        _ => {
            println!("{}", error_string("Invalid command. Please run `archerdndsys --help` for usage information."));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn run_without_command_defaults_to_auto_interface() {
        let parsed = Cli::try_parse_from(["archerdndsys", "run"]).unwrap();
        assert!(matches!(parsed, Cli::Run { interface: Interface::Auto, command: None }));
    }

    #[test]
    fn run_command_is_available_through_cli_interface() {
        let parsed = Cli::try_parse_from(["archerdndsys", "run", "--interface", "cli", "list"])
            .unwrap();
        assert!(matches!(
            parsed,
            Cli::Run {
                interface: Interface::Cli,
                command: Some(RunCommands::List)
            }
        ));
    }

    #[test]
    fn tui_interface_can_be_selected_explicitly() {
        let parsed = Cli::try_parse_from(["archerdndsys", "run", "--interface", "tui"])
            .unwrap();
        assert!(matches!(parsed, Cli::Run { interface: Interface::Tui, command: None }));
    }
}
