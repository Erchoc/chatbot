use std::process::Command;

use anyhow::{Context, Result};

const REPO: &str = "erchoc/chatbot";

pub async fn run() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("  当前版本: v{current}");

    // ── Fetch latest release tag ────────────────────────────────────────────
    println!("  检查新版本...");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let resp: serde_json::Value = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", "cb-updater")
        .send()
        .await?
        .json()
        .await?;

    let latest_tag = resp["tag_name"]
        .as_str()
        .context("无法获取最新版本号")?;
    let latest = latest_tag.trim_start_matches('v');

    if latest == current {
        println!("  \x1b[92m✓\x1b[0m  已是最新版本");
        return Ok(());
    }

    println!("  发现新版本: v{current} → {latest_tag}");

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

fn is_daemon_running() -> bool {
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

fn restart_daemon() {
    if cfg!(target_os = "macos") {
        // KeepAlive=true means launchctl will auto-restart after kill
        let _ = Command::new("launchctl")
            .args(["kill", "SIGTERM", "gui/501/com.erchoc.chatbot"])
            .output();
        // Fallback: pkill if launchctl kill doesn't work
        let _ = Command::new("pkill").args(["-f", "cb chat"]).output();
    } else {
        let _ = Command::new("systemctl")
            .args(["--user", "restart", "chatbot.service"])
            .output();
    }
}
