//! 唤醒词会话状态机 + 匹配规则。
use std::time::{Duration, Instant};

/// 唤醒后保持"清醒"的时长，期间无需再说唤醒词。
pub const WAKE_DURATION: Duration = Duration::from_secs(5 * 60);

/// 唤醒词会话状态。
#[derive(Debug)]
pub enum WakeState {
    /// 等待唤醒词。
    Sleeping,
    /// 已唤醒，`expires_at` 之后无新交互则自动休眠。
    Awake { expires_at: Instant },
}

impl WakeState {
    pub fn is_awake(&self) -> bool {
        match self {
            Self::Awake { expires_at } => Instant::now() < *expires_at,
            Self::Sleeping => false,
        }
    }

    pub fn wake(&mut self) {
        *self = Self::Awake { expires_at: Instant::now() + WAKE_DURATION };
    }

    pub fn renew(&mut self) {
        if let Self::Awake { expires_at } = self {
            *expires_at = Instant::now() + WAKE_DURATION;
        }
    }

    pub fn sleep(&mut self) {
        *self = Self::Sleeping;
    }
}

/// 一次识别结果经过唤醒词规则后的判定。
#[derive(Debug, PartialEq, Eq)]
pub enum WakeDecision {
    /// 唤醒词未开启，或已处于清醒态：原文直接进入对话。
    Proceed(String),
    /// 本句以唤醒词开头，去掉唤醒词后的剩余内容（可能为空，表示只喊了名字）。
    JustWoke(String),
    /// 清醒态下说了"退下"类短语。
    Deactivate,
    /// 休眠态下没听到唤醒词。
    Ignore,
}

/// 把识别文本套上唤醒词规则，并同步推进状态机。
pub fn decide(state: &mut WakeState, enabled: bool, wake_word: &str, text: &str) -> WakeDecision {
    if !enabled {
        return WakeDecision::Proceed(text.to_string());
    }
    if state.is_awake() {
        if is_deactivation(text) {
            state.sleep();
            return WakeDecision::Deactivate;
        }
        state.renew();
        return WakeDecision::Proceed(text.to_string());
    }
    match strip_wake_word(text, wake_word) {
        Some(rest) => {
            state.wake();
            WakeDecision::JustWoke(rest)
        }
        None => WakeDecision::Ignore,
    }
}

/// 判断文本是否以唤醒词开头，返回去掉唤醒词后的剩余部分。
///
/// 两轮匹配：
/// 1. 去标点/空格后的精确字符匹配（快路径）。
/// 2. 拼音匹配 —— 同音字视为相同（"黑小派" == "嘿小派"）。
pub fn strip_wake_word(text: &str, wake_word: &str) -> Option<String> {
    let strip_punct = |s: &str| -> String {
        s.to_lowercase()
            .replace([',', '，', '、', '.', '!', '！', '?', '？', ' ', '\u{3000}'], "")
    };

    let norm_text = strip_punct(text);
    let norm_wake = strip_punct(wake_word);
    if norm_wake.is_empty() {
        return None;
    }

    let char_match = norm_text.starts_with(&norm_wake);
    let pinyin_match = !char_match && {
        to_pinyin_str(wake_word)
            .zip(to_pinyin_str(text))
            .map(|(pw, pt)| pt.starts_with(&pw))
            .unwrap_or(false)
    };
    if !char_match && !pinyin_match {
        return None;
    }

    // 逐字消费原文，直到覆盖了唤醒词的全部规范化字符。
    let wake_chars: Vec<char> = norm_wake.chars().collect();
    let mut consumed_bytes = 0usize;
    let mut matched_chars = 0usize;
    for c in text.chars() {
        if matched_chars >= wake_chars.len() {
            break;
        }
        consumed_bytes += c.len_utf8();
        matched_chars += strip_punct(&c.to_string()).chars().count();
    }

    let rest = &text[consumed_bytes..];
    let rest = rest.trim_start_matches(|c: char| {
        c == ',' || c == '，' || c == '、' || c == ' ' || c == '\u{3000}'
    });
    Some(rest.trim().to_string())
}

/// 汉字 → 无声调、无分隔的拼音串；不含可识别汉字时返回 None。
fn to_pinyin_str(s: &str) -> Option<String> {
    use pinyin::{to_pinyin_vec, Pinyin};
    let result: String = to_pinyin_vec(s, |p: Pinyin| p.plain()).into_iter().collect();
    if result.is_empty() { None } else { Some(result) }
}

/// 用户是否在让助手"退下"。
pub fn is_deactivation(text: &str) -> bool {
    let t = text
        .to_lowercase()
        .replace([' ', '，', ',', '。', '.', '！', '!', '？', '?'], "");
    const PHRASES: &[&str] = &[
        "退下", "可以退下", "暂时退下", "先退下", "好了退下",
        "暂停", "先暂停", "休息", "先休息", "暂时休息",
        "不用了", "不需要了", "结束对话", "停止对话",
        "拜拜", "再见", "先这样", "就这样",
        "goodbye", "stoplistening", "goaway", "thatsenough",
        "dismiss", "sleep",
    ];
    PHRASES.iter().any(|p| t.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_strips_wake_word_and_punctuation() {
        assert_eq!(strip_wake_word("嘿小派，今天天气怎么样", "嘿小派"), Some("今天天气怎么样".into()));
        assert_eq!(strip_wake_word("嘿小派", "嘿小派"), Some("".into()));
    }

    #[test]
    fn pinyin_match_handles_homophones() {
        assert_eq!(strip_wake_word("黑小派 几点了", "嘿小派"), Some("几点了".into()));
    }

    #[test]
    fn no_match_returns_none() {
        assert_eq!(strip_wake_word("今天天气怎么样", "嘿小派"), None);
        assert_eq!(strip_wake_word("随便说说", ""), None);
    }

    #[test]
    fn deactivation_phrases() {
        assert!(is_deactivation("好了，退下吧"));
        assert!(is_deactivation("Stop listening!"));
        assert!(!is_deactivation("讲个笑话"));
    }

    #[test]
    fn state_machine_flow() {
        let mut st = WakeState::Sleeping;
        assert_eq!(decide(&mut st, true, "嘿小派", "几点了"), WakeDecision::Ignore);
        assert_eq!(decide(&mut st, true, "嘿小派", "嘿小派几点了"), WakeDecision::JustWoke("几点了".into()));
        assert!(st.is_awake());
        assert_eq!(decide(&mut st, true, "嘿小派", "再讲一个"), WakeDecision::Proceed("再讲一个".into()));
        assert_eq!(decide(&mut st, true, "嘿小派", "退下"), WakeDecision::Deactivate);
        assert!(!st.is_awake());
        assert_eq!(decide(&mut st, false, "嘿小派", "几点了"), WakeDecision::Proceed("几点了".into()));
    }
}
