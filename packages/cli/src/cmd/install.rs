use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

const SERVICE_LABEL: &str = "com.erchoc.chatbot";
const SERVICE_DESC: &str = "Chatbox voice assistant daemon";

/// Install and start the daemon service
pub async fn run() -> Result<()> {
    let cb_bin = get_cb_binary_path()?;
    println!("  Installing daemon service...");
    println!("  Binary: {}", cb_bin.display());

    if cfg!(target_os = "macos") {
        install_launchd(&cb_bin)?;
    } else if cfg!(target_os = "linux") {
        install_systemd(&cb_bin)?;
    } else {
        anyhow::bail!("Unsupported platform. Only macOS and Linux are supported.");
    }

    // Show post-install summary with usage info
    let cfg = crate::config::AppConfig::load().unwrap_or_default();
    let wake_enabled = cfg.persona.wake_word.enabled;
    let wake_word = &cfg.persona.wake_word.word;

    println!();
    println!("  \x1b[92m✓\x1b[0m  后台服务已启动");
    println!();
    if wake_enabled {
        println!("  \x1b[1m使用方式:\x1b[0m  说「{wake_word}」+ 你的问题");
        println!("  \x1b[90m示例: \"{wake_word}，今天天气怎么样\"\x1b[0m");
    } else {
        println!("  \x1b[1m使用方式:\x1b[0m  直接说话即可对话");
    }
    println!();
    println!("  \x1b[90m查看状态:  cb status\x1b[0m");
    println!("  \x1b[90m查看日志:  cb logs -f\x1b[0m");
    println!("  \x1b[90m停止服务:  cb uninstall\x1b[0m");
    println!();
    Ok(())
}

/// Uninstall the daemon service and remove all traces
pub async fn uninstall() -> Result<()> {
    println!("  Uninstalling daemon service...");

    if cfg!(target_os = "macos") {
        uninstall_launchd()?;
    } else if cfg!(target_os = "linux") {
        uninstall_systemd()?;
    } else {
        anyhow::bail!("Unsupported platform.");
    }

    println!("  Daemon uninstalled. All service files removed.");
    Ok(())
}

/// Show daemon status
pub async fn status() -> Result<()> {
    if cfg!(target_os = "macos") {
        status_launchd()?;
    } else if cfg!(target_os = "linux") {
        status_systemd()?;
    } else {
        anyhow::bail!("Unsupported platform.");
    }
    Ok(())
}

// === Resolve binary path ===

fn get_cb_binary_path() -> Result<PathBuf> {
    // Use the currently running binary path
    let exe = std::env::current_exe().context("Failed to determine binary path")?;
    // If running from cargo target dir, warn user
    let exe_str = exe.to_string_lossy();
    if exe_str.contains("/target/debug/") || exe_str.contains("/target/release/") {
        println!("  Warning: Using development binary. For production, run `cargo install --path .` first.");
    }
    Ok(exe)
}

// === macOS: codesign ===

/// Ensure the binary has an ad-hoc signature with microphone entitlement.
/// If already properly signed (e.g. by CI with Developer ID), this is a no-op.
fn ensure_codesign(cb_bin: &PathBuf) {
    // Check if already signed with a valid identity (not ad-hoc)
    let verify = Command::new("codesign")
        .args(["--verify", "--verbose"])
        .arg(cb_bin)
        .output();

    if let Ok(out) = &verify {
        if out.status.success() {
            // Already properly signed — check if it has the audio entitlement
            let ent_check = Command::new("codesign")
                .args(["--display", "--entitlements", ":-"])
                .arg(cb_bin)
                .output();
            if let Ok(ent_out) = ent_check {
                let ent_str = String::from_utf8_lossy(&ent_out.stdout);
                if ent_str.contains("com.apple.security.device.audio-input") {
                    return; // Fully signed with correct entitlements
                }
            }
        }
    }

    // Write temporary entitlements file
    let entitlements = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.device.audio-input</key>
    <true/>
</dict>
</plist>"#;

    let tmp_dir = std::env::temp_dir();
    let ent_path = tmp_dir.join("cb-entitlements.plist");
    if std::fs::write(&ent_path, entitlements).is_err() {
        println!("  Warning: could not write entitlements file, skipping codesign");
        return;
    }

    println!("  Signing binary with microphone entitlement...");
    // NOTE: do NOT use --options runtime for ad-hoc signing.
    // Hardened runtime is only needed for notarization with Developer ID.
    // With ad-hoc sign it causes unwanted directory access prompts.
    let result = Command::new("codesign")
        .args(["--force", "--entitlements"])
        .arg(&ent_path)
        .args(["--sign", "-"])
        .arg(cb_bin)
        .output();

    let _ = std::fs::remove_file(&ent_path);

    match result {
        Ok(out) if out.status.success() => {
            println!("  Signed ✓");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            println!("  Warning: codesign failed: {stderr}");
        }
        Err(e) => {
            println!("  Warning: codesign not available: {e}");
        }
    }
}

/// Open the microphone briefly to trigger the macOS TCC permission dialog.
/// This must happen in a foreground terminal context — launchd daemons
/// cannot present the dialog, causing infinite re-prompts.
fn request_microphone_permission() -> Result<()> {
    use crate::audio::capture::get_input_device_info;

    println!("  Checking microphone access...");

    match get_input_device_info() {
        Ok((device, config, _)) => {
            // Actually open the stream — this is what triggers the TCC prompt
            use cpal::traits::{DeviceTrait, StreamTrait};
            let stream = device.build_input_stream(
                &config,
                |_data: &[f32], _: &cpal::InputCallbackInfo| {},
                |err| eprintln!("  mic check error: {err}"),
                None,
            );
            match stream {
                Ok(s) => {
                    // Keep stream alive briefly so macOS registers the access
                    s.play().ok();
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    drop(s);
                    println!("  Microphone access ✓");
                }
                Err(e) => {
                    anyhow::bail!(
                        "无法访问麦克风: {e}\n\
                         请在 系统设置 → 隐私与安全 → 麦克风 中允许终端应用访问麦克风，\n\
                         然后重新运行 `cb install`"
                    );
                }
            }
        }
        Err(e) => {
            anyhow::bail!(
                "未检测到麦克风: {e}\n\
                 请连接麦克风后重新运行 `cb install`"
            );
        }
    }

    Ok(())
}

// === macOS: launchd ===

fn launchd_plist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join("Library/LaunchAgents")
        .join(format!("{SERVICE_LABEL}.plist"))
}

fn launchd_log_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join("Library/Logs/chatbot")
}

fn install_launchd(cb_bin: &PathBuf) -> Result<()> {
    // Ad-hoc sign with microphone entitlement if not already properly signed.
    ensure_codesign(cb_bin);

    // Trigger microphone authorization NOW while we have a terminal/GUI context.
    // launchd daemons cannot show the macOS permission dialog, so the user
    // must grant access here. Without this, the daemon would trigger repeated
    // permission popups on every restart.
    request_microphone_permission()?;

    let plist_path = launchd_plist_path();
    let log_dir = launchd_log_dir();

    // Create log directory
    std::fs::create_dir_all(&log_dir)?;

    let stdout_log = log_dir.join("cb.stdout.log");
    let stderr_log = log_dir.join("cb.stderr.log");

    // Unload existing service if present (ignore errors)
    if plist_path.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()])
            .output();
    }

    // Write plist
    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{SERVICE_LABEL}</string>
    <key>Comment</key>
    <string>{SERVICE_DESC}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>chat</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{stdout}</string>
    <key>StandardErrorPath</key>
    <string>{stderr}</string>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>"#,
        bin = cb_bin.display(),
        stdout = stdout_log.display(),
        stderr = stderr_log.display(),
    );

    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(&plist_path)?;
    f.write_all(plist_content.as_bytes())?;
    println!("  Created {}", plist_path.display());

    // Load service
    let output = Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .output()
        .context("Failed to run launchctl load")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("launchctl load failed: {stderr}");
    }

    println!("  Service loaded and started");
    println!("  Logs: {}", log_dir.display());

    // Post-install smoke test: the foreground mic check can pass while
    // the launchd-scoped execution of the same binary still fails (happened
    // with a pre-entitlement ad-hoc signed binary downloaded to Desktop).
    // Watch stderr for ~3s; if record failures appear, unload + clean up.
    verify_daemon_healthy(&plist_path, &stderr_log)?;

    Ok(())
}

/// Wait briefly for the daemon's first loop iteration and confirm it isn't
/// already stuck in a mic-retry cycle. If it is, roll back the install
/// (unload + delete plist) and report the error with fix-it hints — far
/// better than leaving the user with a daemon that claims "running" but
/// never responds.
fn verify_daemon_healthy(plist_path: &PathBuf, stderr_log: &PathBuf) -> Result<()> {
    use std::time::{Duration, Instant};

    // Remember the stderr file's baseline size so we only examine NEW lines
    // written since this install started. This avoids false positives from
    // a previous install's stale errors still at the top of the file.
    let baseline_len = std::fs::metadata(stderr_log).map(|m| m.len()).unwrap_or(0);

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut last_checked_tail: Vec<String>;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        if let Ok(content) = std::fs::read(stderr_log) {
            let fresh = &content[(baseline_len as usize).min(content.len())..];
            let text = String::from_utf8_lossy(fresh);
            last_checked_tail = text.lines().map(|s| s.to_string()).collect();
            let tail_refs: Vec<&str> = last_checked_tail.iter().map(|s| s.as_str()).collect();
            let (mic_count, _) = scan_lines_for_mic_failure(&tail_refs);
            if mic_count >= 2 {
                // Roll back before bailing so the user isn't left with a
                // half-broken install + a false-positive `cb status`.
                let _ = Command::new("launchctl")
                    .args(["unload", &plist_path.to_string_lossy()])
                    .output();
                let _ = std::fs::remove_file(plist_path);
                anyhow::bail!(
                    "Daemon started but mic is unavailable under launchd.\n\
                     前台权限通过不等于 launchd 上下文能拿到麦克风。\n\
                     已回滚安装。\n\
                     \n\
                     修复步骤:\n\
                       1. 系统设置 → 隐私与安全性 → 麦克风: 删除任何已有的 cb / Desktop 条目\n\
                       2. 删掉其他位置可能遗留的老 cb 二进制 (特别是 Desktop、Downloads)\n\
                       3. 重新 `cb install`，允许新弹出的权限对话框\n\
                     \n\
                     守护进程 stderr 最后几行:\n{}",
                    last_checked_tail.iter().rev().take(5).rev().cloned()
                        .collect::<Vec<_>>().join("\n")
                );
            }
        }
    }
    println!("  \x1b[92m✓\x1b[0m Smoke test passed (3s, no mic errors).");
    Ok(())
}

fn uninstall_launchd() -> Result<()> {
    let plist_path = launchd_plist_path();
    let log_dir = launchd_log_dir();

    // Unload service (stops the process)
    if plist_path.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()])
            .output();
        println!("  Service stopped");

        // Remove plist file
        std::fs::remove_file(&plist_path)?;
        println!("  Removed {}", plist_path.display());
    }

    // Fallback: force remove from launchctl registry in case plist was deleted manually
    let _ = Command::new("launchctl")
        .args(["remove", SERVICE_LABEL])
        .output();

    // Kill any lingering cb processes (except ourselves)
    let our_pid = std::process::id();
    if let Ok(output) = Command::new("pgrep").args(["-f", "cb chat"]).output() {
        let pids = String::from_utf8_lossy(&output.stdout);
        for pid_str in pids.lines() {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if pid != our_pid {
                    let _ = Command::new("kill").arg(pid_str.trim()).output();
                }
            }
        }
    }

    // Remove log files
    if log_dir.exists() {
        std::fs::remove_dir_all(&log_dir)?;
        println!("  Removed logs: {}", log_dir.display());
    }

    if !plist_path.exists() && !log_dir.exists() {
        println!("  Nothing to uninstall (already clean)");
    }

    println!("  All service traces removed");
    Ok(())
}

fn status_launchd() -> Result<()> {
    let plist_path = launchd_plist_path();
    let output = Command::new("launchctl")
        .args(["list", SERVICE_LABEL])
        .output()
        .context("Failed to run launchctl list")?;

    if output.status.success() {
        println!("  \x1b[92m●\x1b[0m  后台服务运行中");
        let log_dir = launchd_log_dir();
        println!("     日志目录: {}", log_dir.display());
        println!("     配置文件: {}", plist_path.display());

        // Surface obvious failure modes that leave the daemon "running"
        // but not actually working (most commonly: no mic permission).
        let stderr_path = log_dir.join("cb.stderr.log");
        let warnings = detect_daemon_health_issues(&stderr_path);
        if !warnings.is_empty() {
            println!();
            println!("  \x1b[91m⚠  健康检查\x1b[0m");
            for w in &warnings {
                println!("     \x1b[91m{w}\x1b[0m");
            }
            print_mic_recovery_hint();
        }

        println!();
        println!("     \x1b[90m停止服务: cb uninstall\x1b[0m");
        println!("     \x1b[90m查看日志: cb logs -f\x1b[0m");
    } else if plist_path.exists() {
        println!("  \x1b[93m●\x1b[0m  后台服务已安装但未运行");
        println!("     配置文件: {}", plist_path.display());
        println!();
        println!("     \x1b[90m重新启动: launchctl load {}\x1b[0m", plist_path.display());
        println!("     \x1b[90m完全卸载: cb uninstall\x1b[0m");
    } else {
        println!("  \x1b[90m●\x1b[0m  后台服务未安装");
        println!();
        println!("     \x1b[90m安装并启动: cb install\x1b[0m");
        println!("     \x1b[90m前台运行:   cb\x1b[0m");
    }
    Ok(())
}

// === Linux: systemd user service ===

fn systemd_service_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".config")
        })
        .join("systemd/user")
        .join("chatbot.service")
}

fn install_systemd(cb_bin: &PathBuf) -> Result<()> {
    let service_path = systemd_service_path();

    // Stop existing service if running (ignore errors)
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "chatbot.service"])
        .output();

    let service_content = format!(
        r#"[Unit]
Description={SERVICE_DESC}
After=default.target sound.target

[Service]
Type=simple
ExecStart={bin} chat
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
"#,
        bin = cb_bin.display(),
    );

    if let Some(parent) = service_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(&service_path)?;
    f.write_all(service_content.as_bytes())?;
    println!("  Created {}", service_path.display());

    // Reload, enable, and start
    run_cmd("systemctl", &["--user", "daemon-reload"])?;
    run_cmd("systemctl", &["--user", "enable", "chatbot.service"])?;
    run_cmd("systemctl", &["--user", "start", "chatbot.service"])?;

    println!("  Service enabled and started");
    println!("  Logs: journalctl --user -u chatbot.service -f");
    Ok(())
}

fn uninstall_systemd() -> Result<()> {
    let service_path = systemd_service_path();

    // Stop and disable
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "chatbot.service"])
        .output();
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "chatbot.service"])
        .output();
    println!("  Service stopped and disabled");

    // Remove service file
    if service_path.exists() {
        std::fs::remove_file(&service_path)?;
        println!("  Removed {}", service_path.display());
    }

    // Reload daemon to fully clean up systemd state
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();
    let _ = Command::new("systemctl")
        .args(["--user", "reset-failed"])
        .output();

    // Kill any lingering cb processes (except ourselves)
    let our_pid = std::process::id();
    if let Ok(output) = Command::new("pgrep").args(["-f", "cb chat"]).output() {
        let pids = String::from_utf8_lossy(&output.stdout);
        for pid_str in pids.lines() {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if pid != our_pid {
                    let _ = Command::new("kill").arg(pid_str.trim()).output();
                }
            }
        }
    }

    if !service_path.exists() {
        println!("  Nothing to uninstall (already clean)");
    }

    println!("  All service traces removed");
    Ok(())
}

fn status_systemd() -> Result<()> {
    let service_path = systemd_service_path();
    let output = Command::new("systemctl")
        .args(["--user", "is-active", "chatbot.service"])
        .output()
        .context("Failed to run systemctl")?;

    let state = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if state == "active" {
        println!("  \x1b[92m●\x1b[0m  后台服务运行中");
        println!("     配置文件: {}", service_path.display());

        // Ask journald for the last 80 lines; sniff for mic failure loop.
        let warnings = detect_systemd_health_issues();
        if !warnings.is_empty() {
            println!();
            println!("  \x1b[91m⚠  健康检查\x1b[0m");
            for w in &warnings {
                println!("     \x1b[91m{w}\x1b[0m");
            }
            print_mic_recovery_hint();
        }

        println!();
        println!("     \x1b[90m查看日志: journalctl --user -u chatbot -f\x1b[0m");
        println!("     \x1b[90m停止服务: cb uninstall\x1b[0m");
    } else if service_path.exists() {
        println!("  \x1b[93m●\x1b[0m  后台服务已安装但未运行 ({})", state);
        println!("     配置文件: {}", service_path.display());
        println!();
        println!("     \x1b[90m重新启动: systemctl --user start chatbot\x1b[0m");
        println!("     \x1b[90m完全卸载: cb uninstall\x1b[0m");
    } else {
        println!("  \x1b[90m●\x1b[0m  后台服务未安装");
        println!();
        println!("     \x1b[90m安装并启动: cb install\x1b[0m");
        println!("     \x1b[90m前台运行:   cb\x1b[0m");
    }
    Ok(())
}

// ── Daemon health diagnostics ────────────────────────────────────────────
//
// A "running" daemon can still be useless if it can't acquire the mic (TCC
// not granted for the binary, device unplugged, etc). The daemon retries
// forever and writes to stderr, but `launchctl list` / `systemctl is-active`
// both keep saying "running". These helpers read recent daemon output and
// flag the common silent-failure modes.

const MIC_FAILURE_PATTERNS: &[&str] = &[
    "Failed to get default microphone config",
    "录音失败",
    "麦克风仍不可用",
    "麦克风断开",
    "mic disconnected",
    "mic calibration failed",
];

/// Count how many of the last `max_lines` in `path` match a mic-failure
/// pattern, and whether the most recent line is one.
fn scan_lines_for_mic_failure(lines: &[&str]) -> (usize, bool) {
    let count = lines
        .iter()
        .filter(|l| MIC_FAILURE_PATTERNS.iter().any(|p| l.contains(p)))
        .count();
    let last_is_failure = lines
        .last()
        .map(|l| MIC_FAILURE_PATTERNS.iter().any(|p| l.contains(p)))
        .unwrap_or(false);
    (count, last_is_failure)
}

fn detect_daemon_health_issues(stderr_path: &PathBuf) -> Vec<String> {
    let content = match std::fs::read_to_string(stderr_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let lines: Vec<&str> = content.lines().collect();
    let tail_len = lines.len().min(80);
    let tail: Vec<&str> = lines[lines.len() - tail_len..].to_vec();
    build_health_warnings(&tail)
}

fn detect_systemd_health_issues() -> Vec<String> {
    let output = Command::new("journalctl")
        .args([
            "--user",
            "-u",
            "chatbot.service",
            "-n",
            "80",
            "--no-pager",
            "-o",
            "cat",
        ])
        .output();
    let content = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };
    let tail: Vec<&str> = content.lines().collect();
    build_health_warnings(&tail)
}

fn build_health_warnings(tail: &[&str]) -> Vec<String> {
    let mut warnings = Vec::new();
    let (mic_count, mic_last) = scan_lines_for_mic_failure(tail);
    if mic_count >= 3 && mic_last {
        warnings.push(format!(
            "麦克风持续不可用（最近 {} 行中有 {} 条录音失败日志）",
            tail.len(),
            mic_count
        ));
    } else if mic_count >= 3 {
        warnings.push(format!(
            "最近有 {} 条录音失败日志（可能已恢复，但建议检查）",
            mic_count
        ));
    }
    warnings
}

fn print_mic_recovery_hint() {
    println!();
    println!("     \x1b[90m修复步骤:\x1b[0m");
    println!("     \x1b[90m  1. 系统设置 → 隐私与安全性 → 麦克风 → 确认 cb 已勾选\x1b[0m");
    println!("     \x1b[90m  2. 若无 cb 条目: 前台跑一次 `cb chat 你好` 触发授权弹窗\x1b[0m");
    println!("     \x1b[90m  3. 重装守护进程:  cb uninstall && cb install\x1b[0m");
}

#[cfg(test)]
mod health_tests {
    use super::*;

    #[test]
    fn mic_failure_detected_when_tail_shows_retry_loop() {
        let lines = vec![
            "   录音失败: Failed to get default microphone config",
            "   麦克风仍不可用，每 60 秒自动检测，连接后自动恢复...",
            "   录音失败: Failed to get default microphone config",
            "   麦克风仍不可用，每 60 秒自动检测，连接后自动恢复...",
            "   录音失败: Failed to get default microphone config",
        ];
        let (count, last) = scan_lines_for_mic_failure(&lines);
        assert_eq!(count, 5);
        assert!(last);
        assert!(!build_health_warnings(&lines).is_empty());
    }

    #[test]
    fn clean_log_produces_no_warning() {
        let lines = vec!["   ● session start s123", "   ✓ LLM OK", "   ✓ 语音 API OK"];
        let (count, _) = scan_lines_for_mic_failure(&lines);
        assert_eq!(count, 0);
        assert!(build_health_warnings(&lines).is_empty());
    }

    #[test]
    fn past_failure_flagged_softly_when_recovered() {
        // Three old failures, but the latest line is success — warn
        // softly ("may have recovered") rather than the red-alert variant.
        let mut lines = vec![
            "   录音失败: Failed to get default microphone config",
            "   录音失败: Failed to get default microphone config",
            "   录音失败: Failed to get default microphone config",
        ];
        lines.push("   ● session start s999");
        let warnings = build_health_warnings(&lines);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("可能已恢复"));
    }
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<()> {
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
