use anyhow::Result;
use async_trait::async_trait;

/// 识别时的语境：热词 + 最近几轮对话。豆包 2.0 用它做上下文纠错，
/// 助手名字 / 唤醒词这类专有词放热词里命中率明显提高。
#[derive(Debug, Clone, Default)]
pub struct AsrContext {
    pub hotwords: Vec<String>,
    pub dialog: Vec<DialogTurn>,
}

#[derive(Debug, Clone)]
pub struct DialogTurn {
    pub speaker: Speaker,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    User,
    Bot,
}

/// Speech-to-text. 输入 16kHz 单声道 WAV，返回 (识别文本, 耗时 ms)。
#[async_trait]
pub trait Asr: Send + Sync {
    async fn recognize(&self, wav_data: &[u8], ctx: &AsrContext) -> Result<(String, f32)>;
}
