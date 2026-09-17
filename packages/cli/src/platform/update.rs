//! GitHub Release 版本检查（24h 缓存）与安装渠道探测。
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::cache_path;
use crate::domain::semver::parse_major_minor;

pub const REPO: &str = "erchoc/chatbot";
const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
const CACHE_FILE: &str = "update_check.json";

/// How this binary was installed. Detected from the canonical exe path so we
/// can tell users the right upgrade command instead of a wrong-for-their-setup
/// `cb update` that would silently desync package-manager bookkeeping (brew
/// thinks it's still on v0.1.0 while the Cellar file is v0.1.1, etc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Curl,
    Brew,
    Npm,
    Direct,
}

pub fn detect_channel() -> Channel {
    let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
    else {
        return Channel::Direct;
    };
    let s = exe.to_string_lossy();

    if s.contains("/Cellar/cb/") || s.contains("/linuxbrew/") {
        Channel::Brew
    } else if s.contains("/node_modules/@erchoc/") || s.contains("/node_modules/chatbot/") {
        Channel::Npm
    } else if s.ends_with("/.local/bin/cb") || s.ends_with("/usr/local/bin/cb") {
        Channel::Curl
    } else {
        Channel::Direct
    }
}

/// The right upgrade command for the detected channel. Used in the "new
/// version available" banner so brew/npm users aren't told to run a command
/// that would desync their package manager.
pub fn upgrade_hint() -> String {
    match detect_channel() {
        Channel::Brew => "brew upgrade erchoc/tap/cb".into(),
        Channel::Npm => "npm install -g @erchoc/chatbot@latest".into(),
        Channel::Curl => "cb update".into(),
        Channel::Direct => format!(
            "从 https://github.com/{REPO}/releases/latest 下载新二进制"
        ),
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Cache {
    last_check_at: u64,
    latest_version: String,
}

fn cache_file() -> PathBuf {
    cache_path(CACHE_FILE)
}

fn load_cache() -> Option<Cache> {
    let raw = std::fs::read_to_string(cache_file()).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_cache(c: &Cache) {
    let path = cache_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(c) {
        let _ = std::fs::write(path, raw);
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Returns the cached latest version when its major or minor exceeds the
/// current build. Patch-only differences are considered silent.
pub fn pending_notice() -> Option<String> {
    let cache = load_cache()?;
    let (cur_major, cur_minor) = parse_major_minor(env!("CARGO_PKG_VERSION"))?;
    let (new_major, new_minor) = parse_major_minor(&cache.latest_version)?;
    if new_major > cur_major || (new_major == cur_major && new_minor > cur_minor) {
        Some(cache.latest_version)
    } else {
        None
    }
}

pub async fn fetch_latest_tag() -> anyhow::Result<String> {
    fetch_latest(false).await
}

/// Fetch the most recent release tag. When `include_prerelease` is true, hits
/// the list endpoint (which includes beta/rc tags); otherwise hits
/// `/releases/latest` which GitHub filters to stable only.
pub async fn fetch_latest(include_prerelease: bool) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let url = if include_prerelease {
        format!("https://api.github.com/repos/{REPO}/releases?per_page=1")
    } else {
        format!("https://api.github.com/repos/{REPO}/releases/latest")
    };
    let resp: serde_json::Value = client
        .get(&url)
        .header("User-Agent", "cb-updater")
        .send()
        .await?
        .json()
        .await?;

    // /releases/latest returns a single object with .tag_name; the list
    // endpoint returns an array — normalize here so callers don't care.
    let tag = if include_prerelease {
        resp.get(0).and_then(|r| r["tag_name"].as_str())
    } else {
        resp["tag_name"].as_str()
    };
    tag.map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("无法获取最新版本号"))
}

/// Fire-and-forget daily check. Silent on any failure (network, parse, io).
pub fn spawn_background_check() {
    let should_check = match load_cache() {
        Some(c) => now_secs().saturating_sub(c.last_check_at) >= CHECK_INTERVAL_SECS,
        None => true,
    };
    if !should_check {
        return;
    }
    tokio::spawn(async move {
        if let Ok(tag) = fetch_latest_tag().await {
            save_cache(&Cache {
                last_check_at: now_secs(),
                latest_version: tag.trim_start_matches('v').to_string(),
            });
        }
    });
}
