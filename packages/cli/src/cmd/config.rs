use anyhow::Result;

use crate::config::{AppConfig, CONFIG_PATH_DISPLAY};

/// Run the full interactive config wizard
pub async fn run_wizard() -> Result<()> {
    println!("cb config: interactive setup wizard");
    println!("Config file: {CONFIG_PATH_DISPLAY}");
    println!();

    let mut cfg = AppConfig::load().unwrap_or_default();
    prompt_all_fields(&mut cfg)?;
    cfg.save()?;
    println!("\nConfig saved to {CONFIG_PATH_DISPLAY}");
    Ok(())
}

/// Ensure required config is present. If missing, prompt user interactively.
/// Returns a complete, validated AppConfig.
pub fn ensure_config(mut cfg: AppConfig) -> Result<AppConfig> {
    if cfg.is_complete() {
        return Ok(cfg);
    }

    println!("  Missing required configuration. Let's set it up.\n");

    // Only prompt for missing fields
    if cfg.llm.api_key.is_empty() {
        cfg.llm.api_key = prompt_required("LLM API Key (AI_API_KEY)")?;
    }
    if cfg.llm.base_url.is_empty() {
        cfg.llm.base_url = "https://api.deepseek.com".into();
    }
    // Always show base_url and model so user can change defaults
    if let Some(v) = prompt_optional("LLM Base URL", &cfg.llm.base_url)? {
        cfg.llm.base_url = v;
    }
    if let Some(v) = prompt_optional("LLM Model", &cfg.llm.model)? {
        cfg.llm.model = v;
    }

    if cfg.speech.doubao.app_id.is_empty() {
        cfg.speech.doubao.app_id = prompt_required("Doubao App ID (DOUBAO_APP_ID)")?;
    }
    if cfg.speech.doubao.access_token.is_empty() {
        cfg.speech.doubao.access_token =
            prompt_required("Doubao Access Token (DOUBAO_ACCESS_TOKEN)")?;
    }

    cfg.save()?;
    println!("  Config saved to {CONFIG_PATH_DISPLAY}\n");
    Ok(cfg)
}

pub fn show() -> Result<()> {
    let cfg = AppConfig::load()?;
    let toml_str = toml::to_string_pretty(&cfg)?;
    println!("{toml_str}");
    Ok(())
}

pub fn set(key: &str, value: &str) -> Result<()> {
    let mut cfg = AppConfig::load().unwrap_or_default();

    match key {
        "locale" => {
            if value != "en" && value != "zh" {
                anyhow::bail!("locale must be 'en' or 'zh'");
            }
            cfg.locale = value.to_string();
        }
        "llm.api_key" => cfg.llm.api_key = value.to_string(),
        "llm.base_url" => cfg.llm.base_url = value.to_string(),
        "llm.model" => cfg.llm.model = value.to_string(),
        "speech.doubao.app_id" => cfg.speech.doubao.app_id = value.to_string(),
        "speech.doubao.access_token" => cfg.speech.doubao.access_token = value.to_string(),
        "speech.doubao.voice_type" => cfg.speech.doubao.voice_type = value.to_string(),
        "speech.doubao.tts_speed" => {
            cfg.speech.doubao.tts_speed =
                value.parse().map_err(|_| anyhow::anyhow!("invalid number"))?;
        }
        "audio.silence_seconds" => {
            cfg.audio.silence_seconds =
                value.parse().map_err(|_| anyhow::anyhow!("invalid number"))?;
        }
        "audio.min_speech_seconds" => {
            cfg.audio.min_speech_seconds =
                value.parse().map_err(|_| anyhow::anyhow!("invalid number"))?;
        }
        "wake_word.enabled" => {
            cfg.wake_word.enabled = matches!(value, "true" | "1" | "yes" | "on");
        }
        "wake_word.word_en" => cfg.wake_word.word_en = value.to_string(),
        "wake_word.word_zh" => cfg.wake_word.word_zh = value.to_string(),
        _ => anyhow::bail!("unknown config key: {key}"),
    }

    cfg.save()?;
    println!("Set {key} = {value}");
    Ok(())
}

// === Prompt helpers ===

fn prompt_all_fields(cfg: &mut AppConfig) -> Result<()> {
    println!("== General ==");
    if let Some(v) = prompt_optional("Language / 语言 (en/zh)", &cfg.locale)? {
        if v == "en" || v == "zh" {
            cfg.locale = v;
        } else {
            println!("  (invalid, keeping current: {})", cfg.locale);
        }
    }

    println!("\n== LLM Settings ==");
    if let Some(v) = prompt_optional("API Base URL", &cfg.llm.base_url)? {
        cfg.llm.base_url = v;
    }
    if cfg.llm.api_key.is_empty() {
        cfg.llm.api_key = prompt_required("API Key")?;
    } else if let Some(v) = prompt_optional("API Key", &mask_key(&cfg.llm.api_key))? {
        cfg.llm.api_key = v;
    }
    if let Some(v) = prompt_optional("Model", &cfg.llm.model)? {
        cfg.llm.model = v;
    }

    println!("\n== Wake Word ==");
    let enabled_str = if cfg.wake_word.enabled { "on" } else { "off" };
    if let Some(v) = prompt_optional("Wake word mode (on/off)", enabled_str)? {
        cfg.wake_word.enabled = matches!(v.as_str(), "on" | "true" | "1" | "yes");
    }
    if cfg.wake_word.enabled {
        if let Some(v) = prompt_optional("Wake word (EN)", &cfg.wake_word.word_en)? {
            cfg.wake_word.word_en = v;
        }
        if let Some(v) = prompt_optional("Wake word (ZH)", &cfg.wake_word.word_zh)? {
            cfg.wake_word.word_zh = v;
        }
    }

    println!("\n== Speech Settings (Doubao) ==");
    if cfg.speech.doubao.app_id.is_empty() {
        cfg.speech.doubao.app_id = prompt_required("App ID")?;
    } else if let Some(v) = prompt_optional("App ID", &cfg.speech.doubao.app_id)? {
        cfg.speech.doubao.app_id = v;
    }
    if cfg.speech.doubao.access_token.is_empty() {
        cfg.speech.doubao.access_token = prompt_required("Access Token")?;
    } else if let Some(v) =
        prompt_optional("Access Token", &mask_key(&cfg.speech.doubao.access_token))?
    {
        cfg.speech.doubao.access_token = v;
    }

    Ok(())
}

fn prompt_required(label: &str) -> Result<String> {
    loop {
        print!("  {label}: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        println!("  (required, cannot be empty)");
    }
}

fn prompt_optional(label: &str, current: &str) -> Result<Option<String>> {
    print!("  {label} [{current}]: ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        "*".repeat(key.len())
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}
