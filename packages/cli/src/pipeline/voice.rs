use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
    matches!(c, '。' | '！' | '？' | '，' | ',' | '.' | '!' | '?' | '\n')
}

pub struct VoicePipeline {
    cfg: AppConfig,
    client: Client,
    asr: Box<dyn Asr>,
    llm: OpenAiClient,
    history: Vec<Value>,
    conversation: Conversation,
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
        let llm = OpenAiClient::new(client.clone(), cfg.llm.clone());
        let msg = i18n::get(&cfg.locale);

        let history = vec![json!({
            "role": "system",
            "content": msg.system_prompt
        })];

        Ok(Self {
            cfg,
            client,
            asr,
            llm,
            history,
            conversation: Conversation::new(),
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
        let sp = Spinner::start_inline(
            &format!("{} ({})...", m.llm_connecting, self.cfg.llm.model),
            BR_BLUE,
        );
        // Simulate connection check
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        sp.stop_with(&format!(
            "   {BR_GREEN}✓{RESET} {BOLD}{}{RESET} ({}) {BR_GREEN}OK{RESET}",
            m.llm_connecting, self.cfg.llm.model
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

        // Ctrl+C handler
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        let goodbye = m.goodbye;
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
            println!("\n\n   {BR_CYAN}👋 {goodbye}{RESET}");
            std::process::exit(0);
        })?;

        let record_params = RecordParams {
            silence_seconds: self.cfg.audio.silence_seconds,
            min_speech_seconds: self.cfg.audio.min_speech_seconds,
        };

        let min_speech_s = self.cfg.audio.min_speech_seconds;
        let mut mic_backoff_secs = 0_u64;

        loop {
            if !running.load(Ordering::Relaxed) {
                break;
            }

            banner::separator();

            let params_silence = record_params.silence_seconds;
            let params_min_speech = record_params.min_speech_seconds;
            let listening_msg = m.listening;
            let detected_msg = m.speech_detected;
            let too_short_msg = m.too_short;

            // No spinner during listening — record_speech prints its own status
            // The spinner would conflict with stdout from the blocking recording thread

            let audio = match tokio::task::spawn_blocking(move || {
                record_speech(
                    threshold,
                    dev_info,
                    &RecordParams {
                        silence_seconds: params_silence,
                        min_speech_seconds: params_min_speech,
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
                        continue;
                    }
                    a.clone()
                }
                Some(_) => continue,
                None => {
                    print!("{}", Face::error());
                    eprintln!("   {ERROR_COLOR}{}{RESET}", m.mic_disconnected);
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
                    eprintln!("   {ERROR_COLOR}{}: {e}{RESET}", m.stt_failed);
                    continue;
                }
            };
            if text.is_empty() || text.len() < 2 {
                continue;
            }

            // Wake word check
            let text = if self.cfg.wake_word.enabled {
                let wake_word = match self.cfg.locale.as_str() {
                    "zh" => &self.cfg.wake_word.word_zh,
                    _ => &self.cfg.wake_word.word_en,
                };
                match strip_wake_word(&text, wake_word) {
                    Some(rest) => rest,
                    None => continue,
                }
            } else {
                text
            };

            if text.is_empty() || text.len() < 2 {
                continue;
            }

            println!("\n   {USER_COLOR}🗣 {}: {text}{RESET}", m.you);

            // LLM + TTS pipeline
            let mut metrics = match self.chat_and_speak(&text).await {
                Ok(m) => m,
                Err(e) => {
                    print!("{}", Face::error());
                    eprintln!("   {ERROR_COLOR}{}: {e}{RESET}", self.msg.chat_failed);
                    continue;
                }
            };
            metrics.stt_ms = stt_ms;
            metrics.log();
        }

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

/// Check if the recognized text starts with the wake word (case-insensitive, punctuation-tolerant).
/// Returns the remaining text after the wake word, or None if wake word not found.
fn strip_wake_word(text: &str, wake_word: &str) -> Option<String> {
    let normalize = |s: &str| -> String {
        s.to_lowercase()
            .replace([',', '，', '、', '.', '!', '！', '?', '？', ' ', '\u{3000}'], "")
    };

    let norm_text = normalize(text);
    let norm_wake = normalize(wake_word);

    if norm_wake.is_empty() {
        return None;
    }

    if !norm_text.starts_with(&norm_wake) {
        return None;
    }

    let mut consumed = 0;
    let mut matched = 0;
    for c in text.chars() {
        if matched >= norm_wake.len() {
            break;
        }
        consumed += c.len_utf8();
        let norm_c = normalize(&c.to_string());
        matched += norm_c.len();
    }

    let rest = &text[consumed..];
    let rest = rest.trim_start_matches(|c: char| {
        c == ',' || c == '，' || c == '、' || c == ' ' || c == '\u{3000}'
    });
    let rest = rest.trim().to_string();

    Some(rest)
}
