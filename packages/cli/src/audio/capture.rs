use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::resample::chunk_rms;
use super::DeviceInfo;

const SILENCE_SECONDS_DEFAULT: f32 = 1.0;
const MIN_SPEECH_SECONDS_DEFAULT: f32 = 1.0;
const MIN_LOUD_CHUNKS: usize = 25;
const SPEECH_START_CHUNKS: usize = 12;

/// Noise threshold multiplier (higher = less sensitive)
const NOISE_MULTIPLIER: f32 = 6.0;
/// Minimum noise floor
const NOISE_FLOOR: f32 = 0.018;

use crate::ui::theme::{MUTED, RESET, BR_CYAN, BR_GREEN};

/// Get default input device info
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

/// Calibrate ambient noise level, returns (threshold, device_info)
pub fn calibrate_noise() -> Result<(f32, DeviceInfo)> {
    let (device, config, info) = get_input_device_info()?;

    print!(
        "   Calibrating noise... {MUTED}({}Hz {}ch){RESET}",
        info.sample_rate, info.channels
    );
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
    std::thread::sleep(std::time::Duration::from_secs(2));
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

/// Recording parameters
pub struct RecordParams {
    pub silence_seconds: f32,
    pub min_speech_seconds: f32,
}

impl Default for RecordParams {
    fn default() -> Self {
        Self {
            silence_seconds: SILENCE_SECONDS_DEFAULT,
            min_speech_seconds: MIN_SPEECH_SECONDS_DEFAULT,
        }
    }
}

/// Record speech until silence is detected, returns raw audio at native sample rate
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

    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(200);

    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let _ = tx.try_send(data.to_vec());
        },
        |err| eprintln!("Audio error: {err}"),
        None,
    )?;
    stream.play()?;

    let approx_chunk_frames = 1024_u32;
    let chunks_per_sec =
        (dev_info.sample_rate * dev_info.channels as u32) as f32 / approx_chunk_frames as f32;
    let silence_chunks = (chunks_per_sec * params.silence_seconds) as usize;
    let min_speech_samples =
        (dev_info.sample_rate as f32 * params.min_speech_seconds * dev_info.channels as f32)
            as usize;

    let mut buffer: Vec<f32> = Vec::new();
    let mut pre_buffer: Vec<Vec<f32>> = Vec::new();
    let mut silent_count: usize = 0;
    let mut loud_count: usize = 0;
    let mut pre_loud: usize = 0;
    let mut started = false;

    loop {
        let chunk = match rx.recv() {
            Ok(c) => c,
            Err(_) => break,
        };
        let volume = chunk_rms(&chunk);

        if !started {
            if volume > threshold {
                pre_loud += 1;
                pre_buffer.push(chunk);
                if pre_loud >= SPEECH_START_CHUNKS {
                    started = true;
                    for pb in &pre_buffer {
                        buffer.extend_from_slice(pb);
                    }
                    loud_count = pre_loud;
                    pre_buffer.clear();
                    println!("   {BR_GREEN}🟢 {msg_detected}{RESET}");
                }
            } else {
                pre_loud = 0;
                pre_buffer.push(chunk);
                if pre_buffer.len() > SPEECH_START_CHUNKS {
                    pre_buffer.remove(0);
                }
            }
        } else {
            buffer.extend_from_slice(&chunk);
            if volume > threshold {
                silent_count = 0;
                loud_count += 1;
            } else {
                silent_count += 1;
                if silent_count >= silence_chunks {
                    break;
                }
            }
        }
    }

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
