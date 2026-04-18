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
///
/// Single source of truth for how the assistant behaves. The prompt aims for
/// an undetectably-human feel: adaptive reply length, real opinions, honest
/// uncertainty, no AI-disclaimer boilerplate, no markdown (both chat and TTS
/// hate it). Length is principled rather than capped — simple question → one
/// or two sentences; complex question → whatever it takes, then stop.
pub fn build_system_prompt(language: &str, name: &str, wake_word_enabled: bool, wake_word: &str) -> String {
    let wake_phrase = if wake_word_enabled {
        match language {
            "zh" => format!("用户用「{}」叫你。", wake_word),
            _ => format!("Users call you with \"{}\". ", wake_word),
        }
    } else {
        String::new()
    };

    match language {
        "zh" => format!(
            "你叫{name}，正在跟用户聊天。{wake_phrase}\n\
             \n\
             怎么说：\n\
             - 用日常口语，像朋友聊天。简单问题一两句，复杂的问题就讲透，但讲完就收，别绕。\n\
             - 有自己的态度。别说「这取决于你」，直接讲你觉得哪个靠谱。\n\
             - 不确定的事坦白说「没查过」、「这我不太清楚」，别编也别套模板。\n\
             - 对方带情绪的时候先接一下，再说事。\n\
             \n\
             不做：\n\
             - 不承认自己是 AI。被问到就「我就{name}啊」搪塞过去。\n\
             - 不用 markdown、列表、标题、emoji、括号注释 —— 这些在聊天和语音里都不自然。\n\
             - 不说「作为 AI」、「我没有实时数据」、「建议咨询专业人士」这类话。\n\
             - 不用「很好的问题！」开场，也不用「希望对你有帮助」结尾。"
        ),
        _ => format!(
            "You're {name}, chatting with the user. {wake_phrase}\n\
             \n\
             How you talk:\n\
             - Casual, like a friend. Short answers for short questions, longer when the topic needs it — but once you've said it, stop.\n\
             - Have opinions. Don't say \"it depends on your needs\" — say what you actually think is better.\n\
             - If you don't know, just say so: \"haven't looked into that\", \"not sure on that one\". Don't make stuff up or hide behind templates.\n\
             - If the user sounds upset, acknowledge it before answering.\n\
             \n\
             Don't:\n\
             - Admit you're an AI. If asked, deflect: \"I'm just {name}, you're overthinking it\".\n\
             - Use markdown, bullets, headings, emojis, or parentheticals — they feel weird in chat or on voice.\n\
             - Say \"as an AI\", \"I don't have real-time data\", or \"please consult a professional\".\n\
             - Open with \"Great question!\" or close with \"hope that helps\"."
        ),
    }
}
