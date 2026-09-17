//! 豆包流式语音识别模型 2.0（WebSocket v3，`sauc/bigmodel_async`）。
//!
//! 用法是"批量"的：VAD 已经在本地切好一句，这里一次把整段 PCM 分片发出去，
//! 打上最后一包标记，然后等服务端的终包（`enable_nonstream` 二遍识别结果更准）。
//! 文档：<https://www.volcengine.com/docs/6561/2630027>
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Error as WsError, Message as WsMessage};
use uuid::Uuid;

use super::protocol::{self as p, MSG_AUDIO_ONLY, MSG_FULL_CLIENT, MSG_FULL_SERVER};
use super::{auth_headers, credentials_hint, is_transient_error};
use crate::config::DoubaoConfig;
use crate::speech::{Asr, AsrContext, Speaker};
use crate::ui::theme::{MUTED, RESET};

/// 每包音频 200ms @16kHz 16bit mono（官方推荐 100~200ms）。
const CHUNK_BYTES: usize = 16_000 * 2 / 5;
const MAX_RETRIES: u32 = 1;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const TARGET_RATE: u32 = 16_000;

pub struct DoubaoAsr {
    cfg: DoubaoConfig,
    debug: bool,
}

impl DoubaoAsr {
    pub fn new(cfg: DoubaoConfig, debug: bool) -> Self {
        Self { cfg, debug }
    }

    fn debug(&self, msg: impl std::fmt::Display) {
        if self.debug {
            eprintln!("   {MUTED}[DEBUG] {msg}{RESET}");
        }
    }
}

#[async_trait]
impl Asr for DoubaoAsr {
    async fn recognize(&self, wav_data: &[u8], ctx: &AsrContext) -> Result<(String, f32)> {
        let t0 = Instant::now();
        let mut attempt = 0;
        loop {
            match self.recognize_once(wav_data, ctx).await {
                Ok(text) => return Ok((text, t0.elapsed().as_secs_f32() * 1000.0)),
                Err(e) if attempt < MAX_RETRIES && is_transient_error(&e) => {
                    attempt += 1;
                    self.debug(format!("ASR transient error, retrying ({attempt}/{MAX_RETRIES}): {e:#}"));
                    tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl DoubaoAsr {
    async fn recognize_once(&self, wav_data: &[u8], ctx: &AsrContext) -> Result<String> {
        let request_id = Uuid::new_v4().to_string();
        self.debug(format!("ASR 2.0 -> {} resource={} req={request_id}", self.cfg.asr_url, self.cfg.asr_resource_id));

        let mut request = self.cfg.asr_url.as_str().into_client_request()?;
        {
            let headers = request.headers_mut();
            for (k, v) in auth_headers(&self.cfg) {
                headers.insert(k, v.parse()?);
            }
            headers.insert("X-Api-Resource-Id", self.cfg.asr_resource_id.parse()?);
            headers.insert("X-Api-Request-Id", request_id.parse()?);
            headers.insert("X-Api-Connect-Id", Uuid::new_v4().to_string().parse()?);
        }

        let (mut ws, _) = tokio_tungstenite::connect_async(request).await.map_err(|e| match e {
            WsError::Http(resp) => {
                let status = resp.status();
                let logid = resp.headers().get("x-tt-logid").and_then(|v| v.to_str().ok()).unwrap_or("-");
                if status.as_u16() == 401 || status.as_u16() == 403 {
                    anyhow::anyhow!("ASR 连接被拒 HTTP {status} (x-tt-logid={logid})：{}", credentials_hint())
                } else {
                    anyhow::anyhow!("ASR WebSocket connection failed: HTTP {status} (x-tt-logid={logid})")
                }
            }
            other => anyhow::anyhow!("ASR WebSocket connection failed: {other}"),
        })?;

        // 1. full client request（JSON + gzip）
        let mut req = json!({
            "user": { "uid": "chatbot_cli" },
            "audio": {
                "format": "pcm",
                "codec": "raw",
                "rate": TARGET_RATE,
                "bits": 16,
                "channel": 1
            },
            "request": {
                "model_name": "bigmodel",
                "enable_nonstream": true,
                "enable_itn": true,
                "enable_punc": true,
                "enable_ddc": true,
                "result_type": "full",
                "end_window_size": 800
            }
        });
        if let Some(corpus) = build_corpus(ctx) {
            req["request"]["corpus"] = corpus;
        }
        let frame = p::encode(MSG_FULL_CLIENT, p::FLAG_NONE, p::SER_JSON, p::COMP_GZIP, &serde_json::to_vec(&req)?)?;
        ws.send(WsMessage::Binary(frame)).await?;

        // 2. 音频分片（PCM 压不动，不 gzip）；最后一包打 LAST 标记
        let pcm = strip_wav_header(wav_data);
        let chunks: Vec<&[u8]> = pcm.chunks(CHUNK_BYTES).collect();
        let n = chunks.len();
        for (i, chunk) in chunks.iter().enumerate() {
            let flags = if i + 1 == n { p::FLAG_LAST } else { p::FLAG_NONE };
            let frame = p::encode(MSG_AUDIO_ONLY, flags, p::SER_NONE, p::COMP_NONE, chunk)?;
            ws.send(WsMessage::Binary(frame)).await?;
        }
        if n == 0 {
            ws.send(WsMessage::Binary(p::encode(MSG_AUDIO_ONLY, p::FLAG_LAST, p::SER_NONE, p::COMP_NONE, &[])?)).await?;
        }

        // 3. 收结果直到终包
        let mut last_text = String::new();
        let deadline = tokio::time::Instant::now() + RESPONSE_TIMEOUT;
        loop {
            let msg = match tokio::time::timeout_at(deadline, ws.next()).await {
                Ok(Some(Ok(msg))) => msg,
                Ok(Some(Err(e))) => {
                    self.debug(format!("ASR ws error: {e}"));
                    break;
                }
                Ok(None) => break,
                Err(_) => {
                    self.debug("ASR response timeout");
                    break;
                }
            };
            match msg {
                WsMessage::Binary(data) => {
                    let frame = p::decode(&data)?;
                    if frame.msg_type == MSG_FULL_SERVER && frame.is_json && !frame.payload.is_empty() {
                        if let Ok(resp) = serde_json::from_slice::<AsrResponse>(&frame.payload) {
                            if !resp.result.text.is_empty() {
                                last_text = resp.result.text;
                            }
                        }
                    }
                    if frame.is_last() {
                        break;
                    }
                }
                WsMessage::Close(_) => break,
                _ => {}
            }
        }
        ws.close(None).await.ok();

        self.debug(format!("Recognized: {last_text:?}"));
        Ok(last_text.trim().to_string())
    }
}

/// 热词 + 最近对话 → `request.corpus.context`（服务端要求 JSON 序列化成字符串）。
/// 文档：https://www.volcengine.com/docs/6561/2630027 「corpus」
fn build_corpus(ctx: &AsrContext) -> Option<serde_json::Value> {
    let hotwords: Vec<_> = ctx
        .hotwords
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .map(|w| json!({ "word": w }))
        .collect();
    let dialog: Vec<_> = ctx
        .dialog
        .iter()
        .filter(|t| !t.text.trim().is_empty())
        .map(|t| {
            let speaker = match t.speaker {
                Speaker::User => "user",
                Speaker::Bot => "bot",
            };
            json!({ "speaker": speaker, "text": t.text.chars().take(200).collect::<String>() })
        })
        .collect();
    if hotwords.is_empty() && dialog.is_empty() {
        return None;
    }
    let mut context = json!({});
    if !hotwords.is_empty() {
        context["hotwords"] = json!(hotwords);
    }
    if !dialog.is_empty() {
        context["context_type"] = json!("dialog_ctx");
        context["context_data"] = json!(dialog);
    }
    Some(json!({ "context": context.to_string() }))
}

/// 输入是 `encode_wav` 产出的 44 字节标准头 WAV；服务端要裸 PCM。
fn strip_wav_header(data: &[u8]) -> &[u8] {
    if data.len() >= 44 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        &data[44..]
    } else {
        data
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct AsrResponse {
    #[serde(default)]
    result: AsrResult,
}

#[derive(Debug, Default, serde::Deserialize)]
struct AsrResult {
    #[serde(default)]
    text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_serializes_hotwords_and_dialog_as_json_string() {
        use crate::speech::DialogTurn;
        let ctx = AsrContext {
            hotwords: vec!["小派".into(), " ".into()],
            dialog: vec![
                DialogTurn { speaker: Speaker::User, text: "你好".into() },
                DialogTurn { speaker: Speaker::Bot, text: "嗨".into() },
            ],
        };
        let corpus = build_corpus(&ctx).unwrap();
        let inner: serde_json::Value =
            serde_json::from_str(corpus["context"].as_str().unwrap()).unwrap();
        assert_eq!(inner["hotwords"], json!([{ "word": "小派" }]));
        assert_eq!(inner["context_type"], "dialog_ctx");
        assert_eq!(inner["context_data"][1]["speaker"], "bot");
        assert!(build_corpus(&AsrContext::default()).is_none());
    }

    #[test]
    fn strips_riff_header_only_when_present() {
        let mut wav = b"RIFF\0\0\0\0WAVE".to_vec();
        wav.resize(44, 0);
        wav.extend_from_slice(&[1, 2, 3]);
        assert_eq!(strip_wav_header(&wav), &[1, 2, 3]);
        assert_eq!(strip_wav_header(&[9, 9]), &[9, 9]);
    }
}
