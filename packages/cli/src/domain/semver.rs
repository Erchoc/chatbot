//! 版本号解析与比较（纯函数）。
use std::cmp::Ordering;

/// 解析 `x.y[...]` 的主次版本号；容忍 `v` 前缀与 `-dev` / `+build` 后缀。
pub fn parse_major_minor(v: &str) -> Option<(u32, u32)> {
    let v = v.trim_start_matches('v');
    let mut parts = v.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor_raw = parts.next()?;
    let minor: u32 = minor_raw
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((major, minor))
}

/// 语义化版本比较，含预发布规则：任何 `-suffix`（beta、rc…）都小于同号的正式版。
///
/// ```text
/// cmp("0.1.0",        "0.1.0")        == Equal
/// cmp("0.1.0",        "0.1.1")        == Less
/// cmp("0.1.1-beta.1", "0.1.1")        == Less      (beta 早于正式版)
/// cmp("0.1.1-beta.2", "0.1.1-beta.1") == Greater
/// ```
pub fn compare_versions(a: &str, b: &str) -> Ordering {
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

    let len = a_nums.len().max(b_nums.len());
    for i in 0..len {
        let ai = a_nums.get(i).copied().unwrap_or(0);
        let bi = b_nums.get(i).copied().unwrap_or(0);
        match ai.cmp(&bi) {
            Ordering::Equal => continue,
            other => return other,
        }
    }

    match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => a.cmp(&b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(compare_versions("0.1.1-beta.1", "0.1.1"), Less);
        assert_eq!(compare_versions("0.1.1", "0.1.1-beta.1"), Greater);
        assert_eq!(compare_versions("0.1.1-beta.1", "0.1.1-beta.2"), Less);
        assert_eq!(compare_versions("0.1.1-beta.1", "0.1.1-beta.1"), Equal);
    }

    #[test]
    fn compare_prerelease_vs_older_stable() {
        assert_eq!(compare_versions("0.1.1-beta.1", "0.1.0"), Greater);
        assert_eq!(compare_versions("0.1.0", "0.1.1-beta.1"), Less);
    }

    #[test]
    fn compare_ignores_v_prefix() {
        assert_eq!(compare_versions("v0.1.0", "0.1.0"), Equal);
        assert_eq!(compare_versions("v0.1.1", "v0.1.0"), Greater);
    }
}
