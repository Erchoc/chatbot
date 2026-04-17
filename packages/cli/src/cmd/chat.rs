use std::io::Write;

use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};

use crate::cmd::config::ensure_config;
use crate::config::AppConfig;
use crate::llm::OpenAiClient;
use crate::pipeline::voice::VoicePipeline;
use crate::ui::theme::*;

/// Voice chat mode (default when `cb` or `cb chat` with no args).
pub async fn run_voice(debug: bool) -> Result<()> {
    let cfg = AppConfig::load()?;
    let cfg = ensure_config(cfg)?;
    crate::update_check::spawn_background_check();
    let mut pipeline = VoicePipeline::new(cfg, debug)?;
    pipeline.run_loop().await
}

/// Single-shot text chat mode: `cb chat 你好呀`.
pub async fn run_text(message: &str, _debug: bool) -> Result<()> {
    let cfg = AppConfig::load()?;
    let cfg = ensure_config(cfg)?;
    crate::update_check::spawn_background_check();

    let llm_config = cfg
        .active_llm_config()
        .ok_or_else(|| anyhow::anyhow!("No active LLM profile. Run `cb config`"))?;

    let system_prompt = crate::i18n::build_system_prompt(
        &cfg.persona.language,
        &cfg.persona.name,
        cfg.persona.wake_word.enabled,
        &cfg.persona.wake_word.word,
    );

    let messages: Vec<Value> = vec![
        json!({"role": "system", "content": system_prompt}),
        json!({"role": "user", "content": message}),
    ];

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let llm = OpenAiClient::new(client, llm_config);

    // Print prompt
    println!("   {USER_COLOR}🗣 {message}{RESET}");
    print!("   {BOT_COLOR}🤖 {RESET}");
    std::io::stdout().flush()?;

    // Stream reply — use a dummy channel, just for stdout printing
    let (token_tx, _token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let result = llm.chat_stream(&messages, token_tx).await?;

    println!();
    println!(
        "   {MUTED}TTFT {:.1}s | {}tok {:.0}tok/s | total {:.1}s{RESET}",
        result.ttft_ms / 1000.0,
        result.tokens,
        if result.total_ms > 0.0 {
            result.tokens as f32 / (result.total_ms / 1000.0)
        } else {
            0.0
        },
        result.total_ms / 1000.0,
    );

    if let Some(v) = crate::update_check::pending_notice() {
        println!("   {BR_CYAN}⬆  发现新版本 v{v}，运行 {BOLD}cb update{RESET}{BR_CYAN} 升级{RESET}");
    }

    Ok(())
}
