//! 一轮"说话"：LLM token 流实时切句、并发合成、按序播放；说话期间监听打断。
//!
//! 结束条件二选一：
//! - 正常：LLM 说完 → 所有句子合成完 → 播放器播完（一条链，播放器退出即全部完成）。
//! - 打断：麦克风监听到用户开口 → 立刻停播、掐掉 LLM 与合成任务，把用户这句录完带回去。
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::Value;
use tokio::sync::{oneshot, Semaphore};

use crate::audio::capture::{listen_for_barge_in, RecordParams};
use crate::audio::playback::{spawn_player, PlayItem, PlayerState};
use crate::audio::DeviceInfo;
use crate::domain::sentence::{is_speakable, SentenceBuffer};
use crate::llm::OpenAiClient;
use crate::speech::{Tts, TtsOptions};

/// 并发合成上限：LLM 突发输出时别把豆包的 QPS 打爆（普通账号约 5-10）。
const TTS_CONCURRENCY: usize = 3;

/// 打断监听所需的麦克风参数。
pub struct BargeIn {
    pub threshold: f32,
    pub dev_info: DeviceInfo,
    pub params: RecordParams,
}

pub struct TurnOutcome {
    /// LLM 完整回复；被打断时是打断前已生成的部分。
    pub reply: String,
    /// 真正说出口（开始播放）的句子。
    pub spoken: String,
    /// 打断者说的那句话（原生采样率）；`None` = 正常结束。
    pub interrupted_by: Option<Vec<f32>>,
    pub llm_ttft_ms: f32,
    pub llm_total_ms: f32,
    pub llm_tokens: usize,
    /// 各句合成耗时之和（并发，所以不等于墙钟时间）。
    pub tts_synth_ms: f32,
}

pub struct SpeakRequest<'a> {
    pub llm: &'a OpenAiClient,
    pub messages: Vec<Value>,
    pub tts: Arc<dyn Tts>,
    pub tts_opts: TtsOptions,
    /// `None` = 不监听打断。
    pub barge_in: Option<BargeIn>,
}

pub async fn speak_turn(req: SpeakRequest<'_>) -> Result<TurnOutcome> {
    let SpeakRequest { llm, messages, tts, tts_opts, barge_in } = req;

    // ── 播放器：每句开播时把文本打到终端，让文字和语音同步 ──
    let (audio_tx, audio_rx) = std::sync::mpsc::channel::<PlayItem>();
    let player = PlayerState::new();
    let play_handle = spawn_player(audio_rx, player.clone(), |text| {
        use std::io::Write;
        print!("{text}");
        let _ = std::io::stdout().flush();
    });

    // ── LLM 流：token 进 channel；partial 文本另存一份以备被打断 ──
    let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let llm_owned = llm.clone();
    let llm_task = tokio::spawn(async move { llm_owned.chat_stream(&messages, token_tx, false).await });

    // ── 分句 → 并发合成 → 按序转发 ──
    let (order_tx, mut order_rx) =
        tokio::sync::mpsc::unbounded_channel::<oneshot::Receiver<Option<PlayItem>>>();
    let forwarder = tokio::spawn(async move {
        while let Some(rx) = order_rx.recv().await {
            if let Ok(Some(item)) = rx.await {
                if audio_tx.send(item).is_err() {
                    break;
                }
            }
        }
        // audio_tx 在此 drop → 播放器播完剩余队列后退出
    });

    let tts_ms = Arc::new(AtomicU64::new(0));
    let partial = Arc::new(Mutex::new(String::new()));
    let sem = Arc::new(Semaphore::new(TTS_CONCURRENCY));
    let (tts_ms_d, partial_d, opts_d) = (tts_ms.clone(), partial.clone(), Arc::new(tts_opts));
    let dispatcher = tokio::spawn(async move {
        let dispatch = |text: String| {
            if !is_speakable(&text) {
                return;
            }
            let (done_tx, done_rx) = oneshot::channel();
            if order_tx.send(done_rx).is_err() {
                return;
            }
            let (tts, sem, ms, opts) = (tts.clone(), sem.clone(), tts_ms_d.clone(), opts_d.clone());
            tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.ok();
                let t = Instant::now();
                let audio = tts.synthesize(&text, &opts).await.ok().flatten();
                if audio.is_some() {
                    ms.fetch_add((t.elapsed().as_secs_f32() * 1000.0) as u64, Ordering::Relaxed);
                }
                let _ = done_tx.send(audio.map(|audio| PlayItem { text, audio }));
            });
        };

        let mut buf = SentenceBuffer::new();
        while let Some(token) = token_rx.recv().await {
            if let Ok(mut p) = partial_d.lock() {
                p.push_str(&token);
            }
            if let Some(sentence) = buf.push(&token) {
                dispatch(sentence);
            }
        }
        if let Some(rest) = buf.flush() {
            dispatch(rest);
        }
    });

    // ── 打断监听（阻塞线程）：触发即置位 triggered，随后继续录到静音 ──
    let cancel = Arc::new(AtomicBool::new(false));
    let triggered = Arc::new(AtomicBool::new(false));
    let monitor = barge_in.map(|b| {
        let (playing, cancel, triggered) = (player.playing.clone(), cancel.clone(), triggered.clone());
        tokio::task::spawn_blocking(move || {
            listen_for_barge_in(b.threshold, b.dev_info, &b.params, playing, cancel, triggered)
        })
    });
    let trigger_wait = {
        let triggered = triggered.clone();
        let enabled = monitor.is_some();
        async move {
            if !enabled {
                std::future::pending::<()>().await;
            }
            while !triggered.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        }
    };

    // 播放器退出 = 整条链（LLM → 分句 → 合成 → 转发）都结束了
    let player_done = tokio::task::spawn_blocking(move || play_handle.join());

    let interrupted = tokio::select! {
        _ = player_done => false,
        _ = trigger_wait => true,
    };

    let outcome = if interrupted {
        player.stop.store(true, Ordering::SeqCst);
        llm_task.abort();
        dispatcher.abort();
        forwarder.abort();
        let audio = match monitor {
            Some(m) => m.await.ok().and_then(|r| r.ok()).flatten().unwrap_or_default(),
            None => Vec::new(),
        };
        let reply = partial.lock().map(|p| p.clone()).unwrap_or_default();
        println!();
        TurnOutcome {
            reply,
            spoken: player.spoken_text(),
            interrupted_by: Some(audio),
            llm_ttft_ms: 0.0,
            llm_total_ms: 0.0,
            llm_tokens: 0,
            tts_synth_ms: tts_ms.load(Ordering::Relaxed) as f32,
        }
    } else {
        cancel.store(true, Ordering::SeqCst);
        if let Some(m) = monitor {
            let _ = m.await;
        }
        let _ = dispatcher.await;
        let _ = forwarder.await;
        let result = llm_task.await??;
        println!();
        TurnOutcome {
            reply: result.reply,
            spoken: player.spoken_text(),
            interrupted_by: None,
            llm_ttft_ms: result.ttft_ms,
            llm_total_ms: result.total_ms,
            llm_tokens: result.tokens,
            tts_synth_ms: tts_ms.load(Ordering::Relaxed) as f32,
        }
    };
    Ok(outcome)
}
