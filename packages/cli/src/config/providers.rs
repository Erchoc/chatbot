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
}

pub const DOUBAO_VOICES: &[VoicePreset] = &[
    VoicePreset {
        id: "BV700_V2_streaming",
        name: "灿灿",
        style: "活泼·女声",
    },
    VoicePreset {
        id: "BV701_V2_streaming",
        name: "擎苍",
        style: "浑厚·男声",
    },
    VoicePreset {
        id: "BV406_V2_streaming",
        name: "湾湾小何",
        style: "温柔·台湾",
    },
    VoicePreset {
        id: "BV705_V2_streaming",
        name: "小清",
        style: "清澈·女声",
    },
    VoicePreset {
        id: "BV034_V2_streaming",
        name: "知性女声",
        style: "知性·播报",
    },
    VoicePreset {
        id: "BV102_V2_streaming",
        name: "萌萌",
        style: "可爱·少女",
    },
];
