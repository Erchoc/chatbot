use anyhow::Result;
use async_trait::async_trait;

/// 合成选项。都是可选的"让声音更像人"的语境信息。
#[derive(Debug, Clone, Default)]
pub struct TtsOptions {
    /// 同一轮回复里所有句子共用一个段落 ID，跨句保持语气连贯。
    pub section_id: Option<String>,
    /// 引用上文（只读不合成）：一般放用户刚才说的话，模型据此承接情绪与停顿。
    pub context_text: Option<String>,
    /// 语音指令，如「用轻松自然的语气说」。
    pub instruction: Option<String>,
}

/// Text-to-speech. 返回 mp3 字节；文本太短不值得合成时返回 `None`。
#[async_trait]
pub trait Tts: Send + Sync {
    async fn synthesize(&self, text: &str, opts: &TtsOptions) -> Result<Option<Vec<u8>>>;
}
