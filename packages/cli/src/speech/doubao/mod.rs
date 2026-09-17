//! 豆包（火山引擎 openspeech）语音适配器。
//!
//! - `asr`：豆包流式语音识别模型 2.0 —— WebSocket v3 `sauc/bigmodel_async`
//! - `tts`：豆包语音合成模型 2.0 —— HTTP chunked v3 `tts/unidirectional`
//! - `protocol`：v3 二进制帧编解码（ASR 用；TTS 走 JSON 行流）
//!
//! 鉴权两套并存：新版控制台单个 `X-Api-Key`；旧版控制台 App ID + Access Token。
//! 文档：<https://www.volcengine.com/docs/6561/2630027>（ASR）、
//! <https://www.volcengine.com/docs/6561/2528925>（TTS）。
pub mod asr;
pub mod protocol;
pub mod tts;

pub use asr::DoubaoAsr;
pub use tts::DoubaoTts;

use crate::config::DoubaoConfig;

/// 组装鉴权头。ASR 旧版叫 `X-Api-App-Key`，TTS 旧版叫 `X-Api-App-Id`，两个都带上无副作用。
pub(super) fn auth_headers(cfg: &DoubaoConfig) -> Vec<(&'static str, String)> {
    if !cfg.api_key.trim().is_empty() {
        vec![("X-Api-Key", cfg.api_key.trim().to_string())]
    } else {
        vec![
            ("X-Api-App-Key", cfg.app_id.clone()),
            ("X-Api-App-Id", cfg.app_id.clone()),
            ("X-Api-Access-Key", cfg.access_token.clone()),
        ]
    }
}

/// 值得重试一次的错误：5xx / 网络抖动 / 超时。401/403（凭证错）永不重试。
pub(super) fn is_transient_error(e: &anyhow::Error) -> bool {
    let msg = format!("{e:#}");
    if msg.contains("HTTP 401") || msg.contains("HTTP 403") || msg.contains("凭证") {
        return false;
    }
    msg.contains("HTTP 5")
        || msg.contains("connection")
        || msg.contains("Connection")
        || msg.contains("timed out")
        || msg.contains("reset")
        || msg.contains("dns")
        || msg.contains("Io")
}

pub(super) fn credentials_hint() -> &'static str {
    "请检查 speech.doubao.api_key（或 app_id + access_token）以及 2.0 服务是否已在控制台开通"
}

/// 端到端回环冒烟测试：TTS 2.0 合成一句 → 解码 → ASR 2.0 识别回来。
/// 需要真实凭证（读 ~/.config/chatbot/config.toml），所以默认 ignore：
/// `cargo test --manifest-path packages/cli/Cargo.toml doubao_loopback -- --ignored --nocapture`
#[cfg(test)]
mod loopback {
    use super::*;
    use crate::audio::resample::{downsample_to_mono_16k, encode_wav};
    use crate::audio::DeviceInfo;
    use crate::config::AppConfig;
    use crate::speech::{Asr, AsrContext, Tts, TtsOptions};
    use rodio::{Decoder, Source};
    use std::io::Cursor;

    #[tokio::test]
    #[ignore]
    async fn doubao_loopback() {
        let cfg = AppConfig::load().expect("config");
        assert!(cfg.speech.doubao.has_credentials(), "需要豆包凭证");
        let client = reqwest::Client::new();

        let tts = DoubaoTts::new(client, cfg.speech.doubao.clone());
        let mp3 = tts
            .synthesize(
                "你好，今天天气怎么样？",
                &TtsOptions {
                    section_id: Some("loopback-test".into()),
                    context_text: Some("我刚下班，好累".into()),
                    instruction: Some(cfg.speech.doubao.voice_instruction.clone()),
                },
            )
            .await
            .expect("tts request")
            .expect("tts audio");
        assert!(mp3.len() > 1000, "mp3 too small: {}B", mp3.len());

        let decoder = Decoder::new(Cursor::new(mp3)).expect("mp3 decode");
        let info = DeviceInfo { sample_rate: decoder.sample_rate(), channels: decoder.channels() };
        let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();
        let mono16k = downsample_to_mono_16k(&samples, info);
        let wav = encode_wav(&mono16k).expect("wav");

        let asr = DoubaoAsr::new(cfg.speech.doubao.clone(), true);
        let ctx = AsrContext { hotwords: vec!["天气".into()], dialog: vec![] };
        let (text, ms) = asr.recognize(&wav, &ctx).await.expect("asr");
        eprintln!("ASR 2.0 => {text:?} ({ms:.0}ms)");
        assert!(text.contains("天气"), "unexpected asr text: {text:?}");
    }
}
