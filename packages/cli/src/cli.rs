//! 接口契约 · 命令行参数定义
//!
//! 只有 clap 结构体，不含任何行为。`cmd::dispatch` 负责把这里的枚举路由到处理器。
use clap::{Parser, Subcommand};

#[derive(Parser)]
// `bin_name` forces the Usage: line to render "cb" regardless of argv[0],
// otherwise the npm wrapper's `cb-darwin` / `cb-linux-x64` shim names leak
// into help output.
#[command(name = "cb", bin_name = "cb", version, about = "Cross-platform voice assistant CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Enable debug logging
    #[arg(long, global = true)]
    pub debug: bool,
}

#[derive(Subcommand)]
pub enum Commands {
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
pub enum ConfigAction {
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
