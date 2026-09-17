//! 守护进程（launchd / systemd）的路径、状态与控制原语。
//!
//! 只提供"无交互"的原语：查是否在跑、重启、读最近日志。安装 / 卸载的
//! 用户流程（打印进度、权限引导、回滚）在 `cmd::daemon`。
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

use crate::domain::health::build_health_warnings;

pub const SERVICE_LABEL: &str = "com.erchoc.chatbot";
pub const SERVICE_DESC: &str = "chatbot voice assistant daemon";
pub const SYSTEMD_UNIT: &str = "chatbot.service";

/// 守护进程日志尾巴的默认读取行数。
const LOG_TAIL_LINES: usize = 80;

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"))
}

pub fn launchd_plist_path() -> PathBuf {
    home().join("Library/LaunchAgents").join(format!("{SERVICE_LABEL}.plist"))
}

pub fn launchd_log_dir() -> PathBuf {
    home().join("Library/Logs/chatbot")
}

pub fn launchd_stderr_log() -> PathBuf {
    launchd_log_dir().join("cb.stderr.log")
}

pub fn systemd_service_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| home().join(".config"))
        .join("systemd/user")
        .join(SYSTEMD_UNIT)
}

pub fn is_daemon_running() -> bool {
    if cfg!(target_os = "macos") {
        Command::new("launchctl")
            .args(["list", SERVICE_LABEL])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        Command::new("systemctl")
            .args(["--user", "is-active", SYSTEMD_UNIT])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
            .unwrap_or(false)
    }
}

/// 让受管的守护进程重启（launchd KeepAlive / systemd Restart=always 会自动拉起）。
pub fn restart_daemon() {
    if cfg!(target_os = "macos") {
        // 定位调用者的 GUI 会话：uid 必须动态取，写死 501 只对第一个用户有效。
        let uid = unsafe { libc::getuid() };
        let _ = Command::new("launchctl")
            .args(["kill", "SIGTERM", &format!("gui/{uid}/{SERVICE_LABEL}")])
            .output();
        // 兜底：launchctl kill 不生效时用 pkill
        let _ = Command::new("pkill").args(["-f", "cb chat"]).output();
    } else {
        let _ = Command::new("systemctl")
            .args(["--user", "restart", SYSTEMD_UNIT])
            .output();
    }
}

/// 读取守护进程最近的日志行（macOS 读 stderr 文件，Linux 问 journald）。
pub fn daemon_log_tail() -> Vec<String> {
    if cfg!(target_os = "macos") {
        let Ok(content) = std::fs::read_to_string(launchd_stderr_log()) else {
            return Vec::new();
        };
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(LOG_TAIL_LINES);
        lines[start..].iter().map(|s| s.to_string()).collect()
    } else {
        let n = LOG_TAIL_LINES.to_string();
        let output = Command::new("journalctl")
            .args(["--user", "-u", SYSTEMD_UNIT, "-n", &n, "--no-pager", "-o", "cat"])
            .output();
        match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// 结合最近日志给出健康告警（空 = 健康）。
pub fn daemon_health_warnings() -> Vec<String> {
    let tail = daemon_log_tail();
    let refs: Vec<&str> = tail.iter().map(|s| s.as_str()).collect();
    build_health_warnings(&refs)
}

pub fn run_cmd(cmd: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run: {cmd} {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{cmd} failed: {stderr}");
    }
    Ok(())
}
