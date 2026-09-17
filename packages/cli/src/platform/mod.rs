//! 基础设施层 · 操作系统集成
//!
//! - `service`：launchd / systemd 守护进程的路径、状态查询与重启原语
//! - `notify`：桌面通知（osascript / notify-send）
//! - `update`：GitHub Release 版本检查、安装渠道探测
pub mod notify;
pub mod service;
pub mod update;
