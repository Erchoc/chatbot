use std::process::Command;

use anyhow::{Context, Result};

use crate::update_check::{compare_versions, fetch_latest, Channel, REPO, detect_channel};

pub async fn run(force: bool) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("  当前版本: v{current}");

    // --force widens the resolver to include beta/rc tags AND skips the
    // "already on latest" short-circuit. Without --force we stay on the
    // stable-only channel so beta-pushing stays opt-in.
    println!(
        "  检查新版本{}...",
        if force { "（含 beta，--force）" } else { "" }
    );
    let latest_tag = fetch_latest(force).await?;
    let latest = latest_tag.trim_start_matches('v');

    let ord = compare_versions(current, latest);
    if !force {
        use std::cmp::Ordering::*;
        match ord {
            Equal => {
                println!("  \x1b[92m✓\x1b[0m  已是最新版本");
                return Ok(());
            }
            Greater => {
                // Local is ahead of the stable channel (e.g. running a beta).
                // Staying here would be a silent downgrade — refuse and tell
                // the user they can force it if they actually want to.
                println!(
                    "  \x1b[92m✓\x1b[0m  已是最新版本（当前 v{current} 比稳定版 v{latest} 更新）"
                );
                println!(
                    "     如需切回稳定版或刷到最新 beta，运行 \x1b[1mcb update --force\x1b[0m"
                );
                return Ok(());
            }
            Less => {} // fall through to upgrade
        }
    } else if ord == std::cmp::Ordering::Equal {
        println!(
            "  \x1b[90m(已是最新版本 v{current}，--force 强制重新安装)\x1b[0m"
        );
    }

    println!("  目标版本: v{current} → {latest_tag}");

    // `cb update` self-replaces the binary. That's fine for curl users, but
    // for brew/npm it would silently desync their package manager's view of
    // the installed version. Redirect to the right command instead.
    match detect_channel() {
        Channel::Brew => {
            println!(
                "  \x1b[33m⚠\x1b[0m  检测到通过 Homebrew 安装 —— \
                 请用 \x1b[1mbrew upgrade erchoc/tap/cb\x1b[0m 升级"
            );
            println!("     （`cb update` 会绕过 brew 的版本记录，导致 `brew info` 与实际不符）");
            return Ok(());
        }
        Channel::Npm => {
            println!(
                "  \x1b[33m⚠\x1b[0m  检测到通过 npm 安装 —— \
                 请用 \x1b[1mnpm install -g @erchoc/chatbot@latest\x1b[0m 升级"
            );
            return Ok(());
        }
        Channel::Curl | Channel::Direct => {}
    }

    // ── Determine artifact name ─────────────────────────────────────────────
    let artifact = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", _) => "cb-macos-universal",
        ("linux", "x86_64") => "cb-linux-x86_64",
        ("linux", "aarch64") => "cb-linux-arm64",
        (os, arch) => anyhow::bail!("不支持的平台: {os}/{arch}"),
    };

    let url = format!(
        "https://github.com/{REPO}/releases/download/{latest_tag}/{artifact}"
    );

    // ── Download ────────────────────────────────────────────────────────────
    println!("  下载 {artifact}...");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let bytes = client
        .get(&url)
        .send()
        .await?
        .error_for_status()
        .context(format!("下载失败: {url}"))?
        .bytes()
        .await?;

    // ── Replace current binary ──────────────────────────────────────────────
    // Resolve symlinks so we replace the real file, not a brew symlink
    let exe = std::env::current_exe()
        .context("无法确定当前二进制路径")?
        .canonicalize()
        .context("无法解析二进制真实路径")?;
    let tmp = exe.with_extension("tmp");

    std::fs::write(&tmp, &bytes).context("写入临时文件失败")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }

    // Atomic rename: old binary is replaced in one operation
    std::fs::rename(&tmp, &exe).context("替换二进制失败")?;

    println!("  \x1b[92m✓\x1b[0m  已更新到 {latest_tag}");

    // ── Restart daemon if running ───────────────────────────────────────────
    if is_daemon_running() {
        println!("  重启后台服务...");
        restart_daemon();
        println!("  \x1b[92m✓\x1b[0m  后台服务已重启");
    }

    Ok(())
}

pub fn is_daemon_running() -> bool {
    if cfg!(target_os = "macos") {
        Command::new("launchctl")
            .args(["list", "com.erchoc.chatbot"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        Command::new("systemctl")
            .args(["--user", "is-active", "chatbot.service"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    == "active"
            })
            .unwrap_or(false)
    }
}

pub fn restart_daemon() {
    if cfg!(target_os = "macos") {
        // Target launchd in the caller's GUI session — uid 501 worked for me
        // but breaks for any other user, so resolve it dynamically.
        let uid = unsafe { libc::getuid() };
        let _ = Command::new("launchctl")
            .args(["kill", "SIGTERM", &format!("gui/{uid}/com.erchoc.chatbot")])
            .output();
        // Fallback: pkill if launchctl kill doesn't work
        let _ = Command::new("pkill").args(["-f", "cb chat"]).output();
    } else {
        let _ = Command::new("systemctl")
            .args(["--user", "restart", "chatbot.service"])
            .output();
    }
}
