use anyhow::Result;
use async_trait::async_trait;

/// Speech-to-text trait. Returns (recognized_text, latency_ms).
#[async_trait]
pub trait Asr: Send + Sync {
    async fn recognize(&self, wav_data: &[u8]) -> Result<(String, f32)>;
}
