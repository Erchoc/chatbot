use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

const SERVICE_LABEL: &str = "com.erchoc.chatbox";
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

    println!("  Daemon installed and started successfully.");
    println!("  Use `cb uninstall` to remove.");
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
        .join("Library/Logs/chatbox")
}

fn install_launchd(cb_bin: &PathBuf) -> Result<()> {
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
    let output = Command::new("launchctl")
        .args(["list", SERVICE_LABEL])
        .output()
        .context("Failed to run launchctl list")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("  Service status: running");
        println!("{stdout}");
    } else {
        println!("  Service status: not installed or not running");
    }

    let plist_path = launchd_plist_path();
    println!(
        "  Plist: {} ({})",
        plist_path.display(),
        if plist_path.exists() {
            "exists"
        } else {
            "not found"
        }
    );
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
        .join("chatbox.service")
}

fn install_systemd(cb_bin: &PathBuf) -> Result<()> {
    let service_path = systemd_service_path();

    // Stop existing service if running (ignore errors)
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "chatbox.service"])
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
    run_cmd("systemctl", &["--user", "enable", "chatbox.service"])?;
    run_cmd("systemctl", &["--user", "start", "chatbox.service"])?;

    println!("  Service enabled and started");
    println!("  Logs: journalctl --user -u chatbox.service -f");
    Ok(())
}

fn uninstall_systemd() -> Result<()> {
    let service_path = systemd_service_path();

    // Stop and disable
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "chatbox.service"])
        .output();
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "chatbox.service"])
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
    let output = Command::new("systemctl")
        .args(["--user", "status", "chatbox.service"])
        .output()
        .context("Failed to run systemctl status")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.is_empty() {
        println!("  Service status: not installed");
    } else {
        print!("{stdout}");
    }
    Ok(())
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
