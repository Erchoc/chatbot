mod audio;
mod cmd;
mod config;
pub mod history;
pub mod i18n;
pub mod log;
mod llm;
mod pipeline;
mod speech;
pub mod ui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cb", version, about = "Cross-platform voice assistant CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable debug logging
    #[arg(long, global = true)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start voice chat in foreground (default)
    Chat,
    /// Run as daemon process (alias for `cb install`)
    Up,
    /// Install as system daemon (launchd on macOS, systemd on Linux)
    Install,
    /// Uninstall daemon and remove all service files
    Uninstall,
    /// Show daemon service status
    Status,
    /// Open local web UI
    Open,
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,
    /// Set a configuration value
    Set {
        /// Config key (e.g. llm.model)
        key: String,
        /// Config value
        value: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        std::env::set_var("CB_DEBUG", "1");
    }

    match cli.command {
        None | Some(Commands::Chat) => cmd::chat::run(cli.debug).await,
        Some(Commands::Up) | Some(Commands::Install) => cmd::install::run().await,
        Some(Commands::Uninstall) => cmd::install::uninstall().await,
        Some(Commands::Status) => cmd::install::status().await,
        Some(Commands::Open) => cmd::open::run().await,
        Some(Commands::Config { action }) => match action {
            None => cmd::config::run_wizard().await,
            Some(ConfigAction::Show) => cmd::config::show(),
            Some(ConfigAction::Set { key, value }) => cmd::config::set(&key, &value),
        },
    }
}
