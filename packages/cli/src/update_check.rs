use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::cache_path;

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

fn parse_major_minor(v: &str) -> Option<(u32, u32)> {
    let v = v.trim_start_matches('v');
    let mut parts = v.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor_raw = parts.next()?;
    // Handle "2-dev" / "2+build" suffixes defensively.
    let minor: u32 = minor_raw
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((major, minor))
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

/// Compare two version strings with prerelease awareness. Treats any
/// `-suffix` (beta, rc, …) as lower than the same `x.y.z` without a suffix,
/// matching semver's precedence rule. Returns `Ordering::Less/Greater/Equal`.
///
/// Examples:
///   cmp("0.1.0",        "0.1.0")        == Equal
///   cmp("0.1.0",        "0.1.1")        == Less
///   cmp("0.1.1-beta.1", "0.1.1")        == Less      (beta predates stable)
///   cmp("0.1.1-beta.2", "0.1.1-beta.1") == Greater
///   cmp("0.1.1",        "0.1.1-beta.5") == Greater
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;

    let parse = |v: &str| -> (Vec<u32>, Option<String>) {
        let v = v.trim_start_matches('v');
        let (core, pre) = match v.split_once('-') {
            Some((core, pre)) => (core, Some(pre.to_string())),
            None => (v, None),
        };
        let nums: Vec<u32> = core
            .split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect();
        (nums, pre)
    };

    let (a_nums, a_pre) = parse(a);
    let (b_nums, b_pre) = parse(b);

    // Compare x.y.z numerically, padding shorter vectors with zeros.
    let len = a_nums.len().max(b_nums.len());
    for i in 0..len {
        let ai = a_nums.get(i).copied().unwrap_or(0);
        let bi = b_nums.get(i).copied().unwrap_or(0);
        match ai.cmp(&bi) {
            Equal => continue,
            other => return other,
        }
    }

    // Numeric part is equal — prerelease presence breaks the tie.
    match (a_pre, b_pre) {
        (None, None) => Equal,
        (None, Some(_)) => Greater, // 0.1.1 > 0.1.1-beta.1
        (Some(_), None) => Less,    // 0.1.1-beta.1 < 0.1.1
        (Some(a), Some(b)) => a.cmp(&b), // lexicographic is good enough here
    }
}

/// Best-effort desktop notification. Backgrounded daemons write stdout to a
/// log file, so a terminal banner alone never reaches the user — this routes
/// through the OS notification center instead. Silent if the tooling isn't
/// there (no osascript / notify-send / BurntToast); we don't want a failed
/// notifier to spam the daemon log.
pub fn notify_desktop(version: &str, hint: &str) {
    let title = format!("chatbot — 新版本 v{version}");
    let body = format!("运行 {hint} 升级");

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            r#"display notification "{}" with title "{}""#,
            body.replace('"', "\\\""),
            title.replace('"', "\\\"")
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .args([&title, &body])
            .output();
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (title, body);
    }
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

#[cfg(test)]
mod tests {
    use super::{compare_versions, parse_major_minor};
    use std::cmp::Ordering::*;

    #[test]
    fn parses_plain_version() {
        assert_eq!(parse_major_minor("0.2.0"), Some((0, 2)));
        assert_eq!(parse_major_minor("v1.5.10"), Some((1, 5)));
    }

    #[test]
    fn handles_suffixed_minor() {
        assert_eq!(parse_major_minor("0.2-dev.0"), Some((0, 2)));
        assert_eq!(parse_major_minor("1.0+build"), Some((1, 0)));
    }

    #[test]
    fn rejects_malformed() {
        assert_eq!(parse_major_minor("abc"), None);
        assert_eq!(parse_major_minor("1"), None);
    }

    #[test]
    fn compare_numeric_parts() {
        assert_eq!(compare_versions("0.1.0", "0.1.0"), Equal);
        assert_eq!(compare_versions("0.1.0", "0.1.1"), Less);
        assert_eq!(compare_versions("0.2.0", "0.1.9"), Greater);
        assert_eq!(compare_versions("1.0.0", "0.9.9"), Greater);
    }

    #[test]
    fn compare_prerelease_ordering() {
        // semver: prerelease < same x.y.z stable
        assert_eq!(compare_versions("0.1.1-beta.1", "0.1.1"), Less);
        assert_eq!(compare_versions("0.1.1", "0.1.1-beta.1"), Greater);
        assert_eq!(compare_versions("0.1.1-beta.1", "0.1.1-beta.2"), Less);
        assert_eq!(compare_versions("0.1.1-beta.1", "0.1.1-beta.1"), Equal);
    }

    #[test]
    fn compare_prerelease_vs_older_stable() {
        // A beta of a future release still beats the current stable.
        assert_eq!(compare_versions("0.1.1-beta.1", "0.1.0"), Greater);
        assert_eq!(compare_versions("0.1.0", "0.1.1-beta.1"), Less);
    }

    #[test]
    fn compare_ignores_v_prefix() {
        assert_eq!(compare_versions("v0.1.0", "0.1.0"), Equal);
        assert_eq!(compare_versions("v0.1.1", "v0.1.0"), Greater);
    }
}
