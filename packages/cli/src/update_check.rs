use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::cache_path;

pub const REPO: &str = "erchoc/chatbot";
const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
const CACHE_FILE: &str = "update_check.json";

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
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let resp: serde_json::Value = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", "cb-updater")
        .send()
        .await?
        .json()
        .await?;
    resp["tag_name"]
        .as_str()
        .map(|s| s.to_string())
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

#[cfg(test)]
mod tests {
    use super::parse_major_minor;

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
}
