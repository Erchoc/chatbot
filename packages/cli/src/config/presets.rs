pub struct LlmPreset {
    pub name: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
    pub needs_key: bool,
}

pub const LLM_PRESETS: &[LlmPreset] = &[
    LlmPreset {
        name: "DeepSeek",
        base_url: "https://api.deepseek.com/v1",
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
    /// Only show when language is "zh" (false = always show)
    pub zh_only: bool,
}

/// 默认音色：小何 2.0 —— 1.0「湾湾小何」（台湾口音）的 2.0 版本，官方迁移表如此对应。
pub const DEFAULT_VOICE: &str = "zh_female_xiaohe_uranus_bigtts";

/// 豆包语音合成模型 2.0 音色（`seed-tts-2.0`）。
/// 全量列表：https://www.volcengine.com/docs/6561/1257544
pub const DOUBAO_VOICES: &[VoicePreset] = &[
    // ── 通用 ──────────────────────────────────────────────────────────────
    VoicePreset { id: "zh_female_xiaohe_uranus_bigtts", name: "小何", style: "台湾腔·女声", zh_only: false },
    VoicePreset { id: "zh_female_vv_uranus_bigtts", name: "Vivi", style: "通用·女声", zh_only: false },
    VoicePreset { id: "zh_female_cancan_uranus_bigtts", name: "知性灿灿", style: "知性·女声", zh_only: false },
    VoicePreset { id: "zh_female_tianmeitaozi_uranus_bigtts", name: "甜美桃子", style: "甜美·女声", zh_only: false },
    VoicePreset { id: "zh_female_shuangkuaisisi_uranus_bigtts", name: "爽快思思", style: "爽快·女声", zh_only: true },
    VoicePreset { id: "zh_female_qingxinnvsheng_uranus_bigtts", name: "清新女声", style: "清新·女声", zh_only: true },
    VoicePreset { id: "zh_male_m191_uranus_bigtts", name: "云舟", style: "沉稳·男声", zh_only: false },
    VoicePreset { id: "zh_male_taocheng_uranus_bigtts", name: "小天", style: "阳光·男声", zh_only: false },
    VoicePreset { id: "zh_male_ruyaqingnian_uranus_bigtts", name: "儒雅青年", style: "儒雅·男声", zh_only: true },
    VoicePreset { id: "zh_male_shaonianzixin_uranus_bigtts", name: "少年梓辛", style: "少年·男声", zh_only: true },
    // ── 角色 / 趣味 ──────────────────────────────────────────────────────
    VoicePreset { id: "zh_female_tvbnv_uranus_bigtts", name: "TVB女声", style: "港剧·女声", zh_only: true },
    VoicePreset { id: "zh_female_peiqi_uranus_bigtts", name: "佩奇猪", style: "卡通·趣味", zh_only: true },
    VoicePreset { id: "zh_male_sunwukong_uranus_bigtts", name: "猴哥", style: "西游·男声", zh_only: true },
    VoicePreset { id: "zh_female_xiaoxue_uranus_bigtts", name: "儿童绘本", style: "温暖·少儿", zh_only: true },
    // ── 英语 ──────────────────────────────────────────────────────────────
    VoicePreset { id: "en_male_tim_uranus_bigtts", name: "Tim", style: "美式·男声", zh_only: false },
    VoicePreset { id: "en_female_dacey_uranus_bigtts", name: "Dacey", style: "美式·女声", zh_only: false },
];

/// 1.0 音色 → 最接近的 2.0 音色。
/// `BV700_V2_streaming` 是 1.0 时代的出厂默认，绝大多数用户并没有主动选它，
/// 所以它跟着新默认走（台湾腔小何），而不是机械地换成灿灿 2.0。
const LEGACY_VOICE_MAP: &[(&str, &str)] = &[
    ("BV700_V2_streaming", DEFAULT_VOICE),
    ("BV700_streaming", "zh_female_cancan_uranus_bigtts"),
    ("zh_female_wanwanxiaohe_moon_bigtts", "zh_female_xiaohe_uranus_bigtts"),
    ("BV405_streaming", "zh_female_tianmeitaozi_uranus_bigtts"),
    ("BV406_V2_streaming", "zh_female_xiaohe_uranus_bigtts"),
    ("BV409_streaming", "zh_female_tvbnv_uranus_bigtts"),
    ("BV428_streaming", "zh_female_peiqi_uranus_bigtts"),
    ("BV426_streaming", "zh_female_xiaoxue_uranus_bigtts"),
    ("zh_female_cancan_mars_bigtts", "zh_female_cancan_uranus_bigtts"),
    ("zh_female_tianmeitaozi_mars_bigtts", "zh_female_tianmeitaozi_uranus_bigtts"),
    ("zh_female_shuangkuaisisi_moon_bigtts", "zh_female_shuangkuaisisi_uranus_bigtts"),
    ("zh_male_shaonianzixin_moon_bigtts", "zh_male_shaonianzixin_uranus_bigtts"),
];

/// 判断音色 ID 是否属于 1.0 时代（BV 系列 / `_mars_bigtts` / `_moon_bigtts` / `_streaming`），
/// 是则返回应迁移到的 2.0 音色；已是 2.0 或复刻音色（`S_xxx`）返回 None 不动。
pub fn migrate_voice(voice_type: &str) -> Option<&'static str> {
    if voice_type.is_empty() || voice_type.ends_with("_uranus_bigtts") {
        return None;
    }
    let looks_v1 = voice_type.starts_with("BV")
        || voice_type.ends_with("_mars_bigtts")
        || voice_type.ends_with("_moon_bigtts")
        || voice_type.ends_with("_streaming");
    if !looks_v1 {
        return None;
    }
    Some(
        LEGACY_VOICE_MAP
            .iter()
            .find(|(old, _)| *old == voice_type)
            .map(|(_, new)| *new)
            .unwrap_or(DEFAULT_VOICE),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_known_and_unknown_v1_voices() {
        assert_eq!(migrate_voice("BV700_V2_streaming"), Some(DEFAULT_VOICE));
        assert_eq!(migrate_voice("BV406_V2_streaming"), Some("zh_female_xiaohe_uranus_bigtts"));
        assert_eq!(migrate_voice("BV999_streaming"), Some(DEFAULT_VOICE));
        assert_eq!(migrate_voice("zh_female_xxx_mars_bigtts"), Some(DEFAULT_VOICE));
    }

    #[test]
    fn leaves_v2_and_cloned_voices_alone() {
        assert_eq!(migrate_voice("zh_female_vv_uranus_bigtts"), None);
        assert_eq!(migrate_voice("S_abc123"), None);
        assert_eq!(migrate_voice(""), None);
    }

    #[test]
    fn default_voice_is_in_presets() {
        assert!(DOUBAO_VOICES.iter().any(|v| v.id == DEFAULT_VOICE));
    }
}
