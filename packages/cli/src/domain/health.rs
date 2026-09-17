//! 守护进程健康诊断规则（纯函数）。
//!
//! 一个"运行中"的守护进程可能因为拿不到麦克风（TCC 未授权、设备拔掉）而
//! 永远在重试；`launchctl list` / `systemctl is-active` 都会说它活着。
//! 这里只负责"给一段日志尾巴，判断是否处于该状态"，读日志由 `platform::service` 做。

const MIC_FAILURE_PATTERNS: &[&str] = &[
    "Failed to get default microphone config",
    "录音失败",
    "麦克风仍不可用",
    "麦克风断开",
    "mic disconnected",
    "mic calibration failed",
];

/// 统计 `lines` 中命中麦克风失败模式的行数，以及最后一行是否命中。
pub fn scan_lines_for_mic_failure(lines: &[&str]) -> (usize, bool) {
    fn is_hit(line: &str) -> bool {
        MIC_FAILURE_PATTERNS.iter().any(|p| line.contains(p))
    }
    let count = lines.iter().filter(|l| is_hit(l)).count();
    let last_is_failure = lines.last().map(|l| is_hit(l)).unwrap_or(false);
    (count, last_is_failure)
}

/// 根据日志尾巴生成面向用户的健康告警文案（空 = 健康）。
pub fn build_health_warnings(tail: &[&str]) -> Vec<String> {
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

#[cfg(test)]
mod tests {
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
