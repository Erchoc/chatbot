pub struct LlmPreset {
    pub name: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
    pub needs_key: bool,
}

pub const LLM_PRESETS: &[LlmPreset] = &[
    LlmPreset {
        name: "DeepSeek",
        base_url: "https://api.deepseek.com",
        default_model: "deepseek-chat",
        needs_key: true,
    },
    LlmPreset {
        name: "Claude",
        base_url: "https://api.anthropic.com/v1",
        default_model: "claude-sonnet-4-6",
        needs_key: true,
    },
    LlmPreset {
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o",
        needs_key: true,
    },
    LlmPreset {
        name: "Ollama (local)",
        base_url: "http://localhost:11434/v1",
        default_model: "llama3.2",
        needs_key: false,
    },
];

pub struct VoicePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub style: &'static str,
    /// Only show when language is "zh" (empty = always show)
    pub zh_only: bool,
}

/// Voice presets for Doubao TTS.
/// Docs: https://www.volcengine.com/docs/6561/97465
pub const DOUBAO_VOICES: &[VoicePreset] = &[
    // ── Common voices ──────────────────────────────────────────────────────
    VoicePreset {
        id: "BV700_V2_streaming",
        name: "灿灿",
        style: "知性·女声",
        zh_only: false,
    },
    VoicePreset {
        id: "BV409_streaming",
        name: "TVB女声",
        style: "港剧·女声",
        zh_only: false,
    },
    VoicePreset {
        id: "BV405_streaming",
        name: "甜美桃子",
        style: "甜美·女声",
        zh_only: false,
    },
    VoicePreset {
        id: "BV406_V2_streaming",
        name: "湾湾小何",
        style: "温柔·台湾",
        zh_only: false,
    },
    VoicePreset {
        id: "BV407_V2_streaming",
        name: "呆萌川妹",
        style: "可爱·四川",
        zh_only: false,
    },
    VoicePreset {
        id: "BV424_streaming",
        name: "粤语小唐",
        style: "粤语·女声",
        zh_only: false,
    },
    VoicePreset {
        id: "BV428_streaming",
        name: "小猪佩奇",
        style: "卡通·趣味",
        zh_only: false,
    },
    VoicePreset {
        id: "BV426_streaming",
        name: "少儿故事",
        style: "温暖·少儿",
        zh_only: false,
    },
    VoicePreset {
        id: "BV520_streaming",
        name: "娇喘女声",
        style: "娇甜·女声",
        zh_only: false,
    },
    // ── Chinese-only character voices ──────────────────────────────────────
    VoicePreset {
        id: "BV421_streaming",
        name: "庄周",
        style: "古风·男声",
        zh_only: true,
    },
    VoicePreset {
        id: "BV158_streaming",
        name: "唐僧",
        style: "西游·男声",
        zh_only: true,
    },
    VoicePreset {
        id: "BV159_streaming",
        name: "猪八戒",
        style: "西游·男声",
        zh_only: true,
    },
];
