//! 应用层 · 语音对话编排
//!
//! - `voice`：常驻主循环（录音 → ASR → 唤醒判定 → 一轮对话 → 记账）
//! - `speak`：一轮对话（LLM 流式 → 分句 → TTS 并发合成 → 顺序播放）
pub mod speak;
pub mod voice;
