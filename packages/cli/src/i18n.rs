/// Locale-aware UI messages.
pub struct Messages {
    pub llm_connecting: &'static str,
    pub speech_api: &'static str,
    pub calibrating_noise: &'static str,
    pub ready_banner: [&'static str; 3],
    pub listening: &'static str,
    pub speech_detected: &'static str,
    pub too_short: &'static str,
    pub you: &'static str,
    pub assistant: &'static str,
    pub mic_init_failed: &'static str,
    pub mic_polling: &'static str,
    pub mic_retry: &'static str,
    pub mic_disconnected: &'static str,
    pub recording_failed: &'static str,
    pub stt_failed: &'static str,
    pub chat_failed: &'static str,
    pub goodbye: &'static str,
}

pub const EN: Messages = Messages {
    llm_connecting: "LLM",
    speech_api: "Speech API",
    calibrating_noise: "Calibrating noise...",
    ready_banner: [
        "Voice assistant ready",
        "Speak to start a conversation",
        "Ctrl+C to exit",
    ],
    listening: "Listening...",
    speech_detected: "Speech detected...",
    too_short: "Too short, ignored",
    you: "You",
    assistant: "Assistant",
    mic_init_failed: "Mic init failed",
    mic_polling: "Mic still unavailable, polling every 60s, will auto-resume on connect...",
    mic_retry: "Check mic connection and permissions, retrying in",
    mic_disconnected: "Mic disconnected, please reconnect and restart",
    recording_failed: "Recording failed",
    stt_failed: "Speech recognition failed",
    chat_failed: "Chat failed",
    goodbye: "See you next time!",
};

pub const ZH: Messages = Messages {
    llm_connecting: "LLM",
    speech_api: "语音 API",
    calibrating_noise: "校准环境噪音...",
    ready_banner: [
        "语音助手已就绪",
        "说话即可对话",
        "Ctrl+C 退出",
    ],
    listening: "说话吧...",
    speech_detected: "检测到语音...",
    too_short: "太短，忽略",
    you: "你",
    assistant: "助手",
    mic_init_failed: "麦克风初始化失败",
    mic_polling: "麦克风仍不可用，每 60 秒自动检测，连接后自动恢复...",
    mic_retry: "请检查麦克风连接和权限，重试倒计时",
    mic_disconnected: "麦克风断开，请重新连接后重启程序",
    recording_failed: "录音失败",
    stt_failed: "语音识别失败",
    chat_failed: "对话失败",
    goodbye: "下次再聊！",
};

pub fn get(language: &str) -> &'static Messages {
    match language {
        "zh" => &ZH,
        _ => &EN,
    }
}

/// Build the LLM system prompt from persona settings.
/// This is the single source of truth for what personality the assistant has.
pub fn build_system_prompt(language: &str, name: &str, wake_word_enabled: bool, wake_word: &str) -> String {
    let wake_note = if wake_word_enabled {
        match language {
            "zh" => format!("用户通过唤醒词「{}」呼叫你。", wake_word),
            _ => format!("Users call you with the wake word \"{}\". ", wake_word),
        }
    } else {
        String::new()
    };

    match language {
        "zh" => format!(
            "你是语音助手「{name}」。{wake_note}\
             每次回复不超过两句话，使用简短自然的口语中文。\
             不要使用 markdown、列表或表情符号。\
             如果用户问时间、天气等实时信息，直接说你目前没有这些数据。"
        ),
        _ => format!(
            "You are a voice assistant named {name}. {wake_note}\
             Keep every reply under two sentences, using casual spoken language. \
             No markdown, no lists, no emojis. \
             If asked for real-time data like weather or time, say you don't have access."
        ),
    }
}
