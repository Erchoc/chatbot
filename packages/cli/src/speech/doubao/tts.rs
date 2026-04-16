use anyhow::Result;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::DoubaoConfig;
use crate::speech::Tts;

use crate::ui::theme::{MUTED, RESET};

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
        if text.is_empty() {
            return Ok(None);
        }

        let body = json!({
            "app": {
                "appid": self.cfg.app_id,
                "token": self.cfg.access_token,
                "cluster": self.cfg.tts_cluster
            },
            "user": { "uid": "chatbox_cli" },
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
        let result: Value = resp.json().await?;

        if !status.is_success() {
            let msg = result
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
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
