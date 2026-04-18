use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::{cache_path, DoubaoConfig};
use crate::speech::Tts;

use crate::ui::theme::{MUTED, RESET};

const MAX_RETRIES: u32 = 1;

pub struct DoubaoTts {
    client: Client,
    cfg: DoubaoConfig,
}

impl DoubaoTts {
    pub fn new(client: Client, cfg: DoubaoConfig) -> Self {
        Self { client, cfg }
    }
}

#[async_trait]
impl Tts for DoubaoTts {
    async fn synthesize(&self, text: &str) -> Result<Option<Vec<u8>>> {
        // Skip empty or very short text — the TTS engine returns 500
        // ("Init Engine Instance failed") on fragments like "。" or "呢"
        let meaningful: usize = text
            .trim()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .count();
        if meaningful < 2 {
            return Ok(None);
        }

        let cache_file = tts_cache_path(text, &self.cfg.voice_type, self.cfg.tts_speed);
        if let Ok(bytes) = std::fs::read(&cache_file) {
            if !bytes.is_empty() {
                return Ok(Some(bytes));
            }
        }

        let mut attempt: u32 = 0;
        loop {
            match self.synthesize_once(text).await {
                Ok(Some(mp3)) => {
                    write_cache(&cache_file, &mp3);
                    return Ok(Some(mp3));
                }
                Ok(None) => return Ok(None),
                Err(e) => {
                    if attempt >= MAX_RETRIES || !is_transient_tts_error(&e) {
                        return Err(e);
                    }
                    attempt += 1;
                    tokio::time::sleep(tokio::time::Duration::from_millis(500 * attempt as u64))
                        .await;
                }
            }
        }
    }
}

impl DoubaoTts {
    async fn synthesize_once(&self, text: &str) -> Result<Option<Vec<u8>>> {
        let body = json!({
            "app": {
                "appid": self.cfg.app_id,
                "token": self.cfg.access_token,
                "cluster": self.cfg.tts_cluster
            },
            "user": { "uid": "chatbot_cli" },
            "audio": {
                "voice_type": self.cfg.voice_type,
                "encoding": "mp3",
                "speed_ratio": self.cfg.tts_speed,
                "volume_ratio": 1.0,
                "pitch_ratio": 1.0
            },
            "request": {
                "reqid": Uuid::new_v4().to_string(),
                "text": text,
                "text_type": "plain",
                "operation": "query"
            }
        });

        let resp = self
            .client
            .post(&self.cfg.tts_url)
            .header("Content-Type", "application/json")
            .header(
                "Authorization",
                format!("Bearer;{}", self.cfg.access_token),
            )
            .header("X-Api-App-Key", &self.cfg.app_id)
            .header("X-Api-Access-Key", &self.cfg.access_token)
            .header("X-Api-Resource-Id", &self.cfg.tts_resource_id)
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let result: Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                // Non-JSON 5xx — signal transient so the retry wrapper can decide.
                if status.is_server_error() {
                    anyhow::bail!("TTS HTTP {status} (body not JSON: {e})");
                }
                eprintln!("   {MUTED}TTS response not JSON: {e}{RESET}");
                return Ok(None);
            }
        };

        if !status.is_success() {
            let msg = result
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            if status == reqwest::StatusCode::UNAUTHORIZED {
                eprintln!("   {MUTED}TTS request failed: HTTP {status} - {msg}{RESET}");
                if msg.contains("load grant") {
                    eprintln!(
                        "   {MUTED}Hint: TTS 授权缺失或参数不匹配，请检查 app_id/access_token/tts_resource_id 是否同属一个语音应用{RESET}"
                    );
                }
                return Ok(None);
            }
            if status.is_server_error() {
                anyhow::bail!("TTS HTTP {status} - {msg}");
            }
            eprintln!("   {MUTED}TTS request failed: HTTP {status} - {msg}{RESET}");
            return Ok(None);
        }

        let code = result.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
        if code != 3000 {
            let msg = result
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            eprintln!("   {MUTED}TTS response: code={code} msg={msg}{RESET}");
            return Ok(None);
        }

        let audio_b64 = match result.get("data").and_then(|d| d.as_str()) {
            Some(d) => d,
            None => {
                eprintln!("   {MUTED}TTS response missing 'data' field{RESET}");
                return Ok(None);
            }
        };

        let mp3_bytes = B64.decode(audio_b64)?;
        Ok(Some(mp3_bytes))
    }
}

fn tts_cache_path(text: &str, voice: &str, speed: f64) -> PathBuf {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    voice.hash(&mut h);
    speed.to_bits().hash(&mut h);
    let hex = format!("{:016x}", h.finish());
    cache_path(&format!("cache/tts/{hex}.mp3"))
}

fn write_cache(path: &PathBuf, bytes: &[u8]) {
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let tmp = parent.join(format!(
        ".tmp-{}",
        Uuid::new_v4().simple()
    ));
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn is_transient_tts_error(e: &anyhow::Error) -> bool {
    let msg = format!("{e}");
    if msg.contains("HTTP 401") {
        return false;
    }
    msg.contains("HTTP 5")
        || msg.contains("connection")
        || msg.contains("Connection")
        || msg.contains("timed out")
        || msg.contains("reset")
        || msg.contains("dns")
}
