//! LLM 流式 token → 句子切分。
//!
//! 只在真正的句末标点处切（。！？.!?换行），**不在逗号切**：逗号是句中停顿，
//! 切出来的碎片太短，TTS 引擎会拒绝合成（见 CLAUDE.md 错误记录 #13）。

pub fn is_sentence_end(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '.' | '!' | '?' | '\n')
}

/// TTS 前最少要有多少个"有意义"的字符（字母/数字/汉字）。更短的片段引擎会 500。
pub const MIN_TTS_CHARS: usize = 2;

/// 文本是否值得送去合成。
pub fn is_speakable(text: &str) -> bool {
    text.trim().chars().filter(|c| c.is_alphanumeric()).count() >= MIN_TTS_CHARS
}

/// 累积 token，遇到句末标点就吐出一整句。
#[derive(Debug, Default)]
pub struct SentenceBuffer {
    pending: String,
}

impl SentenceBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个 token；若累积文本已含句末标点，返回这一句（已 trim）。
    pub fn push(&mut self, token: &str) -> Option<String> {
        self.pending.push_str(token);
        if self.pending.chars().any(is_sentence_end) && !self.pending.trim().is_empty() {
            let text = self.pending.trim().to_string();
            self.pending.clear();
            Some(text)
        } else {
            None
        }
    }

    /// 流结束时把残留文本吐出来。
    pub fn flush(&mut self) -> Option<String> {
        let text = self.pending.trim().to_string();
        self.pending.clear();
        if text.is_empty() { None } else { Some(text) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_full_stop_not_comma() {
        let mut b = SentenceBuffer::new();
        assert_eq!(b.push("今天"), None);
        assert_eq!(b.push("天气，"), None);
        assert_eq!(b.push("不错。"), Some("今天天气，不错。".into()));
        assert_eq!(b.push("再见"), None);
        assert_eq!(b.flush(), Some("再见".into()));
        assert_eq!(b.flush(), None);
    }

    #[test]
    fn speakable_requires_two_alnum_chars() {
        assert!(!is_speakable("。"));
        assert!(!is_speakable("呢"));
        assert!(is_speakable("好的"));
        assert!(is_speakable("ok"));
    }
}
