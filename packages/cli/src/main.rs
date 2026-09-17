//! `cb` 进程入口：只做进程级初始化，然后把参数交给 `cmd::dispatch`。
//!
//! # 分层（依赖只能向下，见 DESIGN.md）
//!
//! ```text
//! main + cli        进程入口 / clap 契约
//!   └─ cmd          接口层：子命令处理器，负责终端交互与展示
//!        └─ pipeline 应用层：语音对话编排（录音 → ASR → 唤醒判定 → LLM → TTS → 播放）
//!             ├─ domain    领域层：纯逻辑、零 IO（唤醒词、分句、指标、semver、健康规则）
//!             └─ speech / llm / audio / config / storage / platform   基础设施层
//!   ui              横切的展示工具（主题、横幅、spinner、i18n），domain 之外都可用
//! ```
mod audio;
mod cli;
mod cmd;
mod config;
mod domain;
mod llm;
mod pipeline;
mod platform;
mod speech;
mod storage;
mod ui;

use clap::Parser;

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
    let cli = cli::Cli::parse();

    if cli.debug {
        std::env::set_var("CB_DEBUG", "1");
    }

    cmd::dispatch(cli).await
}
