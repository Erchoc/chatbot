//! 语音能力 · 端口（trait）+ 适配器（provider 实现）+ 工厂
//!
//! 上层只依赖 [`Asr`] / [`Tts`] 两个 trait 和 [`build_asr`] / [`build_tts`]，
//! 不直接 `use` 任何 provider 类型；换 provider 只需改工厂里的 `match`。
pub mod asr;
pub mod doubao;
pub mod tts;

use std::sync::Arc;

use anyhow::Result;

pub use asr::{Asr, AsrContext, DialogTurn, Speaker};
pub use tts::{Tts, TtsOptions};

use crate::config::AppConfig;

fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// 按 `speech.provider` 构造语音识别实现。
pub fn build_asr(cfg: &AppConfig, debug: bool) -> Result<Box<dyn Asr>> {
    match cfg.speech.provider.as_str() {
        "doubao" => Ok(Box::new(doubao::DoubaoAsr::new(cfg.speech.doubao.clone(), debug))),
        other => anyhow::bail!("不支持的语音供应商: {other}（目前仅支持 doubao）"),
    }
}

/// 按 `speech.provider` 构造语音合成实现（`Arc` 便于多句并发合成时共享）。
pub fn build_tts(cfg: &AppConfig) -> Result<Arc<dyn Tts>> {
    match cfg.speech.provider.as_str() {
        "doubao" => Ok(Arc::new(doubao::DoubaoTts::new(http_client()?, cfg.speech.doubao.clone()))),
        other => anyhow::bail!("不支持的语音供应商: {other}（目前仅支持 doubao）"),
    }
}
