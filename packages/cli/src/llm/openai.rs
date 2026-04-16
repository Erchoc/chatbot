use std::io::Write;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::config::LlmConfig;

/// OpenAI-compatible streaming LLM client
pub struct OpenAiClient {
    client: Client,
    cfg: LlmConfig,
}

/// Streaming result
pub struct StreamResult {
    pub reply: String,
    pub tokens: usize,
    pub ttft_ms: f32,
    pub total_ms: f32,
}

impl OpenAiClient {
    pub fn new(client: Client, cfg: LlmConfig) -> Self {
        Self { client, cfg }
    }

    /// Stream a chat completion, sending each token via channel and printing to stdout
    pub async fn chat_stream(
        &self,
        messages: &[Value],
        token_tx: mpsc::UnboundedSender<String>,
    ) -> Result<StreamResult> {
        let t0 = std::time::Instant::now();
        let mut ttft_ms = 0.0_f32;
        let mut reply = String::new();
        let mut tokens = 0_usize;

        let url = format!(
            "{}/chat/completions",
            self.cfg.base_url.trim_end_matches('/')
        );

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.cfg.api_key))
            .header("Content-Type", "application/json")
            .json(&json!({
                "model": self.cfg.model,
                "messages": messages,
                "max_tokens": 1000,
                "stream": true
            }))
            .send()
            .await
            .context("LLM request failed")?
            .error_for_status()
            .context("LLM returned error status")?;

        let mut byte_stream = resp.bytes_stream();
        let mut buf = String::new();

        'outer: while let Some(chunk_result) = byte_stream.next().await {
            let chunk = chunk_result?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = buf.find('\n') {
                let line = buf[..newline_pos].trim().to_string();
                buf = buf[newline_pos + 1..].to_string();

                let data = match line.strip_prefix("data: ") {
                    Some(d) => d,
                    None => continue,
                };

                if data == "[DONE]" {
                    break 'outer;
                }

                let v: Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let content = match v
                    .pointer("/choices/0/delta/content")
                    .and_then(|c| c.as_str())
                {
                    Some(c) if !c.is_empty() => c.to_string(),
                    _ => continue,
                };

                if ttft_ms == 0.0 {
                    ttft_ms = t0.elapsed().as_secs_f32() * 1000.0;
                }

                reply.push_str(&content);
                tokens += 1;
                print!("{content}");
                std::io::stdout().flush()?;
                let _ = token_tx.send(content);
            }
        }

        let total_ms = t0.elapsed().as_secs_f32() * 1000.0;

        Ok(StreamResult {
            reply,
            tokens,
            ttft_ms,
            total_ms,
        })
    }
}
