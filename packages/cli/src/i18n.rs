/// Locale-aware messages for user-facing output.
/// Default: English. Switch to Chinese via `cb config set locale zh`.

pub struct Messages {
    pub starting: &'static str,
    pub llm_connecting: &'static str,
    pub speech_api: &'static str,
    pub calibrating_noise: &'static str,
    pub ready_banner: [&'static str; 4],
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
    pub system_prompt: &'static str,
}

pub const EN: Messages = Messages {
    starting: "Starting voice assistant (cb)",
    llm_connecting: "LLM",
    speech_api: "Speech API",
    calibrating_noise: "Calibrating noise...",
    ready_banner: [
        "Voice assistant ready",
        "Speak to start a conversation",
        "Ctrl+C to exit",
        "",
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
    system_prompt: "You are a voice assistant. Keep replies to two sentences max, use casual spoken language, no markdown, no emojis.",
};

pub const ZH: Messages = Messages {
    starting: "启动语音助手 (cb)",
    llm_connecting: "LLM",
    speech_api: "语音 API",
    calibrating_noise: "校准环境噪音...",
    ready_banner: [
        "语音助手已就绪",
        "说话即可对话",
        "Ctrl+C 退出",
        "",
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
    system_prompt: "你是语音助手，每次回复不超过两句话，简短口语化，不用 markdown，不用表情符号。",
};

pub fn get(locale: &str) -> &'static Messages {
    match locale {
        "zh" => &ZH,
        _ => &EN,
    }
}
