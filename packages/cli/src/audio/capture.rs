//! 麦克风采集 + 能量 VAD。
//!
//! 两种用法：
//! - [`record_speech`]：安静时等人开口，录到静音为止（主循环）。
//! - [`listen_for_barge_in`]：助手正在说话时监听打断。麦克风会听到扬声器的回声，
//!   所以先用播放开头的几百毫秒估计"回声电平"，打断阈值取回声电平与基础阈值的较大者
//!   再放大；检测到持续人声即置位 `triggered`（调用方立刻停播），并继续录到静音。
//!
//! VAD 参数见 CLAUDE.md「VAD Tuning」，改一个参数就要拿键盘敲击实测一遍。
use std::collections::VecDeque;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::resample::chunk_rms;
use super::DeviceInfo;
use crate::ui::theme::{BR_CYAN, BR_GREEN, MUTED, RESET};

const SILENCE_SECONDS_DEFAULT: f32 = 1.0;
const MIN_SPEECH_SECONDS_DEFAULT: f32 = 0.4;

/// 滑窗大小（块数）。~47 块/秒（48kHz 单声道 1024 帧一块），30 块 ≈ 0.6s。
const VAD_WINDOW: usize = 30;
/// 窗内至少多少块是响的才算人声开口：22/30 = 73%，键盘敲击 ~40% 到不了。
const VAD_TRIGGER: usize = 22;
/// 一段录音里至少要有多少响块，否则当噪声丢弃。
const MIN_LOUD_CHUNKS: usize = 15;

/// 阈值 = 环境噪声 × 倍数。
const NOISE_MULTIPLIER: f32 = 5.0;
/// 阈值下限，防止极安静房间里校准得过于灵敏。
const NOISE_FLOOR: f32 = 0.014;

/// 打断监听：先用这么多块（≈0.7s）估计扬声器回声电平。
const BARGE_IN_CALIB_CHUNKS: usize = 32;
/// 打断阈值至少是基础阈值的这么多倍。
const BARGE_IN_MIN_SCALE: f32 = 2.0;
/// 打断阈值至少是回声电平的这么多倍（要盖过自己的声音）。
const BARGE_IN_ECHO_SCALE: f32 = 2.2;
/// 助手没出声（LLM 思考 / 句间空隙）时的打断门限倍数：没有回声，略高于普通录音即可。
const BARGE_IN_QUIET_SCALE: f32 = 1.2;

/// 默认输入设备与流配置。
pub fn get_input_device_info() -> Result<(cpal::Device, cpal::StreamConfig, DeviceInfo)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("No microphone detected, please connect one and retry")?;
    let default_config = device
        .default_input_config()
        .context("Failed to get default microphone config")?;
    let info = DeviceInfo {
        sample_rate: default_config.sample_rate().0,
        channels: default_config.channels(),
    };
    let config = cpal::StreamConfig {
        channels: info.channels,
        sample_rate: default_config.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };
    Ok((device, config, info))
}

/// 校准环境噪声，返回 (阈值, 设备信息)。
pub fn calibrate_noise() -> Result<(f32, DeviceInfo)> {
    let (device, config, info) = get_input_device_info()?;
    print!("   Calibrating noise... {MUTED}({}Hz {}ch){RESET}", info.sample_rate, info.channels);
    std::io::stdout().flush()?;

    let levels = Arc::new(Mutex::new(Vec::<f32>::new()));
    let levels_w = levels.clone();
    let done = Arc::new(AtomicBool::new(false));
    let done_r = done.clone();
    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            if !done_r.load(Ordering::Relaxed) {
                levels_w.lock().unwrap().push(chunk_rms(data));
            }
        },
        |err| eprintln!("Audio error: {err}"),
        None,
    )?;
    stream.play()?;
    std::thread::sleep(Duration::from_secs(2));
    done.store(true, Ordering::Relaxed);
    drop(stream);

    let lvs = levels.lock().unwrap();
    if lvs.is_empty() {
        anyhow::bail!("Calibration failed: no audio data captured");
    }
    let ambient: f32 = lvs.iter().sum::<f32>() / lvs.len() as f32;
    let threshold = (ambient * NOISE_MULTIPLIER).max(NOISE_FLOOR);
    println!(" {MUTED}noise:{ambient:.4} threshold:{threshold:.4}{RESET}");
    Ok((threshold, info))
}

/// 录音参数。
pub struct RecordParams {
    pub silence_seconds: f32,
    pub min_speech_seconds: f32,
    /// 作用在校准阈值上的倍数：1.0 正常（等唤醒词），<1.0 更灵敏（已唤醒时用 0.8）。
    pub threshold_scale: f32,
}

impl Default for RecordParams {
    fn default() -> Self {
        Self {
            silence_seconds: SILENCE_SECONDS_DEFAULT,
            min_speech_seconds: MIN_SPEECH_SECONDS_DEFAULT,
            threshold_scale: 1.0,
        }
    }
}

/// 滑窗开口检测（纯逻辑，可单测）：最近 `VAD_WINDOW` 块里 ≥ `VAD_TRIGGER` 块响即触发。
#[derive(Debug, Default)]
pub struct SpeechGate {
    window: VecDeque<bool>,
}

impl SpeechGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// 推入一块的响/静，返回是否触发；触发后返回窗内响块数。
    pub fn push(&mut self, loud: bool) -> Option<usize> {
        self.window.push_back(loud);
        if self.window.len() > VAD_WINDOW {
            self.window.pop_front();
        }
        let loud_in_window = self.window.iter().filter(|&&v| v).count();
        if self.window.len() >= VAD_WINDOW && loud_in_window >= VAD_TRIGGER {
            self.window.clear();
            Some(loud_in_window)
        } else {
            None
        }
    }
}

/// 打开输入流，把每块样本丢进有界队列。返回的 stream 一 drop 就停。
fn open_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
) -> Result<(cpal::Stream, Receiver<Vec<f32>>)> {
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(200);
    let stream = device.build_input_stream(
        config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let _ = tx.try_send(data.to_vec());
        },
        |err| eprintln!("Audio error: {err}"),
        None,
    )?;
    stream.play()?;
    Ok((stream, rx))
}

fn silence_chunks_for(dev_info: DeviceInfo, silence_seconds: f32) -> usize {
    let approx_chunk_frames = 1024_u32;
    let chunks_per_sec =
        (dev_info.sample_rate * dev_info.channels as u32) as f32 / approx_chunk_frames as f32;
    (chunks_per_sec * silence_seconds).max(1.0) as usize
}

/// 开口之后：一直录到连续 `silence_chunks` 块静音。返回 (音频, 响块数)。
fn capture_until_silence(
    rx: &Receiver<Vec<f32>>,
    mut buffer: Vec<f32>,
    mut loud_count: usize,
    threshold: f32,
    silence_chunks: usize,
) -> (Vec<f32>, usize) {
    let mut silent = 0usize;
    while let Ok(chunk) = rx.recv() {
        let loud = chunk_rms(&chunk) > threshold;
        buffer.extend_from_slice(&chunk);
        if loud {
            silent = 0;
            loud_count += 1;
        } else {
            silent += 1;
            if silent >= silence_chunks {
                break;
            }
        }
    }
    (buffer, loud_count)
}

/// 等人开口、录到静音。返回原生采样率的样本；太短 / 噪声返回 `Some(vec![])`。
pub fn record_speech(
    threshold: f32,
    dev_info: DeviceInfo,
    params: &RecordParams,
    msg_listening: &str,
    msg_detected: &str,
    msg_too_short: &str,
) -> Result<Option<Vec<f32>>> {
    let (device, config, _) = get_input_device_info()?;
    println!("\n  {BR_CYAN}🎤 {msg_listening}{RESET}");
    let (stream, rx) = open_stream(&device, &config)?;

    let effective_threshold = threshold * params.threshold_scale;
    let silence_chunks = silence_chunks_for(dev_info, params.silence_seconds);
    let min_speech_samples =
        (dev_info.sample_rate as f32 * params.min_speech_seconds * dev_info.channels as f32) as usize;

    let mut gate = SpeechGate::new();
    let mut pre_buffer: VecDeque<Vec<f32>> = VecDeque::with_capacity(VAD_WINDOW + 2);
    let mut opened: Option<usize> = None;
    while let Ok(chunk) = rx.recv() {
        let loud = chunk_rms(&chunk) > effective_threshold;
        pre_buffer.push_back(chunk);
        if pre_buffer.len() > VAD_WINDOW + 2 {
            pre_buffer.pop_front();
        }
        if let Some(n) = gate.push(loud) {
            opened = Some(n);
            break;
        }
    }
    let Some(loud_in_window) = opened else {
        drop(stream);
        return Ok(None); // 输入流断了
    };
    println!("   {BR_GREEN}🟢 {msg_detected}{RESET}");

    // 把开口前的预缓冲一起带上，避免吃掉第一个字
    let buffer: Vec<f32> = pre_buffer.into_iter().flatten().collect();
    let (buffer, loud_count) =
        capture_until_silence(&rx, buffer, loud_in_window, effective_threshold, silence_chunks);
    drop(stream);

    if loud_count < MIN_LOUD_CHUNKS {
        println!("   {MUTED}{msg_too_short}{RESET}");
        return Ok(Some(Vec::new()));
    }
    if buffer.len() < min_speech_samples {
        return Ok(Some(Vec::new()));
    }
    Ok(Some(buffer))
}

/// 助手说话期间监听打断。
///
/// - `playing`：播放器正在出声（用来只在有回声时做回声校准）。
/// - `cancel`：本轮正常结束，停止监听（返回 `None`）。
/// - `triggered`：检测到用户开口时置位，调用方据此立刻停播。
///
/// 返回打断者说的那句话（原生采样率），用作下一轮的输入；未触发返回 `None`。
pub fn listen_for_barge_in(
    threshold: f32,
    dev_info: DeviceInfo,
    params: &RecordParams,
    playing: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    triggered: Arc<AtomicBool>,
) -> Result<Option<Vec<f32>>> {
    let (device, config, _) = get_input_device_info()?;
    let (stream, rx) = open_stream(&device, &config)?;

    let base = threshold * params.threshold_scale;
    let silence_chunks = silence_chunks_for(dev_info, params.silence_seconds);

    // ── 1. 回声校准：播放中的前 N 块，取最大值当回声电平 ──
    let mut echo_level = 0.0_f32;
    let mut calib_seen = 0usize;
    let mut gate = SpeechGate::new();
    let mut pre_buffer: VecDeque<Vec<f32>> = VecDeque::with_capacity(VAD_WINDOW + 2);

    let loud_in_window = loop {
        if cancel.load(Ordering::Relaxed) {
            drop(stream);
            return Ok(None);
        }
        let chunk = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(c) => c,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                drop(stream);
                return Ok(None);
            }
        };
        let level = chunk_rms(&chunk);

        let is_playing = playing.load(Ordering::Relaxed);
        if is_playing && calib_seen < BARGE_IN_CALIB_CHUNKS {
            // 回声校准期：只学不判
            echo_level = echo_level.max(level);
            calib_seen += 1;
            pre_buffer.push_back(chunk);
            if pre_buffer.len() > VAD_WINDOW + 2 {
                pre_buffer.pop_front();
            }
            continue;
        }

        // 出声时要盖过回声；助手还在"想"（静音）时麦克风里只有用户，门限贴近基础阈值即可
        let barge_threshold = if is_playing {
            (base * BARGE_IN_MIN_SCALE).max(echo_level * BARGE_IN_ECHO_SCALE)
        } else {
            base * BARGE_IN_QUIET_SCALE
        };
        pre_buffer.push_back(chunk);
        if pre_buffer.len() > VAD_WINDOW + 2 {
            pre_buffer.pop_front();
        }
        if let Some(n) = gate.push(level > barge_threshold) {
            break n;
        }
    };

    // ── 2. 触发：先叫停播放，再按普通阈值录到静音（播放停了回声就没了） ──
    triggered.store(true, Ordering::SeqCst);
    let buffer: Vec<f32> = pre_buffer.into_iter().flatten().collect();
    let (buffer, loud_count) =
        capture_until_silence(&rx, buffer, loud_in_window, base, silence_chunks);
    drop(stream);

    if loud_count < MIN_LOUD_CHUNKS {
        return Ok(Some(Vec::new()));
    }
    Ok(Some(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sustained_sound_opens_gate() {
        let mut g = SpeechGate::new();
        let mut fired = None;
        for _ in 0..VAD_WINDOW {
            fired = g.push(true).or(fired);
        }
        assert_eq!(fired, Some(VAD_WINDOW));
    }

    #[test]
    fn percussive_keyboard_pattern_never_opens_gate() {
        // 响-静-静 循环 ≈ 33% 密度，远低于 73%
        let mut g = SpeechGate::new();
        for i in 0..(VAD_WINDOW * 4) {
            assert_eq!(g.push(i % 3 == 0), None);
        }
    }

    #[test]
    fn gate_needs_a_full_window_first() {
        let mut g = SpeechGate::new();
        for _ in 0..(VAD_WINDOW - 1) {
            assert_eq!(g.push(true), None);
        }
    }
}
