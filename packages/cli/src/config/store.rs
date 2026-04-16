use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn config_dir() -> PathBuf {
    // Always use ~/.config/chatbot/ for cross-platform consistency.
    // macOS dirs::config_dir() returns ~/Library/Application Support/ which
    // is hard to type (spaces) and unexpected for CLI tool users.
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("chatbot")
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn config_path_display() -> String {
    "~/.config/chatbot/config.toml".to_string()
}

fn migrate_from_dir(old_dir: &PathBuf, new_path: &PathBuf) -> bool {
    let old_path = old_dir.join("config.toml");
    if !old_path.exists() {
        return false;
    }

    if let Some(parent) = new_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if std::fs::copy(&old_path, new_path).is_err() {
        return false;
    }

    let _ = std::fs::remove_file(&old_path);

    // Also migrate persisted runtime data if present.
    let new_root = config_dir();
    for name in ["history", "events"] {
        let old_data = old_dir.join(name);
        let new_data = new_root.join(name);
        if old_data.exists() && !new_data.exists() {
            let _ = std::fs::rename(old_data, new_data);
        }
    }

    // Clean up old dir if it is empty after migration.
    let _ = std::fs::remove_dir(old_dir);
    true
}

/// Migrate config from historical locations (chatbox/chatbot, macOS/Linux)
/// to the unified path (~/.config/chatbot/). Called once at startup.
pub fn migrate_config_path() {
    let new_path = config_path();
    if new_path.exists() {
        return; // already using the new location
    }

    let mut candidates: Vec<PathBuf> = vec![];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".config").join("chatbox"));
        candidates.push(home.join(".config").join("chatbot"));
    }

    if let Some(old_dir) = dirs::config_dir() {
        // macOS historical path under ~/Library/Application Support/
        candidates.push(old_dir.join("chatbox"));
        candidates.push(old_dir.join("chatbot"));
    }

    for old in candidates {
        if migrate_from_dir(&old, &new_path) {
            break;
        }
    }
}

// ─── Top-level config ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub persona: PersonaConfig,

    /// Name of the currently active LLM profile
    #[serde(default)]
    pub active_llm: String,

    /// All configured LLM profiles (one active at a time)
    #[serde(default)]
    pub llm_profiles: Vec<LlmProfile>,

    #[serde(default)]
    pub speech: SpeechConfig,

    #[serde(default)]
    pub audio: AudioConfig,

    // ── Legacy fields – read-only on load, never written back ────────────────
    #[serde(default, skip_serializing)]
    pub locale: Option<String>,

    #[serde(default, skip_serializing)]
    pub llm: Option<LegacyLlmConfig>,

    #[serde(default, skip_serializing)]
    pub wake_word: Option<LegacyWakeWordConfig>,
}

// ─── Persona ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaConfig {
    /// Display name of the assistant (used in system prompt and UI)
    #[serde(default = "default_name")]
    pub name: String,

    /// UI + system prompt language: "zh" | "en"
    #[serde(default = "default_language")]
    pub language: String,

    #[serde(default)]
    pub wake_word: WakeWordConfig,
}

fn default_name() -> String {
    "Chatbox".into()
}
fn default_language() -> String {
    "zh".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordConfig {
    pub enabled: bool,
    /// The trigger phrase, e.g. "嘿小派" or "Hey Chatbox"
    #[serde(default = "default_wake_word")]
    pub word: String,
}

fn default_wake_word() -> String {
    "嘿小派".into()
}

// ─── LLM profiles ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProfile {
    /// Human-readable label, e.g. "DeepSeek", "My Claude"
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

/// Compatibility shim so OpenAiClient keeps its existing constructor signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

// ─── Speech ──────────────────────────────────────────────────────────────────

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
    pub tts_speed: f64,
    pub tts_url: String,
    pub asr_url: String,
}

// ─── Audio ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub silence_seconds: f32,
    pub min_speech_seconds: f32,
}

// ─── Legacy structs (migration only) ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LegacyLlmConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LegacyWakeWordConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub word_en: String,
    #[serde(default)]
    pub word_zh: String,
}

// ─── Defaults ────────────────────────────────────────────────────────────────

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            persona: PersonaConfig::default(),
            active_llm: String::new(),
            llm_profiles: vec![],
            speech: SpeechConfig::default(),
            audio: AudioConfig::default(),
            locale: None,
            llm: None,
            wake_word: None,
        }
    }
}

impl Default for PersonaConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            language: default_language(),
            wake_word: WakeWordConfig::default(),
        }
    }
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            word: default_wake_word(),
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

// ─── AppConfig methods ────────────────────────────────────────────────────────

impl AppConfig {
    /// Load config file, apply env overrides, then migrate legacy fields.
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

        cfg.migrate_legacy();
        cfg.apply_env_overrides();
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Migrate old flat `[llm]` / `locale` / `[wake_word]` fields into new structure.
    fn migrate_legacy(&mut self) {
        // locale → persona.language
        if let Some(locale) = self.locale.take() {
            if !locale.is_empty() {
                self.persona.language = locale;
            }
        }

        // [llm] → llm_profiles[0]
        if let Some(llm) = self.llm.take() {
            if self.llm_profiles.is_empty() && is_real_value(&llm.api_key) {
                let name = guess_provider_name(&llm.base_url);
                self.llm_profiles.push(LlmProfile {
                    name: name.clone(),
                    base_url: llm.base_url,
                    model: llm.model,
                    api_key: llm.api_key,
                });
                if self.active_llm.is_empty() {
                    self.active_llm = name;
                }
            }
        }

        // [wake_word] → persona.wake_word
        if let Some(ww) = self.wake_word.take() {
            self.persona.wake_word.enabled = ww.enabled;
            if !ww.word_zh.is_empty() {
                self.persona.wake_word.word = ww.word_zh;
            } else if !ww.word_en.is_empty() {
                self.persona.wake_word.word = ww.word_en;
            }
        }

        // Ensure active_llm points to an existing profile
        if !self.active_llm.is_empty()
            && !self.llm_profiles.iter().any(|p| p.name == self.active_llm)
        {
            self.active_llm = self
                .llm_profiles
                .first()
                .map(|p| p.name.clone())
                .unwrap_or_default();
        }
    }

    fn apply_env_overrides(&mut self) {
        // Env vars patch the active profile (or create one on the fly)
        let api_key = std::env::var("AI_API_KEY").ok();
        let base_url = std::env::var("AI_BASE_URL").ok();
        let model = std::env::var("AI_MODEL").ok();

        if api_key.is_some() || base_url.is_some() || model.is_some() {
            if let Some(profile) = self.active_llm_profile_mut() {
                if let Some(v) = api_key {
                    profile.api_key = v;
                }
                if let Some(v) = base_url {
                    profile.base_url = v;
                }
                if let Some(v) = model {
                    profile.model = v;
                }
            }
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

    pub fn active_llm_profile(&self) -> Option<&LlmProfile> {
        self.llm_profiles
            .iter()
            .find(|p| p.name == self.active_llm)
            .or_else(|| self.llm_profiles.first())
    }

    fn active_llm_profile_mut(&mut self) -> Option<&mut LlmProfile> {
        let name = self.active_llm.clone();
        if let Some(idx) = self.llm_profiles.iter().position(|p| p.name == name) {
            return Some(&mut self.llm_profiles[idx]);
        }
        self.llm_profiles.first_mut()
    }

    /// Convert active profile to the LlmConfig shape OpenAiClient expects.
    pub fn active_llm_config(&self) -> Option<LlmConfig> {
        self.active_llm_profile().map(|p| LlmConfig {
            api_key: p.api_key.clone(),
            base_url: p.base_url.clone(),
            model: p.model.clone(),
        })
    }

    pub fn is_complete(&self) -> bool {
        let Some(profile) = self.active_llm_profile() else {
            return false;
        };
        // Ollama-style local providers don't need a real key
        let key_ok = is_real_value(&profile.api_key) || profile.api_key == "ollama";
        key_ok
            && is_real_value(&self.speech.doubao.app_id)
            && is_real_value(&self.speech.doubao.access_token)
    }

    pub fn validate(&self) -> Result<()> {
        let profile = self
            .active_llm_profile()
            .ok_or_else(|| anyhow::anyhow!("No LLM profile configured. Run `cb config`"))?;

        if profile.api_key.is_empty() {
            anyhow::bail!(
                "LLM API key not set for profile '{}'. Run `cb config`",
                profile.name
            );
        }
        if self.speech.doubao.app_id.is_empty() {
            anyhow::bail!("Doubao App ID not set. Run `cb config`");
        }
        if self.speech.doubao.access_token.is_empty() {
            anyhow::bail!("Doubao Access Token not set. Run `cb config`");
        }
        Ok(())
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

pub fn is_real_value(val: &str) -> bool {
    !val.is_empty() && !val.contains("your-") && !val.contains("xxx") && val != "placeholder"
}

fn guess_provider_name(base_url: &str) -> String {
    if base_url.contains("deepseek") {
        "DeepSeek".into()
    } else if base_url.contains("anthropic") {
        "Claude".into()
    } else if base_url.contains("openai") {
        "OpenAI".into()
    } else if base_url.contains("groq") {
        "Groq".into()
    } else if base_url.contains("localhost") || base_url.contains("127.0.0.1") {
        "Ollama".into()
    } else {
        "Custom".into()
    }
}
