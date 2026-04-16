use std::io::{Read as IoRead, Write};
use std::time::Instant;

use anyhow::{Context, Result};
use async_trait::async_trait;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

use crate::config::DoubaoConfig;
use crate::speech::Asr;

// WebSocket binary protocol frame headers (Doubao ASR v3)
const WS_FULL_CLIENT: [u8; 4] = [0x11, 0x10, 0x11, 0x00];
const WS_AUDIO_ONLY: [u8; 4] = [0x11, 0x20, 0x11, 0x00];
const WS_LAST_AUDIO: [u8; 4] = [0x11, 0x22, 0x11, 0x00];
const ASR_SEG_SIZE: usize = 160_000; // 5s @ 16kHz * 2 bytes

const TARGET_RATE: u32 = 16000;

use crate::ui::theme::{MUTED, RESET};

pub struct DoubaoAsr {
    cfg: DoubaoConfig,
    debug: bool,
}

impl DoubaoAsr {
    pub fn new(cfg: DoubaoConfig, debug: bool) -> Self {
        Self { cfg, debug }
    }
}

macro_rules! debug_log {
    ($self:expr, $($arg:tt)*) => {
        if $self.debug {
            eprintln!($($arg)*);
        }
    };
}

#[async_trait]
impl Asr for DoubaoAsr {
    async fn recognize(&self, wav_data: &[u8]) -> Result<(String, f32)> {
        let t0 = Instant::now();

        let connect_id = Uuid::new_v4().to_string();
        debug_log!(
            self,
            "   {MUTED}[DEBUG] ASR(WS v3) -> {} appid={}{RESET}",
            self.cfg.asr_url,
            self.cfg.app_id
        );

        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&self.cfg.asr_url)
            .header("X-Api-App-Key", &self.cfg.app_id)
            .header("X-Api-Access-Key", &self.cfg.access_token)
            .header("X-Api-Resource-Id", &self.cfg.asr_resource_id)
            .header("X-Api-Connect-Id", &connect_id)
            .header("Host", "openspeech.bytedance.com")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())?;

        let (mut ws, _) = tokio_tungstenite::connect_async(request)
            .await
            .context("ASR WebSocket connection failed")?;

        // 1. Send config frame
        let req_json = json!({
            "user": { "uid": "chatbox_cli" },
            "request": {
                "reqid": Uuid::new_v4().to_string(),
                "nbest": 1,
                "model_name": "bigmodel",
                "enable_punc": true,
                "enable_itn": true,
                "result_type": "full",
                "sequence": 1
            },
            "audio": {
                "format": "wav",
                "codec": "raw",
                "rate": TARGET_RATE,
                "bits": 16,
                "channel": 1,
                "language": "zh-CN"
            }
        });

        let frame = build_ws_frame(&WS_FULL_CLIENT, &serde_json::to_vec(&req_json)?);
        ws.send(WsMessage::Binary(frame)).await?;

        // 2. Send audio segments
        for chunk in wav_data.chunks(ASR_SEG_SIZE) {
            let frame = build_ws_frame(&WS_AUDIO_ONLY, chunk);
            ws.send(WsMessage::Binary(frame)).await?;
        }

        // 3. Send end-of-audio frame
        let finish = build_ws_frame(&WS_LAST_AUDIO, &[]);
        ws.send(WsMessage::Binary(finish)).await?;

        // 4. Read responses
        let mut last_text = String::new();
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);

        loop {
            let msg = match tokio::time::timeout_at(deadline, ws.next()).await {
                Ok(Some(Ok(msg))) => msg,
                Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
            };

            if let WsMessage::Binary(data) = msg {
                if let Ok(resp) = parse_asr_ws(&data) {
                    if !resp.result.text.is_empty() {
                        debug_log!(
                            self,
                            "   {MUTED}[DEBUG] Recognized: \"{}\"{RESET}",
                            resp.result.text
                        );
                        last_text = resp.result.text.clone();
                    }
                }
            } else if matches!(msg, WsMessage::Close(_)) {
                break;
            }
        }

        ws.close(None).await.ok();

        let elapsed = t0.elapsed().as_secs_f32() * 1000.0;
        Ok((last_text.trim().to_string(), elapsed))
    }
}

// === Internal helpers ===

fn gzip_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

fn gzip_decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut dec = GzDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out)?;
    Ok(out)
}

fn build_ws_frame(header: &[u8; 4], raw_data: &[u8]) -> Vec<u8> {
    let compressed = gzip_compress(raw_data).unwrap_or_else(|_| raw_data.to_vec());
    let mut frame = Vec::with_capacity(4 + 4 + compressed.len());
    frame.extend_from_slice(header);
    frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    frame.extend_from_slice(&compressed);
    frame
}

#[derive(Debug, Default, serde::Deserialize)]
struct AsrWsResponse {
    #[serde(default)]
    result: AsrResultItem,
}

#[derive(Debug, Default, serde::Deserialize)]
struct AsrResultItem {
    #[serde(default)]
    text: String,
}

fn parse_asr_ws(msg: &[u8]) -> Result<AsrWsResponse> {
    if msg.len() < 4 {
        anyhow::bail!("ASR response too short ({}B)", msg.len());
    }
    let header_size = (msg[0] & 0x0f) as usize;
    let message_type = msg[1] >> 4;
    let flags = msg[1] & 0x0f;
    let serialization = msg[2] >> 4;
    let compression = msg[2] & 0x0f;
    let header_bytes = header_size * 4;

    if msg.len() < header_bytes {
        anyhow::bail!("ASR header incomplete");
    }
    let payload = &msg[header_bytes..];
    let has_sequence = (flags & 0x01) != 0;

    let payload_msg: Option<&[u8]> = match message_type {
        // SERVER_FULL_RESPONSE
        0x09 => {
            let mut off = 0_usize;
            if has_sequence {
                off += 4;
            }
            if payload.len() < off + 4 {
                return Ok(AsrWsResponse::default());
            }
            let size = u32::from_be_bytes(payload[off..off + 4].try_into()?) as usize;
            off += 4;
            if size == 0 {
                None
            } else {
                Some(&payload[off..off + size])
            }
        }
        // SERVER_ACK
        0x0b => {
            if payload.len() >= 8 {
                let size = u32::from_be_bytes(payload[4..8].try_into()?) as usize;
                if size == 0 {
                    None
                } else {
                    Some(&payload[8..8 + size])
                }
            } else {
                None
            }
        }
        // SERVER_ERROR_RESPONSE
        0x0f => {
            if payload.len() < 8 {
                anyhow::bail!("Error payload too short");
            }
            let code = i32::from_be_bytes(payload[..4].try_into()?);
            let size = u32::from_be_bytes(payload[4..8].try_into()?) as usize;
            let data = &payload[8..8 + size];
            let decompressed = if compression == 1 {
                gzip_decompress(data)?
            } else {
                data.to_vec()
            };
            anyhow::bail!(
                "ASR error code={}: {}",
                code,
                String::from_utf8_lossy(&decompressed)
            );
        }
        _ => None,
    };

    let Some(payload_msg) = payload_msg else {
        return Ok(AsrWsResponse::default());
    };

    let decompressed = if compression == 1 {
        gzip_decompress(payload_msg)?
    } else {
        payload_msg.to_vec()
    };

    if serialization == 1 && !decompressed.is_empty() {
        Ok(serde_json::from_slice(&decompressed)?)
    } else {
        Ok(AsrWsResponse::default())
    }
}
