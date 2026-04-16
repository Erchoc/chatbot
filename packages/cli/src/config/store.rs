use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const CONFIG_PATH_DISPLAY: &str = "~/.config/chatbox/config.toml";

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("chatbox")
        .join("config.toml")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Display language: "en" or "zh"
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default)]
    pub wake_word: WakeWordConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub speech: SpeechConfig,
    #[serde(default)]
    pub audio: AudioConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordConfig {
    /// Enable wake word mode
    pub enabled: bool,
    /// Wake word for English mode
    pub word_en: String,
    /// Wake word for Chinese mode
    pub word_zh: String,
}

fn default_locale() -> String {
    "en".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechConfig {
    pub provider: String,
    #[serde(default)]
    pub doubao: DoubaoConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoubaoConfig {
    pub app_id: String,
    pub access_token: String,
    pub tts_cluster: String,
    pub asr_resource_id: String,
    pub tts_resource_id: String,
    pub voice_type: String,
    pub tts_speed: f32,
    pub tts_url: String,
    pub asr_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub silence_seconds: f32,
    pub min_speech_seconds: f32,
}

// === Defaults ===

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            locale: default_locale(),
            wake_word: WakeWordConfig::default(),
            llm: LlmConfig::default(),
            speech: SpeechConfig::default(),
            audio: AudioConfig::default(),
        }
    }
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            word_en: "Hi Pai".into(),
            word_zh: "嘿小派".into(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.deepseek.com".into(),
            model: "deepseek-chat".into(),
        }
    }
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            provider: "doubao".into(),
            doubao: DoubaoConfig::default(),
        }
    }
}

impl Default for DoubaoConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            access_token: String::new(),
            tts_cluster: "volcano_tts".into(),
            asr_resource_id: "volc.bigasr.sauc.duration".into(),
            tts_resource_id: "volc.service_type.10029".into(),
            voice_type: "BV700_V2_streaming".into(),
            tts_speed: 1.3,
            tts_url: "https://openspeech.bytedance.com/api/v1/tts".into(),
            asr_url: "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async".into(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            silence_seconds: 1.0,
            min_speech_seconds: 1.0,
        }
    }
}

impl AppConfig {
    /// Load config: config.toml -> env var overrides
    pub fn load() -> Result<Self> {
        let path = config_path();
        let mut cfg = if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read config: {}", path.display()))?;
            toml::from_str(&content)
                .with_context(|| format!("failed to parse config: {}", path.display()))?
        } else {
            Self::default()
        };

        // Env var overrides (compatible with remote_chat .env)
        cfg.apply_env_overrides();
        Ok(cfg)
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("AI_API_KEY") {
            self.llm.api_key = v;
        }
        if let Ok(v) = std::env::var("AI_BASE_URL") {
            self.llm.base_url = v;
        }
        if let Ok(v) = std::env::var("AI_MODEL") {
            self.llm.model = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_APP_ID") {
            self.speech.doubao.app_id = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_ACCESS_TOKEN") {
            self.speech.doubao.access_token = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_TTS_CLUSTER") {
            self.speech.doubao.tts_cluster = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_ASR_RESOURCE_ID") {
            self.speech.doubao.asr_resource_id = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_TTS_RESOURCE_ID") {
            self.speech.doubao.tts_resource_id = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_VOICE_TYPE") {
            self.speech.doubao.voice_type = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_TTS_URL") {
            self.speech.doubao.tts_url = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_ASR_URL") {
            self.speech.doubao.asr_url = v;
        }
    }

    /// Check if all required fields are filled (not empty, not placeholder)
    pub fn is_complete(&self) -> bool {
        is_real_value(&self.llm.api_key)
            && is_real_value(&self.speech.doubao.app_id)
            && is_real_value(&self.speech.doubao.access_token)
    }

    /// Validate required config fields
    pub fn validate(&self) -> Result<()> {
        if self.llm.api_key.is_empty() {
            anyhow::bail!("Missing LLM API Key. Run `cb config` or set AI_API_KEY env var");
        }
        if self.speech.doubao.app_id.is_empty() {
            anyhow::bail!("Missing Doubao App ID. Run `cb config` or set DOUBAO_APP_ID env var");
        }
        if self.speech.doubao.access_token.is_empty() {
            anyhow::bail!(
                "Missing Doubao Access Token. Run `cb config` or set DOUBAO_ACCESS_TOKEN env var"
            );
        }
        Ok(())
    }
}

/// Check if a config value is real (not empty, not a placeholder like "your-xxx-here")
fn is_real_value(val: &str) -> bool {
    !val.is_empty() && !val.contains("your-") && !val.contains("xxx") && val != "placeholder"
}
