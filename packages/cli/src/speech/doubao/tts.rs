//! 豆包语音合成模型 2.0（HTTP chunked v3，`tts/unidirectional`）。
//!
//! 一次请求一句话：POST JSON，响应是按行分隔的 JSON 流，每行 `data` 是 base64 音频片段，
//! 拼起来就是完整 mp3。命中磁盘缓存（`cache/tts/<hash>.mp3`）时 0 延迟。
//! 文档：<https://www.volcengine.com/docs/6561/2528925>
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{auth_headers, credentials_hint, is_transient_error};
use crate::config::{cache_path, DoubaoConfig};
use crate::domain::sentence::is_speakable;
use crate::speech::{Tts, TtsOptions};
use crate::ui::theme::{MUTED, RESET};

const MAX_RETRIES: u32 = 1;
const AUDIO_FORMAT: &str = "mp3";
const SAMPLE_RATE: u32 = 24_000;

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
    async fn synthesize(&self, text: &str, opts: &TtsOptions) -> Result<Option<Vec<u8>>> {
        // 太短的片段（"。"、"呢"）引擎会 500，直接跳过
        if !is_speakable(text) {
            return Ok(None);
        }

        let instruction = build_instruction(opts);
        let cache_file = tts_cache_path(text, &self.cfg.voice_type, self.cfg.tts_speed, &instruction);
        if let Ok(bytes) = std::fs::read(&cache_file) {
            if !bytes.is_empty() {
                return Ok(Some(bytes));
            }
        }

        let mut attempt = 0;
        loop {
            match self.synthesize_once(text, opts, &instruction).await {
                Ok(audio) => {
                    write_cache(&cache_file, &audio);
                    return Ok(Some(audio));
                }
                Err(e) if attempt < MAX_RETRIES && is_transient_error(&e) => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// 每行一个 JSON 对象；`code` 0 为成功，`data` 为 base64 音频。
#[derive(Debug, Deserialize)]
struct TtsChunk {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<String>,
}

impl DoubaoTts {
    async fn synthesize_once(&self, text: &str, opts: &TtsOptions, instruction: &str) -> Result<Vec<u8>> {
        let request_id = Uuid::new_v4().to_string();
        // LLM 偶尔漏出 markdown / emoji，让服务端过滤掉别念出来；
        // section_id 让同一轮的多句共享语境；context_texts 是 2.0 的语音指令。
        let mut additions = json!({
            "disable_markdown_filter": true,
            "disable_emoji_filter": true
        });
        if let Some(sid) = &opts.section_id {
            additions["section_id"] = json!(sid);
        }
        if !instruction.is_empty() {
            additions["context_texts"] = json!([instruction]);
        }
        let body = json!({
            "user": { "uid": "chatbot_cli" },
            "req_params": {
                "text": text,
                "speaker": self.cfg.voice_type,
                "audio_params": {
                    "format": AUDIO_FORMAT,
                    "sample_rate": SAMPLE_RATE,
                    "speech_rate": speech_rate_from_ratio(self.cfg.tts_speed)
                },
                "additions": additions.to_string()
            }
        });

        let mut req = self
            .client
            .post(&self.cfg.tts_url)
            .header("Content-Type", "application/json")
            .header("X-Api-Resource-Id", &self.cfg.tts_resource_id)
            .header("X-Api-Request-Id", &request_id);
        for (k, v) in auth_headers(&self.cfg) {
            req = req.header(k, v);
        }
        let resp = req.json(&body).send().await.context("TTS request failed")?;

        let status = resp.status();
        let logid = resp
            .headers()
            .get("x-tt-logid")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-")
            .to_string();
        let raw = resp.bytes().await.context("TTS read body failed")?;

        if !status.is_success() {
            let snippet = String::from_utf8_lossy(&raw[..raw.len().min(300)]).to_string();
            if status.as_u16() == 401 || status.as_u16() == 403 {
                anyhow::bail!("TTS HTTP {status} (x-tt-logid={logid})：{}\n{snippet}", credentials_hint());
            }
            anyhow::bail!("TTS HTTP {status} (x-tt-logid={logid}) {snippet}");
        }

        let mut audio = Vec::new();
        for line in String::from_utf8_lossy(&raw).lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let chunk: TtsChunk = match serde_json::from_str(line) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("   {MUTED}TTS 响应行无法解析: {e} ({}...){RESET}", &line[..line.len().min(80)]);
                    continue;
                }
            };
            if chunk.code != 0 && chunk.code != 20_000_000 {
                anyhow::bail!(
                    "TTS error code={} msg={} (x-tt-logid={logid})",
                    chunk.code,
                    chunk.message
                );
            }
            if let Some(b64) = chunk.data.filter(|d| !d.is_empty()) {
                audio.extend_from_slice(&B64.decode(b64).context("TTS audio base64 decode failed")?);
            }
        }
        if audio.is_empty() {
            anyhow::bail!("TTS 返回空音频 (x-tt-logid={logid})");
        }
        Ok(audio)
    }
}

/// 用户面向的语速是倍率（0.5 ~ 2.0），2.0 接口要 `speech_rate` 整数：
/// `100` = 2 倍速，`-50` = 0.5 倍速，`0` = 原速，线性映射。
pub fn speech_rate_from_ratio(ratio: f64) -> i32 {
    (((ratio - 1.0) * 100.0).round() as i32).clamp(-50, 100)
}

/// 语音指令 = 用户配置的风格 + 承接上文的提示（上文截断，避免指令过长）。
fn build_instruction(opts: &TtsOptions) -> String {
    let mut s = opts.instruction.clone().unwrap_or_default().trim().to_string();
    if let Some(ctx) = opts.context_text.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        let short: String = ctx.chars().take(60).collect();
        if !s.is_empty() {
            s.push('，');
        }
        s.push_str(&format!("这句是在回应对方刚说的「{short}」，语气要承接上文"));
    }
    s
}

fn tts_cache_path(text: &str, voice: &str, speed: f64, instruction: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    voice.hash(&mut h);
    speed.to_bits().hash(&mut h);
    instruction.hash(&mut h);
    cache_path(&format!("cache/tts/{:016x}.mp3", h.finish()))
}

fn write_cache(path: &Path, bytes: &[u8]) {
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let tmp = parent.join(format!(".tmp-{}", Uuid::new_v4().simple()));
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_rate_mapping() {
        assert_eq!(speech_rate_from_ratio(1.0), 0);
        assert_eq!(speech_rate_from_ratio(1.3), 30);
        assert_eq!(speech_rate_from_ratio(2.0), 100);
        assert_eq!(speech_rate_from_ratio(0.5), -50);
        assert_eq!(speech_rate_from_ratio(3.0), 100);
        assert_eq!(speech_rate_from_ratio(0.1), -50);
    }

    #[test]
    fn cache_key_changes_with_voice_speed_and_instruction() {
        let a = tts_cache_path("你好", "v1", 1.0, "");
        let b = tts_cache_path("你好", "v2", 1.0, "");
        let c = tts_cache_path("你好", "v1", 1.5, "");
        let d = tts_cache_path("你好", "v1", 1.0, "温柔");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }

    #[test]
    fn instruction_combines_style_and_context() {
        let none = build_instruction(&TtsOptions::default());
        assert!(none.is_empty());
        let full = build_instruction(&TtsOptions {
            instruction: Some("自然地说".into()),
            context_text: Some("  今天心情不好 ".into()),
            section_id: None,
        });
        assert!(full.starts_with("自然地说，"));
        assert!(full.contains("「今天心情不好」"));
    }
}
