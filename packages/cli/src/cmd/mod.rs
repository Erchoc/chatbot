//! 接口层 · 子命令处理器
//!
//! 每个文件对应一个子命令族。处理器只做三件事：读取参数 / 配置、调用下层
//! （`pipeline`、`platform`、`storage`…）、把结果渲染到终端。业务规则不在这里。
pub mod chat;
pub mod config;
pub mod daemon;
pub mod logs;
pub mod open;
pub mod update;

use crate::cli::{Cli, Commands, ConfigAction};

/// 把解析好的命令行路由到对应处理器。
pub async fn dispatch(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        None => chat::run_voice(cli.debug).await,
        Some(Commands::Chat { message }) => {
            if message.is_empty() {
                chat::run_voice(cli.debug).await
            } else {
                chat::run_text(&message.join(" "), cli.debug).await
            }
        }
        Some(Commands::Update { force }) => update::run(force).await,
        Some(Commands::Install) => daemon::install().await,
        Some(Commands::Uninstall) => daemon::uninstall().await,
        Some(Commands::Status) => daemon::status().await,
        Some(Commands::Open) => open::run().await,
        Some(Commands::Logs { follow, date }) => logs::run(follow, date).await,
        Some(Commands::Config { action }) => match action {
            None => config::run_wizard().await,
            Some(ConfigAction::Show) => config::show(),
            Some(ConfigAction::Set { key, value }) => config::set(&key, &value),
        },
    }
}
