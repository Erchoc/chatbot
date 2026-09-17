//! 一轮"说话"：把 LLM 的 token 流实时切句、并发合成、按序播放。
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::{oneshot, Semaphore};

use crate::audio::playback::spawn_player;
use crate::domain::sentence::{is_speakable, SentenceBuffer};
use crate::llm::OpenAiClient;
use crate::speech::Tts;

/// 并发合成上限：LLM 突发输出时别把豆包的 QPS 打爆（普通账号约 5-10）。
const TTS_CONCURRENCY: usize = 3;

pub struct TurnOutcome {
    pub reply: String,
    pub llm_ttft_ms: f32,
    pub llm_total_ms: f32,
    pub llm_tokens: usize,
    /// 各句合成耗时之和（并发，所以不等于墙钟时间）。
    pub tts_synth_ms: f32,
}

/// 流式请求 LLM，边出 token 边合成、边播放；返回完整回复与指标。
///
/// 顺序保证：dispatcher 按切句顺序把 `oneshot::Receiver` 推进队列，
/// forwarder 依次 await 再送播放器 —— 短句可以先合成完，但绝不会抢播。
pub async fn speak_turn(
    llm: &OpenAiClient,
    messages: &[Value],
    tts: Arc<dyn Tts>,
) -> Result<TurnOutcome> {
    let (audio_tx, audio_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let stop = Arc::new(AtomicBool::new(false));
    let play_handle = spawn_player(audio_rx, stop.clone());

    let tts_ms = Arc::new(AtomicU64::new(0));
    let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let llm_handle = llm.chat_stream(messages, token_tx);

    let (order_tx, mut order_rx) =
        tokio::sync::mpsc::unbounded_channel::<oneshot::Receiver<Option<Vec<u8>>>>();

    let audio_tx_forwarder = audio_tx.clone();
    let forwarder = tokio::spawn(async move {
        while let Some(rx) = order_rx.recv().await {
            if let Ok(Some(audio)) = rx.await {
                if audio_tx_forwarder.send(audio).is_err() {
                    break;
                }
            }
        }
    });

    let sem = Arc::new(Semaphore::new(TTS_CONCURRENCY));
    let tts_ms_dispatch = tts_ms.clone();
    let dispatcher = tokio::spawn(async move {
        let dispatch = |text: String| {
            if !is_speakable(&text) {
                return;
            }
            let (done_tx, done_rx) = oneshot::channel();
            if order_tx.send(done_rx).is_err() {
                return;
            }
            let (tts, sem, ms) = (tts.clone(), sem.clone(), tts_ms_dispatch.clone());
            tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.ok();
                let t = Instant::now();
                let audio = tts.synthesize(&text).await.ok().flatten();
                if audio.is_some() {
                    ms.fetch_add((t.elapsed().as_secs_f32() * 1000.0) as u64, Ordering::Relaxed);
                }
                let _ = done_tx.send(audio);
            });
        };

        let mut buf = SentenceBuffer::new();
        while let Some(token) = token_rx.recv().await {
            if let Some(sentence) = buf.push(&token) {
                dispatch(sentence);
            }
        }
        if let Some(rest) = buf.flush() {
            dispatch(rest);
        }
        // order_tx 随闭包一起 drop，forwarder 随之收尾
    });

    let result = llm_handle.await?;
    let _ = dispatcher.await;
    let _ = forwarder.await;

    println!();
    drop(audio_tx);
    let _ = play_handle.join();

    Ok(TurnOutcome {
        reply: result.reply,
        llm_ttft_ms: result.ttft_ms,
        llm_total_ms: result.total_ms,
        llm_tokens: result.tokens,
        tts_synth_ms: tts_ms.load(Ordering::Relaxed) as f32,
    })
}
