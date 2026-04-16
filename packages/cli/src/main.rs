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
    /// Start voice chat, or send a text message directly
    Chat {
        /// Text to send (skip voice input). e.g. `cb chat hello`
        message: Vec<String>,
    },
    /// Install as system daemon (launchd / systemd)
    Install,
    /// Uninstall daemon and remove service files
    Uninstall,
    /// Show daemon service status
    Status,
    /// Open local web dashboard
    Open,
    /// View conversation event logs
    Logs {
        /// Follow log output in real-time
        #[arg(short, long)]
        follow: bool,
        /// View logs for a specific date (YYYY-MM-DD)
        #[arg(short, long)]
        date: Option<String>,
    },
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
    /// Set a configuration value (e.g. `cb config set llm.model gpt-4o`)
    Set {
        /// Config key
        key: String,
        /// Config value
        value: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ui::theme::init_colors();
    config::migrate_config_path();
    let cli = Cli::parse();

    if cli.debug {
        std::env::set_var("CB_DEBUG", "1");
    }

    match cli.command {
        None => cmd::chat::run_voice(cli.debug).await,
        Some(Commands::Chat { message }) => {
            if message.is_empty() {
                cmd::chat::run_voice(cli.debug).await
            } else {
                cmd::chat::run_text(&message.join(" "), cli.debug).await
            }
        }
        Some(Commands::Install) => cmd::install::run().await,
        Some(Commands::Uninstall) => cmd::install::uninstall().await,
        Some(Commands::Status) => cmd::install::status().await,
        Some(Commands::Open) => cmd::open::run().await,
        Some(Commands::Logs { follow, date }) => cmd::logs::run(follow, date).await,
        Some(Commands::Config { action }) => match action {
            None => cmd::config::run_wizard().await,
            Some(ConfigAction::Show) => cmd::config::show(),
            Some(ConfigAction::Set { key, value }) => cmd::config::set(&key, &value),
        },
    }
}
