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

pub fn cache_path(name: &str) -> PathBuf {
    config_dir().join(name)
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
    "小派".into()
}
fn default_language() -> String {
    "zh".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeWordConfig {
    pub enabled: bool,
    /// The trigger phrase, e.g. "小派小派" or "Hey Chatbox"
    #[serde(default = "default_wake_word")]
    pub word: String,
}

fn default_wake_word() -> String {
    "小派小派".into()
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
    /// 新版控制台的 API Key（优先）。填了就不再需要 app_id / access_token。
    #[serde(default)]
    pub api_key: String,
    /// 旧版控制台：App ID + Access Token
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub access_token: String,
    /// 语音识别 2.0：`volc.seedasr.sauc.duration`（小时版）/ `volc.seedasr.sauc.concurrent`（并发版）
    #[serde(default = "default_asr_resource_id")]
    pub asr_resource_id: String,
    /// 语音合成 2.0：`seed-tts-2.0`（`seed-icl-2.0` 为复刻音色）
    #[serde(default = "default_tts_resource_id")]
    pub tts_resource_id: String,
    /// 2.0 音色 ID（`*_uranus_bigtts`）
    #[serde(default = "default_voice_type")]
    pub voice_type: String,
    /// 语速倍率 0.5 ~ 2.0（内部映射为接口的 speech_rate）
    #[serde(default = "default_tts_speed")]
    pub tts_speed: f64,
    /// 语音指令（2.0 音色支持），控制整体语气；留空则不下发。
    #[serde(default = "default_voice_instruction")]
    pub voice_instruction: String,
    #[serde(default = "default_tts_url")]
    pub tts_url: String,
    #[serde(default = "default_asr_url")]
    pub asr_url: String,
}

fn default_asr_resource_id() -> String {
    "volc.seedasr.sauc.duration".into()
}
fn default_tts_resource_id() -> String {
    "seed-tts-2.0".into()
}
fn default_voice_type() -> String {
    super::presets::DEFAULT_VOICE.into()
}
fn default_tts_speed() -> f64 {
    1.3
}
fn default_voice_instruction() -> String {
    "用轻松自然、像朋友聊天一样的语气说，不要念稿".into()
}
fn default_tts_url() -> String {
    "https://openspeech.bytedance.com/api/v3/tts/unidirectional".into()
}
fn default_asr_url() -> String {
    "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async".into()
}

impl DoubaoConfig {
    /// 新版 API Key 或旧版 App ID + Token，二选一即可。
    pub fn has_credentials(&self) -> bool {
        is_real_value(&self.api_key)
            || (is_real_value(&self.app_id) && is_real_value(&self.access_token))
    }
}

// ─── Audio ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default = "default_silence_seconds")]
    pub silence_seconds: f32,
    #[serde(default = "default_min_speech_seconds")]
    pub min_speech_seconds: f32,
    /// 助手说话时允许用户开口打断（需要麦克风能盖过扬声器回声；戴耳机效果最好）。
    #[serde(default = "default_barge_in")]
    pub barge_in: bool,
}

fn default_silence_seconds() -> f32 {
    1.0
}
fn default_min_speech_seconds() -> f32 {
    1.0
}
fn default_barge_in() -> bool {
    true
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
            enabled: true,
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
            api_key: String::new(),
            app_id: String::new(),
            access_token: String::new(),
            asr_resource_id: default_asr_resource_id(),
            tts_resource_id: default_tts_resource_id(),
            voice_type: default_voice_type(),
            tts_speed: default_tts_speed(),
            voice_instruction: default_voice_instruction(),
            tts_url: default_tts_url(),
            asr_url: default_asr_url(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            silence_seconds: default_silence_seconds(),
            min_speech_seconds: default_min_speech_seconds(),
            barge_in: default_barge_in(),
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

        // DeepSeek v1 path: early configs stored the bare host, which 404s
        // because this client appends /chat/completions directly. Bump to /v1
        // transparently so existing users don't have to re-run the wizard.
        for profile in self.llm_profiles.iter_mut() {
            if profile.base_url.trim_end_matches('/') == "https://api.deepseek.com" {
                profile.base_url = "https://api.deepseek.com/v1".into();
            }
        }

        // 语音 1.0 → 2.0：资源 ID、接口地址、音色一起升级，用户不用重跑向导。
        self.speech.doubao.migrate_to_v2();

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

        if let Ok(v) = std::env::var("DOUBAO_API_KEY") {
            self.speech.doubao.api_key = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_APP_ID") {
            self.speech.doubao.app_id = v;
        }
        if let Ok(v) = std::env::var("DOUBAO_ACCESS_TOKEN") {
            self.speech.doubao.access_token = v;
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
        if let Ok(v) = std::env::var("DOUBAO_VOICE_INSTRUCTION") {
            self.speech.doubao.voice_instruction = v;
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
        key_ok && self.speech.doubao.has_credentials()
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
        if !self.speech.doubao.has_credentials() {
            anyhow::bail!("Doubao credentials not set (api_key, or app_id + access_token). Run `cb config`");
        }
        Ok(())
    }
}

impl DoubaoConfig {
    /// 把 1.0 时代的配置值换成 2.0 对应值。幂等：已是 2.0 的值原样保留。
    fn migrate_to_v2(&mut self) {
        if let Some(rest) = self.asr_resource_id.strip_prefix("volc.bigasr.") {
            self.asr_resource_id = format!("volc.seedasr.{rest}");
        }
        if self.tts_resource_id == "volc.service_type.10029"
            || self.tts_resource_id.starts_with("seed-tts-1.0")
        {
            self.tts_resource_id = default_tts_resource_id();
        }
        if self.tts_url.contains("/api/v1/tts") {
            self.tts_url = default_tts_url();
        }
        if self.asr_url.ends_with("/sauc/bigmodel") {
            self.asr_url = default_asr_url();
        }
        if let Some(v2) = super::presets::migrate_voice(&self.voice_type) {
            self.voice_type = v2.to_string();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_v1_values_to_v2() {
        let mut d = DoubaoConfig {
            asr_resource_id: "volc.bigasr.sauc.duration".into(),
            tts_resource_id: "volc.service_type.10029".into(),
            tts_url: "https://openspeech.bytedance.com/api/v1/tts".into(),
            asr_url: "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel".into(),
            voice_type: "BV700_V2_streaming".into(),
            ..Default::default()
        };
        d.migrate_to_v2();
        assert_eq!(d.asr_resource_id, "volc.seedasr.sauc.duration");
        assert_eq!(d.tts_resource_id, "seed-tts-2.0");
        assert!(d.tts_url.ends_with("/api/v3/tts/unidirectional"));
        assert!(d.asr_url.ends_with("/sauc/bigmodel_async"));
        assert_eq!(d.voice_type, super::super::presets::DEFAULT_VOICE);
    }

    #[test]
    fn migrate_is_idempotent_and_keeps_cloned_voices() {
        let mut d = DoubaoConfig { voice_type: "S_abc123".into(), ..Default::default() };
        let before = d.clone();
        d.migrate_to_v2();
        assert_eq!(d.voice_type, before.voice_type);
        assert_eq!(d.asr_resource_id, before.asr_resource_id);
    }

    #[test]
    fn old_toml_without_new_fields_still_parses() {
        let toml_src = r#"
            [speech]
            provider = "doubao"
            [speech.doubao]
            app_id = "123"
            access_token = "tok"
            tts_cluster = "volcano_tts"
            asr_resource_id = "volc.bigasr.sauc.duration"
            tts_resource_id = "volc.service_type.10029"
            voice_type = "BV405_streaming"
            tts_speed = 1.6
            tts_url = "https://openspeech.bytedance.com/api/v1/tts"
            asr_url = "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async"
        "#;
        let mut cfg: AppConfig = toml::from_str(toml_src).expect("parse");
        cfg.migrate_legacy();
        assert!(cfg.speech.doubao.has_credentials());
        assert_eq!(cfg.speech.doubao.voice_type, "zh_female_tianmeitaozi_uranus_bigtts");
        assert_eq!(cfg.speech.doubao.tts_speed, 1.6);
    }
}
