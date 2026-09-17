//! 单轮对话的耗时指标。

#[derive(Debug, Clone, Default)]
pub struct TurnMetrics {
    pub stt_ms: f32,
    pub llm_ttft_ms: f32,
    pub llm_total_ms: f32,
    pub llm_tokens: usize,
    pub tts_synth_ms: f32,
}

impl TurnMetrics {
    pub fn e2e_ms(&self) -> f32 {
        self.stt_ms + self.llm_total_ms
    }

    pub fn tok_per_s(&self) -> f32 {
        if self.llm_total_ms > 0.0 {
            self.llm_tokens as f32 / (self.llm_total_ms / 1000.0)
        } else {
            0.0
        }
    }

    /// 一行摘要，供 UI 打印（本层不直接输出）。
    pub fn summary(&self) -> String {
        format!(
            "STT {:.1}s | TTFT {:.1}s | {}tok {:.0}tok/s | TTS {:.1}s | E2E {:.1}s",
            self.stt_ms / 1000.0,
            self.llm_ttft_ms / 1000.0,
            self.llm_tokens,
            self.tok_per_s(),
            self.tts_synth_ms / 1000.0,
            self.e2e_ms() / 1000.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tok_per_s_is_zero_without_llm_time() {
        assert_eq!(TurnMetrics::default().tok_per_s(), 0.0);
    }

    #[test]
    fn summary_contains_all_stages() {
        let m = TurnMetrics { stt_ms: 500.0, llm_ttft_ms: 300.0, llm_total_ms: 2000.0, llm_tokens: 40, tts_synth_ms: 800.0 };
        let s = m.summary();
        assert!(s.contains("STT 0.5s") && s.contains("20tok/s") && s.contains("E2E 2.5s"));
    }
}
