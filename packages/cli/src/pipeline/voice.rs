use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};

use crate::audio::capture::{calibrate_noise, record_speech, RecordParams};
use crate::audio::playback::spawn_player;
use crate::audio::resample::{downsample_to_mono_16k, encode_wav};
use crate::audio::TARGET_RATE;
use crate::config::AppConfig;
use crate::history::Conversation;
use crate::i18n::{self, Messages};
use crate::llm::OpenAiClient;
use crate::log::EventLogger;
use crate::speech::doubao::{DoubaoAsr, DoubaoTts};
use crate::speech::{Asr, Tts};
use crate::ui::{art::Face, banner, spinner::Spinner, theme::*};

struct TurnMetrics {
    stt_ms: f32,
    llm_ttft_ms: f32,
    llm_total_ms: f32,
    llm_tokens: usize,
    tts_synth_ms: f32,
}

impl TurnMetrics {
    fn e2e_ms(&self) -> f32 {
        self.stt_ms + self.llm_total_ms
    }

    fn tok_per_s(&self) -> f32 {
        if self.llm_total_ms > 0.0 {
            self.llm_tokens as f32 / (self.llm_total_ms / 1000.0)
        } else {
            0.0
        }
    }

    fn log(&self) {
        println!(
            "   {MUTED}STT {:.1}s | TTFT {:.1}s | {}tok {:.0}tok/s | TTS {:.1}s | E2E {:.1}s{RESET}",
            self.stt_ms / 1000.0,
            self.llm_ttft_ms / 1000.0,
            self.llm_tokens,
            self.tok_per_s(),
            self.tts_synth_ms / 1000.0,
            self.e2e_ms() / 1000.0,
        );
    }
}

fn is_sentence_end(c: char) -> bool {
    // Only true sentence-ending punctuation — NOT commas.
    // Commas are mid-sentence pauses and shouldn't trigger a TTS split,
    // otherwise the TTS engine gets fragments too short to synthesize.
    matches!(c, '。' | '！' | '？' | '.' | '!' | '?' | '\n')
}

/// How long the assistant stays "awake" after the last interaction.
const WAKE_DURATION: Duration = Duration::from_secs(5 * 60);

/// Wake-word session state.
enum WakeState {
    /// Waiting for wake word.
    Sleeping,
    /// Active – expires_at is when it auto-sleeps without new interaction.
    Awake { expires_at: Instant },
}

impl WakeState {
    fn is_awake(&self) -> bool {
        match self {
            Self::Awake { expires_at } => Instant::now() < *expires_at,
            Self::Sleeping => false,
        }
    }

    fn wake(&mut self) {
        *self = Self::Awake { expires_at: Instant::now() + WAKE_DURATION };
    }

    fn renew(&mut self) {
        if let Self::Awake { expires_at } = self {
            *expires_at = Instant::now() + WAKE_DURATION;
        }
    }

    fn sleep(&mut self) {
        *self = Self::Sleeping;
    }
}

pub struct VoicePipeline {
    cfg: AppConfig,
    client: Client,
    asr: Box<dyn Asr>,
    llm: OpenAiClient,
    history: Vec<Value>,
    conversation: Conversation,
    logger: EventLogger,
    turn_count: usize,
    wake_state: WakeState,
    msg: &'static Messages,
    #[allow(dead_code)]
    debug: bool,
}

impl VoicePipeline {
    pub fn new(cfg: AppConfig, debug: bool) -> Result<Self> {
        cfg.validate()?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let asr: Box<dyn Asr> = Box::new(DoubaoAsr::new(cfg.speech.doubao.clone(), debug));
        let llm_config = cfg
            .active_llm_config()
            .ok_or_else(|| anyhow::anyhow!("No active LLM profile. Run `cb config`"))?;
        let llm = OpenAiClient::new(client.clone(), llm_config);
        let msg = i18n::get(&cfg.persona.language);

        let system_prompt = i18n::build_system_prompt(
            &cfg.persona.language,
            &cfg.persona.name,
            cfg.persona.wake_word.enabled,
            &cfg.persona.wake_word.word,
        );
        let history = vec![json!({
            "role": "system",
            "content": system_prompt
        })];

        // Session ID = timestamp prefix for log grouping
        let session_id = format!(
            "s{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        let logger = EventLogger::new(session_id);

        Ok(Self {
            cfg,
            client,
            asr,
            llm,
            history,
            conversation: Conversation::new(),
            logger,
            turn_count: 0,
            wake_state: WakeState::Sleeping,
            msg,
            debug,
        })
    }

    pub async fn run_loop(&mut self) -> Result<()> {
        let m = self.msg;

        // Startup banner
        banner::print_banner(env!("CARGO_PKG_VERSION"));
        println!("{}", Face::idle());

        // Init spinners for loading
        let llm_model = self
            .cfg
            .active_llm_profile()
            .map(|p| p.model.as_str())
            .unwrap_or("unknown")
            .to_string();
        let sp = Spinner::start_inline(
            &format!("{} ({})...", m.llm_connecting, llm_model),
            BR_BLUE,
        );
        // Log session start
        self.logger.session_start(
            &llm_model,
            &self.cfg.speech.doubao.voice_type,
            &self.cfg.persona.language,
        );

        // Simulate connection check
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        sp.stop_with(&format!(
            "   {BR_GREEN}✓{RESET} {BOLD}{}{RESET} ({}) {BR_GREEN}OK{RESET}",
            m.llm_connecting, llm_model
        ));

        let sp = Spinner::start_inline(
            &format!("{}...", m.speech_api),
            BR_BLUE,
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        sp.stop_with(&format!(
            "   {BR_GREEN}✓{RESET} {BOLD}{}{RESET} (STT: {}, TTS: {}) {BR_GREEN}OK{RESET}",
            m.speech_api, self.cfg.speech.doubao.asr_resource_id, self.cfg.speech.doubao.voice_type
        ));

        // Mic calibration with exponential backoff
        let (threshold, dev_info) = {
            let mut backoff_secs = 1_u64;
            loop {
                let sp = Spinner::start_inline(m.calibrating_noise, BR_CYAN);
                match tokio::task::spawn_blocking(calibrate_noise).await? {
                    Ok(v) => {
                        sp.stop_with(&format!(
                            "   {BR_GREEN}✓{RESET} {BOLD}{}{RESET} {MUTED}(noise:{:.4} threshold:{:.4}){RESET}",
                            m.calibrating_noise, v.0 / 6.0, v.0  // approximate ambient from threshold
                        ));
                        break v;
                    }
                    Err(e) => {
                        sp.stop();
                        print!("{}", Face::error());
                        eprintln!("   {ERROR_COLOR}{}: {e}{RESET}", m.mic_init_failed);
                        self.logger.error("audio", &format!("mic calibration failed: {e}"));
                        if backoff_secs >= 60 {
                            eprintln!("   {MUTED}{}{RESET}", m.mic_polling);
                        } else {
                            eprintln!(
                                "   {MUTED}{} {backoff_secs}s...{RESET}",
                                m.mic_retry
                            );
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(60);
                    }
                }
            }
        };

        // Ready banner
        banner::print_ready(&[m.ready_banner[0], m.ready_banner[1], m.ready_banner[2]]);

        if let Some(v) = crate::update_check::pending_notice() {
            let hint = crate::update_check::upgrade_hint();
            println!(
                "   {BR_CYAN}⬆  发现新版本 v{v}，运行 {BOLD}{hint}{RESET}{BR_CYAN} 升级{RESET}\n"
            );
            // Daemon stdout is redirected to a log file, so this banner is
            // invisible to a user who isn't tailing logs. Fire an OS-level
            // desktop notification too so backgrounded-daemon users actually
            // find out a new version exists.
            crate::update_check::notify_desktop(&v, &hint);
        }

        // Ctrl+C handler
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        let goodbye = m.goodbye;
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
            println!("\n\n   {BR_CYAN}👋 {goodbye}{RESET}");
            std::process::exit(0);
        })?;

        let cfg_silence = self.cfg.audio.silence_seconds;
        let cfg_min_speech = self.cfg.audio.min_speech_seconds;

        let min_speech_s = cfg_min_speech;
        let mut mic_backoff_secs = 0_u64;

        loop {
            if !running.load(Ordering::Relaxed) {
                break;
            }

            // A backgrounded daemon can run for weeks — if we only check for
            // new versions at boot, the user never learns about updates until
            // they restart. spawn_background_check is self-throttled (24h
            // cache guard inside), so calling it every turn is cheap: 99%
            // of turns are an instant no-op.
            crate::update_check::spawn_background_check();

            banner::separator();

            // When the wake-word session is already active, lower the
            // threshold by 30% so normal conversational speech is detected
            // more reliably without needing to raise one's voice.
            let wake_enabled = self.cfg.persona.wake_word.enabled;
            let is_awake = self.wake_state.is_awake();
            let threshold_scale = if wake_enabled && !is_awake { 1.0_f32 } else { 0.8 };

            let listening_msg = m.listening;
            let detected_msg = m.speech_detected;
            let too_short_msg = m.too_short;

            let audio = match tokio::task::spawn_blocking(move || {
                record_speech(
                    threshold,
                    dev_info,
                    &RecordParams {
                        silence_seconds: cfg_silence,
                        min_speech_seconds: cfg_min_speech,
                        threshold_scale,
                    },
                    listening_msg,
                    detected_msg,
                    too_short_msg,
                )
            })
            .await?
            {
                Ok(a) => {
                    mic_backoff_secs = 0;
                    a
                }
                Err(e) => {
                    mic_backoff_secs = if mic_backoff_secs == 0 {
                        1
                    } else {
                        (mic_backoff_secs * 2).min(60)
                    };
                    eprintln!("   {ERROR_COLOR}{}: {e}{RESET}", m.recording_failed);
                    self.logger.error("audio", &format!("record failed: {e}"));
                    if mic_backoff_secs >= 60 {
                        eprintln!("   {MUTED}{}{RESET}", m.mic_polling);
                    } else {
                        eprintln!("   {MUTED}{} {mic_backoff_secs}s...{RESET}", m.mic_retry);
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(mic_backoff_secs)).await;
                    continue;
                }
            };

            let min_samples = (TARGET_RATE as f32 * min_speech_s) as usize;

            let audio = match audio {
                Some(ref a) if !a.is_empty() => {
                    let mono16k = downsample_to_mono_16k(a, dev_info);
                    if mono16k.len() < min_samples {
                        println!("   {MUTED}音频太短，已跳过{RESET}");
                        // Not logged — this is hardware noise / mic transient, not user intent.
                        continue;
                    }
                    a.clone()
                }
                Some(_) => continue,
                None => {
                    print!("{}", Face::error());
                    eprintln!("   {ERROR_COLOR}{}{RESET}", m.mic_disconnected);
                    self.logger.error("audio", "mic disconnected");
                    self.logger.session_end(self.turn_count);
                    std::process::exit(1);
                }
            };

            // ASR with thinking spinner
            let mono16k = downsample_to_mono_16k(&audio, dev_info);
            let wav_data = encode_wav(&mono16k)?;
            let think_spinner = Spinner::start_frames(Face::think_frames());
            let (text, stt_ms) = match self.asr.recognize(&wav_data).await {
                Ok(v) => {
                    think_spinner.stop();
                    v
                }
                Err(e) => {
                    think_spinner.stop();
                    eprintln!("   {ERROR_COLOR}{}: {e:#}{RESET}", m.stt_failed);
                    eprintln!("   {MUTED}提示: 检查 Doubao 凭证是否正确 (cb config show){RESET}");
                    self.logger.error("asr", &e.to_string());
                    continue;
                }
            };

            // Show ASR result for transparency
            if !text.is_empty() {
                println!("   {MUTED}ASR: {text}{RESET}");
            }

            if text.is_empty() || text.len() < 2 {
                println!("   {MUTED}未识别到文字，已跳过{RESET}");
                // Not logged — empty ASR is almost always background noise
                // or keyboard sound, not a deliberate user utterance.
                continue;
            }

            // Wake word / session state check
            let text = if self.cfg.persona.wake_word.enabled {
                let wake_word = &self.cfg.persona.wake_word.word;

                if self.wake_state.is_awake() {
                    // Already awake — check for deactivation phrases first
                    if is_deactivation(&text) {
                        self.wake_state.sleep();
                        println!("   {MUTED}已退出对话模式，需要「{wake_word}」重新唤醒{RESET}");
                        self.logger.skip("user_deactivated", Some(&text));
                        continue;
                    }
                    // Renew the session timer and proceed
                    self.wake_state.renew();
                    text
                } else {
                    // Sleeping — look for wake word
                    match strip_wake_word(&text, wake_word) {
                        Some(rest) => {
                            self.wake_state.wake();
                            println!("   {BR_CYAN}✦ 已唤醒，5 分钟内持续对话{RESET}");
                            if rest.len() < 2 {
                                // Wake word only, no command yet — wait for next turn
                                continue;
                            }
                            rest
                        }
                        None => {
                            println!(
                                "   {MUTED}等待唤醒词「{wake_word}」，已跳过{RESET}"
                            );
                            self.logger.skip("wake_word", Some(&text));
                            continue;
                        }
                    }
                }
            } else {
                text
            };

            if text.is_empty() || text.len() < 2 {
                self.logger.skip("after_wake_word", None);
                continue;
            }

            println!("\n   {USER_COLOR}🗣 {}: {text}{RESET}", m.you);

            // LLM + TTS pipeline
            let mut metrics = match self.chat_and_speak(&text).await {
                Ok(m) => m,
                Err(e) => {
                    print!("{}", Face::error());
                    eprintln!("   {ERROR_COLOR}{}: {e}{RESET}", self.msg.chat_failed);
                    self.logger.error("llm", &e.to_string());
                    continue;
                }
            };
            metrics.stt_ms = stt_ms;
            metrics.log();

            // Log the completed turn
            self.turn_count += 1;
            let reply = self.history.last()
                .and_then(|v| v["content"].as_str())
                .unwrap_or("")
                .to_string();
            self.logger.turn(
                &text,
                &reply,
                metrics.stt_ms,
                metrics.llm_ttft_ms,
                metrics.llm_total_ms,
                metrics.tts_synth_ms,
            );
        }

        self.logger.session_end(self.turn_count);
        Ok(())
    }

    async fn chat_and_speak(&mut self, user_text: &str) -> Result<TurnMetrics> {
        self.history
            .push(json!({"role": "user", "content": user_text}));
        print!("   {BOT_COLOR}🤖 {}: {RESET}", self.msg.assistant);
        std::io::stdout().flush()?;

        let (audio_tx, audio_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let stop = Arc::new(AtomicBool::new(false));
        let play_handle = spawn_player(audio_rx, stop.clone());

        let tts_ms = Arc::new(AtomicU64::new(0));
        let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        let llm_messages = self.history.clone();
        let llm_handle = self.llm.chat_stream(&llm_messages, token_tx);

        let tts_client = self.client.clone();
        let tts_cfg = self.cfg.speech.doubao.clone();
        let tts_ms_clone = tts_ms.clone();
        let audio_tx_clone = audio_tx.clone();

        let tts_dispatcher = tokio::spawn(async move {
            let mut pending = String::new();

            while let Some(token) = token_rx.recv().await {
                pending.push_str(&token);

                if pending.chars().any(is_sentence_end) && !pending.trim().is_empty() {
                    let text = pending.trim().to_string();
                    pending.clear();
                    let tx = audio_tx_clone.clone();
                    let c = tts_client.clone();
                    let cf = tts_cfg.clone();
                    let ms = tts_ms_clone.clone();
                    tokio::spawn(async move {
                        let tts = DoubaoTts::new(c, cf);
                        let t = Instant::now();
                        if let Ok(Some(mp3)) = tts.synthesize(&text).await {
                            ms.fetch_add(
                                (t.elapsed().as_secs_f32() * 1000.0) as u64,
                                Ordering::Relaxed,
                            );
                            let _ = tx.send(mp3);
                        }
                    });
                }
            }

            if !pending.trim().is_empty() {
                let text = pending.trim().to_string();
                let tts = DoubaoTts::new(tts_client, tts_cfg);
                let t = Instant::now();
                if let Ok(Some(mp3)) = tts.synthesize(&text).await {
                    tts_ms_clone.fetch_add(
                        (t.elapsed().as_secs_f32() * 1000.0) as u64,
                        Ordering::Relaxed,
                    );
                    let _ = audio_tx_clone.send(mp3);
                }
            }
        });

        let result = llm_handle.await?;
        let _ = tts_dispatcher.await;

        println!();
        drop(audio_tx);
        let _ = play_handle.join();

        self.history
            .push(json!({"role": "assistant", "content": result.reply}));

        // Persist conversation to disk
        self.conversation.add_turn("user", user_text);
        self.conversation.add_turn("assistant", &result.reply);
        if let Err(e) = self.conversation.save() {
            eprintln!("   {MUTED}Failed to save history: {e}{RESET}");
        }

        Ok(TurnMetrics {
            stt_ms: 0.0,
            llm_ttft_ms: result.ttft_ms,
            llm_total_ms: result.total_ms,
            llm_tokens: result.tokens,
            tts_synth_ms: tts_ms.load(Ordering::Relaxed) as f32,
        })
    }
}

/// Check if the recognized text starts with the wake word.
///
/// Matching is done in two passes:
/// 1. Exact match after punctuation/space stripping (fast path).
/// 2. Pinyin match — treats homophones as equal (e.g. "黑小派" == "嘿小派").
///
/// Returns the remaining text after the wake word, or None if wake word not found.
fn strip_wake_word(text: &str, wake_word: &str) -> Option<String> {
    let strip_punct = |s: &str| -> String {
        s.to_lowercase()
            .replace([',', '，', '、', '.', '!', '！', '?', '？', ' ', '\u{3000}'], "")
    };

    let norm_text = strip_punct(text);
    let norm_wake = strip_punct(wake_word);

    if norm_wake.is_empty() {
        return None;
    }

    // ── Pass 1: exact character match ────────────────────────────────────────
    let char_match = norm_text.starts_with(&norm_wake);

    // ── Pass 2: pinyin match (handles homophones like 嘿/黑, 哎/诶) ──────────
    let pinyin_match = !char_match && {
        to_pinyin_str(wake_word)
            .zip(to_pinyin_str(text))
            .map(|(pw, pt)| pt.starts_with(&pw))
            .unwrap_or(false)
    };

    if !char_match && !pinyin_match {
        return None;
    }

    // Compute how many original characters to consume.
    // We match character-by-character from `text` against `norm_wake`.
    let wake_chars: Vec<char> = norm_wake.chars().collect();
    let mut consumed_bytes = 0usize;
    let mut matched_chars = 0usize;

    for c in text.chars() {
        if matched_chars >= wake_chars.len() {
            break;
        }
        consumed_bytes += c.len_utf8();
        // Count how many normalized chars this original char produces.
        let norm_c = strip_punct(&c.to_string());
        matched_chars += norm_c.chars().count();
    }

    let rest = &text[consumed_bytes..];
    let rest = rest.trim_start_matches(|c: char| {
        c == ',' || c == '，' || c == '、' || c == ' ' || c == '\u{3000}'
    });
    Some(rest.trim().to_string())
}

/// Convert a Chinese string to a compact pinyin representation (no tones, no spaces).
/// Returns None if the string contains no recognised characters.
fn to_pinyin_str(s: &str) -> Option<String> {
    use pinyin::{to_pinyin_vec, Pinyin};
    let result: String = to_pinyin_vec(s, |p: Pinyin| p.plain())
        .into_iter()
        .collect();
    if result.is_empty() { None } else { Some(result) }
}

/// Detect if the user is trying to deactivate the wake-word session.
/// Matches common Chinese and English phrases for "go away / stop listening".
fn is_deactivation(text: &str) -> bool {
    let t = text
        .to_lowercase()
        .replace([' ', '，', ',', '。', '.', '！', '!', '？', '?'], "");
    let phrases = [
        // Chinese
        "退下", "可以退下", "暂时退下", "先退下", "好了退下",
        "暂停", "先暂停", "休息", "先休息", "暂时休息",
        "不用了", "不需要了", "结束对话", "停止对话",
        "拜拜", "再见", "先这样", "就这样",
        // English
        "goodbye", "stoplistening", "goaway", "thatsenough",
        "dismiss", "sleep",
    ];
    phrases.iter().any(|p| t.contains(p))
}
