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
mod update_check;

use clap::{Parser, Subcommand};

#[derive(Parser)]
// `bin_name` forces the Usage: line to render "cb" regardless of argv[0],
// otherwise the npm wrapper's `cb-darwin` / `cb-linux-x64` shim names leak
// into help output.
#[command(name = "cb", bin_name = "cb", version, about = "Cross-platform voice assistant CLI")]
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
    /// Update cb to latest version
    Update {
        /// Force re-download even if already on the latest stable, and
        /// include prereleases (beta) when resolving the latest tag.
        #[arg(short, long)]
        force: bool,
    },
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
    // Rust 1.66+ installs a SIGPIPE handler that aborts with a panic
    // (exit 101) when stdout is closed mid-write. That breaks every
    // `cb config show | head -5` style invocation. Restore the POSIX
    // default so the process exits silently (128 + SIGPIPE = 141).
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

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
        Some(Commands::Update { force }) => cmd::update::run(force).await,
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
