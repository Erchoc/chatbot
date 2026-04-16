use anyhow::Result;
use async_trait::async_trait;

/// Text-to-speech trait. Returns MP3 bytes.
#[async_trait]
pub trait Tts: Send + Sync {
    async fn synthesize(&self, text: &str) -> Result<Option<Vec<u8>>>;
}
