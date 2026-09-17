//! 常驻语音主循环：录音 → ASR → 唤醒判定 → 一轮对话 → 记账。
//!
//! 这里只做编排：唤醒词规则在 `domain::wake`，分句/合成/播放在 `speak`，
//! ASR/TTS 通过 `speech` 工厂拿到 trait 对象，不感知具体供应商。
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};

use crate::audio::capture::{calibrate_noise, record_speech, RecordParams};
use crate::audio::resample::{downsample_to_mono_16k, encode_wav};
use crate::audio::TARGET_RATE;
use crate::config::AppConfig;
use crate::domain::metrics::TurnMetrics;
use crate::domain::wake::{decide, WakeDecision, WakeState};
use crate::llm::OpenAiClient;
use crate::pipeline::speak::speak_turn;
use crate::platform::{notify, update};
use crate::speech::{build_asr, build_tts, Asr, Tts};
use crate::storage::events::EventLogger;
use crate::storage::history::Conversation;
use crate::ui::i18n::{self, Messages};
use crate::ui::{art::Face, banner, spinner::Spinner, theme::*};

pub struct VoicePipeline {
    cfg: AppConfig,
    asr: Box<dyn Asr>,
    tts: Arc<dyn Tts>,
    llm: OpenAiClient,
    history: Vec<Value>,
    conversation: Conversation,
    logger: EventLogger,
    turn_count: usize,
    wake_state: WakeState,
    msg: &'static Messages,
}

impl VoicePipeline {
    pub fn new(cfg: AppConfig, debug: bool) -> Result<Self> {
        cfg.validate()?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let asr = build_asr(&cfg, debug)?;
        let tts = build_tts(&cfg)?;
        let llm_config = cfg
            .active_llm_config()
            .ok_or_else(|| anyhow::anyhow!("No active LLM profile. Run `cb config`"))?;
        let llm = OpenAiClient::new(client, llm_config);
        let msg = i18n::get(&cfg.persona.language);

        let system_prompt = i18n::build_system_prompt(
            &cfg.persona.language,
            &cfg.persona.name,
            cfg.persona.wake_word.enabled,
            &cfg.persona.wake_word.word,
        );
        let history = vec![json!({ "role": "system", "content": system_prompt })];

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
            asr,
            tts,
            llm,
            history,
            conversation: Conversation::new(),
            logger,
            turn_count: 0,
            wake_state: WakeState::Sleeping,
            msg,
        })
    }

    pub async fn run_loop(&mut self) -> Result<()> {
        let m = self.msg;

        banner::print_banner(env!("CARGO_PKG_VERSION"));
        println!("{}", Face::idle());

        let llm_model = self
            .cfg
            .active_llm_profile()
            .map(|p| p.model.as_str())
            .unwrap_or("unknown")
            .to_string();
        let sp = Spinner::start_inline(&format!("{} ({})...", m.llm_connecting, llm_model), BR_BLUE);
        self.logger.session_start(
            &llm_model,
            &self.cfg.speech.doubao.voice_type,
            &self.cfg.persona.language,
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        sp.stop_with(&format!(
            "   {BR_GREEN}✓{RESET} {BOLD}{}{RESET} ({}) {BR_GREEN}OK{RESET}",
            m.llm_connecting, llm_model
        ));

        let sp = Spinner::start_inline(&format!("{}...", m.speech_api), BR_BLUE);
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
                            m.calibrating_noise, v.0 / 6.0, v.0
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
                            eprintln!("   {MUTED}{} {backoff_secs}s...{RESET}", m.mic_retry);
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(60);
                    }
                }
            }
        };

        banner::print_ready(&[m.ready_banner[0], m.ready_banner[1], m.ready_banner[2]]);

        if let Some(v) = update::pending_notice() {
            let hint = update::upgrade_hint();
            println!("   {BR_CYAN}⬆  发现新版本 v{v}，运行 {BOLD}{hint}{RESET}{BR_CYAN} 升级{RESET}\n");
            // Daemon stdout is a log file — also raise an OS notification so
            // backgrounded users actually find out.
            notify::notify_desktop(&v, &hint);
        }

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
        let min_samples = (TARGET_RATE as f32 * cfg_min_speech) as usize;
        let mut mic_backoff_secs = 0_u64;

        loop {
            if !running.load(Ordering::Relaxed) {
                break;
            }

            // Self-throttled (24h cache) — cheap to call every turn, and it's
            // the only way a weeks-old daemon ever learns about a release.
            update::spawn_background_check();

            banner::separator();

            // Awake session: 20% more sensitive so normal speech is caught
            // without raising one's voice.
            let wake_enabled = self.cfg.persona.wake_word.enabled;
            let threshold_scale = if wake_enabled && !self.wake_state.is_awake() { 1.0_f32 } else { 0.8 };

            let (listening_msg, detected_msg, too_short_msg) = (m.listening, m.speech_detected, m.too_short);
            let audio = match tokio::task::spawn_blocking(move || {
                record_speech(
                    threshold,
                    dev_info,
                    &RecordParams { silence_seconds: cfg_silence, min_speech_seconds: cfg_min_speech, threshold_scale },
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
                    mic_backoff_secs = if mic_backoff_secs == 0 { 1 } else { (mic_backoff_secs * 2).min(60) };
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

            let mono16k = match audio {
                Some(ref a) if !a.is_empty() => {
                    let mono16k = downsample_to_mono_16k(a, dev_info);
                    if mono16k.len() < min_samples {
                        println!("   {MUTED}音频太短，已跳过{RESET}");
                        continue; // hardware transient, not user intent — not logged
                    }
                    mono16k
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

            // ASR
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

            if !text.is_empty() {
                println!("   {MUTED}ASR: {text}{RESET}");
            }
            if text.chars().count() < 2 {
                println!("   {MUTED}未识别到文字，已跳过{RESET}");
                continue; // almost always noise — not logged
            }

            // Wake word
            let wake_word = self.cfg.persona.wake_word.word.clone();
            let text = match decide(&mut self.wake_state, wake_enabled, &wake_word, &text) {
                WakeDecision::Proceed(t) => t,
                WakeDecision::JustWoke(rest) => {
                    println!("   {BR_CYAN}✦ 已唤醒，5 分钟内持续对话{RESET}");
                    if rest.chars().count() < 2 {
                        continue; // wake word only — wait for the actual question
                    }
                    rest
                }
                WakeDecision::Deactivate => {
                    println!("   {MUTED}已退出对话模式，需要「{wake_word}」重新唤醒{RESET}");
                    self.logger.skip("user_deactivated", Some(&text));
                    continue;
                }
                WakeDecision::Ignore => {
                    println!("   {MUTED}等待唤醒词「{wake_word}」，已跳过{RESET}");
                    self.logger.skip("wake_word", Some(&text));
                    continue;
                }
            };

            println!("\n   {USER_COLOR}🗣 {}: {text}{RESET}", m.you);

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
            println!("   {MUTED}{}{RESET}", metrics.summary());

            self.turn_count += 1;
            let reply = self.history.last().and_then(|v| v["content"].as_str()).unwrap_or("").to_string();
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
        self.history.push(json!({"role": "user", "content": user_text}));
        print!("   {BOT_COLOR}🤖 {}: {RESET}", self.msg.assistant);
        std::io::stdout().flush()?;

        let outcome = speak_turn(&self.llm, &self.history, self.tts.clone()).await?;

        self.history.push(json!({"role": "assistant", "content": outcome.reply}));
        self.conversation.add_turn("user", user_text);
        self.conversation.add_turn("assistant", &outcome.reply);
        if let Err(e) = self.conversation.save() {
            eprintln!("   {MUTED}Failed to save history: {e}{RESET}");
        }

        Ok(TurnMetrics {
            stt_ms: 0.0,
            llm_ttft_ms: outcome.llm_ttft_ms,
            llm_total_ms: outcome.llm_total_ms,
            llm_tokens: outcome.llm_tokens,
            tts_synth_ms: outcome.tts_synth_ms,
        })
    }
}
