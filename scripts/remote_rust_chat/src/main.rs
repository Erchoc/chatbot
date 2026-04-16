//! 远程语音对话机器人 (Rust)
//! 麦克风 → 豆包ASR(STT) → LLM(流式) → 豆包TTS → 扬声器
//!
//! 与 local_python_chat 同体验，语音能力全部走豆包 OpenAPI。

use std::io::{self, Cursor, Read as IoRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

static DEBUG: AtomicBool = AtomicBool::new(false);

macro_rules! debug_log {
    ($($arg:tt)*) => {
        if DEBUG.load(Ordering::Relaxed) {
            eprintln!($($arg)*);
        }
    };
}

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use rodio::{Decoder, OutputStream, Sink};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

// ============ 常量 ============
const TARGET_RATE: u32 = 16000; // ASR 目标采样率
const SILENCE_SECONDS: f32 = 0.8;
const MIN_SPEECH_SECONDS: f32 = 0.8;
const MIN_LOUD_CHUNKS: usize = 15;
const SPEECH_START_CHUNKS: usize = 8;
const TTS_SPEED: f32 = 1.3;

const DIM: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

// WebSocket 二进制协议帧头 (豆包 ASR v2)
// [version:4bit|header_size:4bit] [msg_type:4bit|flags:4bit] [serial:4bit|compress:4bit] [reserved]
const WS_FULL_CLIENT: [u8; 4] = [0x11, 0x10, 0x11, 0x00]; // full request, JSON+GZIP
const WS_AUDIO_ONLY: [u8; 4] = [0x11, 0x20, 0x11, 0x00]; // audio only, GZIP
const WS_LAST_AUDIO: [u8; 4] = [0x11, 0x22, 0x11, 0x00]; // last audio, neg_sequence
const ASR_SEG_SIZE: usize = 160_000; // 5s @ 16kHz × 2bytes

// ============ 音频设备信息 ============
#[derive(Clone, Copy)]
struct DeviceInfo {
    sample_rate: u32,
    channels: u16,
}

// ============ 配置 ============
#[derive(Clone)]
struct Config {
    ai_api_key: String,
    ai_base_url: String,
    ai_model: String,
    doubao_app_id: String,
    doubao_access_token: String,
    doubao_tts_cluster: String,
    doubao_asr_resource_id: String,
    doubao_tts_resource_id: String,
    doubao_voice_type: String,
    doubao_tts_url: String,
    doubao_asr_url: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        let get =
            |k: &str| std::env::var(k).with_context(|| format!("缺少环境变量: {k}"));
        let opt =
            |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.into());
        Ok(Self {
            ai_api_key: get("AI_API_KEY")?,
            ai_base_url: get("AI_BASE_URL")?,
            ai_model: get("AI_MODEL")?,
            doubao_app_id: get("DOUBAO_APP_ID")?,
            doubao_access_token: get("DOUBAO_ACCESS_TOKEN")?,
            doubao_tts_cluster: opt("DOUBAO_TTS_CLUSTER", "volcano_tts"),
            doubao_asr_resource_id: opt("DOUBAO_ASR_RESOURCE_ID", "volc.bigasr.sauc.duration"),
            doubao_tts_resource_id: opt("DOUBAO_TTS_RESOURCE_ID", "volc.service_type.10029"),
            doubao_voice_type: opt("DOUBAO_VOICE_TYPE", "BV700_V2_streaming"),
            doubao_tts_url: opt(
                "DOUBAO_TTS_URL",
                "https://openspeech.bytedance.com/api/v1/tts",
            ),
            doubao_asr_url: opt(
                "DOUBAO_ASR_URL",
                "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async",
            ),
        })
    }
}

// ============ 性能统计 ============
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
            "   {DIM}STT {:.1}s │ LLM首字 {:.1}s · {}tok · {:.0}tok/s │ TTS合成 {:.1}s │ 总计 {:.1}s{RESET}",
            self.stt_ms / 1000.0,
            self.llm_ttft_ms / 1000.0,
            self.llm_tokens,
            self.tok_per_s(),
            self.tts_synth_ms / 1000.0,
            self.e2e_ms() / 1000.0,
        );
    }
}

// ============ 音频工具函数 ============
fn chunk_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|s| s.abs()).sum::<f32>() / samples.len() as f32
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '，' | ',' | '.' | '!' | '?' | '\n')
}

/// 多声道混合为单声道 + 线性插值降采样到 TARGET_RATE
fn downsample_to_mono_16k(input: &[f32], info: DeviceInfo) -> Vec<f32> {
    // 1. 混合为单声道
    let mono: Vec<f32> = if info.channels > 1 {
        input
            .chunks(info.channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / info.channels as f32)
            .collect()
    } else {
        input.to_vec()
    };

    // 2. 重采样
    if info.sample_rate == TARGET_RATE {
        return mono;
    }

    let ratio = info.sample_rate as f64 / TARGET_RATE as f64;
    let out_len = (mono.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;

        let sample = if idx + 1 < mono.len() {
            mono[idx] * (1.0 - frac) + mono[idx + 1] * frac
        } else {
            mono[idx.min(mono.len().saturating_sub(1))]
        };
        output.push(sample);
    }

    output
}

// ============ 获取音频设备配置 ============
fn get_input_device_info() -> Result<(cpal::Device, cpal::StreamConfig, DeviceInfo)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("未检测到麦克风，请连接后重试")?;

    let default_config = device
        .default_input_config()
        .context("无法获取麦克风默认配置")?;

    let info = DeviceInfo {
        sample_rate: default_config.sample_rate().0,
        channels: default_config.channels(),
    };

    // 用设备原生配置
    let config = cpal::StreamConfig {
        channels: info.channels,
        sample_rate: default_config.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };

    Ok((device, config, info))
}

// ============ 噪音校准 ============
fn calibrate_noise() -> Result<(f32, DeviceInfo)> {
    let (device, config, info) = get_input_device_info()?;

    print!(
        "   🔇 校准环境噪音... {DIM}({}Hz {}ch){RESET}",
        info.sample_rate, info.channels
    );
    io::stdout().flush()?;

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
        |err| eprintln!("音频错误: {err}"),
        None,
    )?;
    stream.play()?;
    std::thread::sleep(std::time::Duration::from_secs(2));
    done.store(true, Ordering::Relaxed);
    drop(stream);

    let lvs = levels.lock().unwrap();
    if lvs.is_empty() {
        anyhow::bail!("校准失败：未采集到音频数据");
    }
    let ambient: f32 = lvs.iter().sum::<f32>() / lvs.len() as f32;
    let threshold = (ambient * 4.0).max(0.012);
    println!(" {DIM}噪音:{ambient:.4} 阈值:{threshold:.4}{RESET}");
    Ok((threshold, info))
}

// ============ 录音（原生采样率） ============
fn record_speech(threshold: f32, dev_info: DeviceInfo) -> Result<Option<Vec<f32>>> {
    let (device, config, _) = get_input_device_info()?;

    println!("\n🎤 说话吧...");

    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(200);

    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let _ = tx.try_send(data.to_vec());
        },
        |err| eprintln!("音频错误: {err}"),
        None,
    )?;
    stream.play()?;

    // 计算 silence_chunks：基于原生采样率
    // 每个回调大约 ~512-4096 帧，按经验取 ~1024 帧/回调
    let approx_chunk_frames = 1024_u32;
    let chunks_per_sec =
        (dev_info.sample_rate * dev_info.channels as u32) as f32 / approx_chunk_frames as f32;
    let silence_chunks = (chunks_per_sec * SILENCE_SECONDS) as usize;
    let min_speech_samples =
        (dev_info.sample_rate as f32 * MIN_SPEECH_SECONDS * dev_info.channels as f32) as usize;

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
                    println!("   🟢 检测到语音...");
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
        println!("   {DIM}太短，忽略{RESET}");
        return Ok(Some(Vec::new()));
    }

    if buffer.len() < min_speech_samples {
        return Ok(Some(Vec::new()));
    }

    Ok(Some(buffer))
}

// ============ 编码为 WAV (16kHz mono) ============
fn encode_wav(samples: &[f32]) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
    for &s in samples {
        writer.write_sample((s * 32767.0) as i16)?;
    }
    writer.finalize()?;
    Ok(cursor.into_inner())
}

// ============ GZIP 压缩/解压 ============
fn gzip_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

fn gzip_decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut dec = GzDecoder::new(data);
    let mut out = Vec::new();
    dec.read_to_end(&mut out)?;
    Ok(out)
}

// ============ WebSocket 帧构建 ============
fn build_ws_frame(header: &[u8; 4], raw_data: &[u8]) -> Vec<u8> {
    let compressed = gzip_compress(raw_data).unwrap_or_else(|_| raw_data.to_vec());
    let mut frame = Vec::with_capacity(4 + 4 + compressed.len());
    frame.extend_from_slice(header);
    frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    frame.extend_from_slice(&compressed);
    frame
}

// ============ ASR 响应解析 ============
#[derive(Debug, Default, serde::Deserialize)]
#[allow(dead_code)]
struct AsrWsResponse {
    #[serde(default)]
    code: i32,
    #[serde(default)]
    message: String,
    #[serde(default)]
    result: AsrResultItem,
}

#[derive(Debug, Default, serde::Deserialize)]
struct AsrResultItem {
    #[serde(default)]
    text: String,
}

fn parse_asr_ws(msg: &[u8]) -> Result<AsrWsResponse> {
    if msg.len() < 4 {
        anyhow::bail!("ASR 响应太短 ({}B)", msg.len());
    }
    let header_size = (msg[0] & 0x0f) as usize;
    let message_type = msg[1] >> 4;
    let flags = msg[1] & 0x0f;
    let serialization = msg[2] >> 4;
    let compression = msg[2] & 0x0f;
    let header_bytes = header_size * 4;

    if msg.len() < header_bytes {
        anyhow::bail!("ASR header 不完整");
    }
    let payload = &msg[header_bytes..];
    let has_sequence = (flags & 0x01) != 0;

    let payload_msg: Option<&[u8]> = match message_type {
        // SERVER_FULL_RESPONSE
        0x09 => {
            let mut off = 0_usize;
            // 有序列号标记时，前 4 字节是 sequence，跳过
            if has_sequence {
                off += 4;
            }
            if payload.len() < off + 4 {
                return Ok(AsrWsResponse::default());
            }
            let size = u32::from_be_bytes(payload[off..off + 4].try_into()?) as usize;
            off += 4;
            if size == 0 { None } else { Some(&payload[off..off + size]) }
        }
        // SERVER_ACK
        0x0b => {
            // seq(4) + payload_size(4) + payload_msg
            if payload.len() >= 8 {
                let size = u32::from_be_bytes(payload[4..8].try_into()?) as usize;
                if size == 0 { None } else { Some(&payload[8..8 + size]) }
            } else {
                None
            }
        }
        // SERVER_ERROR_RESPONSE
        0x0f => {
            if payload.len() < 8 {
                anyhow::bail!("error payload 太短");
            }
            let code = i32::from_be_bytes(payload[..4].try_into()?);
            let size = u32::from_be_bytes(payload[4..8].try_into()?) as usize;
            let data = &payload[8..8 + size];
            let decompressed = if compression == 1 {
                gzip_decompress(data)?
            } else {
                data.to_vec()
            };
            anyhow::bail!(
                "ASR 错误 code={}: {}",
                code,
                String::from_utf8_lossy(&decompressed)
            );
        }
        _ => None,
    };

    let Some(payload_msg) = payload_msg else {
        return Ok(AsrWsResponse::default());
    };

    let decompressed = if compression == 1 {
        gzip_decompress(payload_msg)?
    } else {
        payload_msg.to_vec()
    };

    if serialization == 1 && !decompressed.is_empty() {
        debug_log!(
            "   {DIM}[DEBUG] ASR JSON: {}{RESET}",
            String::from_utf8_lossy(&decompressed)
        );
        Ok(serde_json::from_slice(&decompressed)?)
    } else {
        Ok(AsrWsResponse::default())
    }
}

// ============ 豆包 ASR (WebSocket v3 大模型) ============
async fn doubao_asr(
    cfg: &Config,
    raw_audio: &[f32],
    dev_info: DeviceInfo,
) -> Result<(String, f32)> {
    let t0 = Instant::now();

    // 降采样到 16kHz mono + 编码 WAV
    let mono16k = downsample_to_mono_16k(raw_audio, dev_info);
    let wav_data = encode_wav(&mono16k)?;

    let connect_id = Uuid::new_v4().to_string();
    debug_log!(
        "   {DIM}[DEBUG] ASR(WS v3) → {} appid={}{RESET}",
        cfg.doubao_asr_url, cfg.doubao_app_id
    );

    // 建立 WebSocket 连接 (v3 用 X-Api 系列 header 认证)
    let request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(&cfg.doubao_asr_url)
        .header("X-Api-App-Key", &cfg.doubao_app_id)
        .header("X-Api-Access-Key", &cfg.doubao_access_token)
        .header("X-Api-Resource-Id", &cfg.doubao_asr_resource_id)
        .header("X-Api-Connect-Id", &connect_id)
        .header("Host", "openspeech.bytedance.com")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())?;

    let (mut ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .context("ASR WebSocket 连接失败")?;

    // 1. 发送完整客户端请求 (配置帧)
    let req_json = json!({
        "user": { "uid": "remote_rust_chat" },
        "request": {
            "reqid": Uuid::new_v4().to_string(),
            "nbest": 1,
            "model_name": "bigmodel",
            "enable_punc": true,
            "enable_itn": true,
            "result_type": "full",
            "sequence": 1
        },
        "audio": {
            "format": "wav",
            "codec": "raw",
            "rate": TARGET_RATE,
            "bits": 16,
            "channel": 1,
            "language": "zh-CN"
        }
    });

    let frame = build_ws_frame(&WS_FULL_CLIENT, &serde_json::to_vec(&req_json)?);
    ws.send(WsMessage::Binary(frame)).await?;
    debug_log!("   {DIM}[DEBUG] 已发送配置帧{RESET}");

    // 2. 发送所有音频段（全部用 AUDIO_ONLY，不带 LAST 标记）
    for chunk in wav_data.chunks(ASR_SEG_SIZE) {
        let frame = build_ws_frame(&WS_AUDIO_ONLY, chunk);
        ws.send(WsMessage::Binary(frame)).await?;
    }
    debug_log!(
        "   {DIM}[DEBUG] 已发送 {} 段音频 ({}B){RESET}",
        wav_data.chunks(ASR_SEG_SIZE).len(),
        wav_data.len()
    );

    // 3. 发送空的结束帧（koe 模式：空 payload + LAST 标记）
    let finish = build_ws_frame(&WS_LAST_AUDIO, &[]);
    ws.send(WsMessage::Binary(finish)).await?;
    debug_log!("   {DIM}[DEBUG] 已发送结束帧{RESET}");

    // 4. 循环读取响应，直到拿到最终识别结果
    let mut last_text = String::new();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);

    loop {
        let msg = match tokio::time::timeout_at(deadline, ws.next()).await {
            Ok(Some(Ok(msg))) => msg,
            Ok(Some(Err(e))) => {
                debug_log!("   {DIM}[DEBUG] 退出: WS 错误 {e}{RESET}");
                break;
            }
            Ok(None) => {
                debug_log!("   {DIM}[DEBUG] 退出: 连接关闭{RESET}");
                break;
            }
            Err(_) => {
                debug_log!("   {DIM}[DEBUG] 退出: 10s 超时{RESET}");
                break;
            }
        };

        match msg {
            WsMessage::Binary(data) => {
                debug_log!(
                    "   {DIM}[DEBUG] 收到 {}B: type={:#04x} flags={:#04x}{RESET}",
                    data.len(),
                    if data.len() > 1 { data[1] >> 4 } else { 0 },
                    if data.len() > 1 { data[1] & 0x0f } else { 0 },
                );
                match parse_asr_ws(&data) {
                    Ok(resp) => {
                        if !resp.result.text.is_empty() {
                            debug_log!(
                                "   {DIM}[DEBUG] 识别文本: \"{}\"{RESET}",
                                resp.result.text
                            );
                            last_text = resp.result.text.clone();
                        }
                    }
                    Err(e) => {
                        debug_log!("   {DIM}[DEBUG] 解析失败: {e}{RESET}");
                    }
                }
            }
            WsMessage::Close(_) => break,
            _ => {} // skip ping/pong/text
        }
    }

    ws.close(None).await.ok();

    let elapsed = t0.elapsed().as_secs_f32() * 1000.0;
    Ok((last_text.trim().to_string(), elapsed))
}

// ============ 豆包 TTS ============
async fn doubao_tts(
    client: &Client,
    cfg: &Config,
    text: &str,
) -> Result<Option<Vec<u8>>> {
    if text.is_empty() {
        return Ok(None);
    }

    let body = json!({
        "app": {
            "appid": cfg.doubao_app_id,
            "token": cfg.doubao_access_token,
            "cluster": cfg.doubao_tts_cluster
        },
        "user": { "uid": "remote_rust_chat" },
        "audio": {
            "voice_type": cfg.doubao_voice_type,
            "encoding": "mp3",
            "speed_ratio": TTS_SPEED,
            "volume_ratio": 1.0,
            "pitch_ratio": 1.0
        },
        "request": {
            "reqid": Uuid::new_v4().to_string(),
            "text": text,
            "text_type": "plain",
            "operation": "query"
        }
    });

    let resp = client
        .post(&cfg.doubao_tts_url)
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("Bearer;{}", cfg.doubao_access_token),
        )
        .header("X-Api-App-Key", &cfg.doubao_app_id)
        .header("X-Api-Access-Key", &cfg.doubao_access_token)
        .header("X-Api-Resource-Id", &cfg.doubao_tts_resource_id)
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let result: Value = resp.json().await?;

    if !status.is_success() {
        let msg = result
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        eprintln!("   {DIM}TTS 请求失败: HTTP {status} - {msg}{RESET}");
        return Ok(None);
    }

    let code = result.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 3000 {
        let msg = result
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        eprintln!("   {DIM}TTS 响应: code={code} msg={msg}{RESET}");
        return Ok(None);
    }

    let audio_b64 = match result.get("data").and_then(|d| d.as_str()) {
        Some(d) => d,
        None => {
            eprintln!("   {DIM}TTS 响应中缺少 data 字���{RESET}");
            return Ok(None);
        }
    };

    let mp3_bytes = B64.decode(audio_b64)?;
    Ok(Some(mp3_bytes))
}

// ============ LLM 流式 + TTS 流水线 ============
async fn chat_and_speak(
    client: &Client,
    cfg: &Config,
    history: &mut Vec<Value>,
    user_text: &str,
) -> Result<TurnMetrics> {
    history.push(json!({"role": "user", "content": user_text}));
    print!("   🤖 助手: ");
    io::stdout().flush()?;

    let t0 = Instant::now();
    let mut ttft_ms = 0.0_f32;
    let mut reply = String::new();
    let mut tokens = 0_usize;
    let tts_ms = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut pending = String::new();

    // 播放通道 (有序: TTS 结果按句子顺序排队)
    let (audio_tx, audio_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_play = stop.clone();

    // 播放线程
    let play_handle = std::thread::spawn(move || {
        let Ok((_stream, handle)) = OutputStream::try_default() else {
            eprintln!("   无法初始化音频输出");
            return;
        };
        let Ok(sink) = Sink::try_new(&handle) else {
            eprintln!("   无法创建音频播放器");
            return;
        };

        while let Ok(mp3_bytes) = audio_rx.recv() {
            if stop_play.load(Ordering::Relaxed) {
                break;
            }
            let cursor = Cursor::new(mp3_bytes);
            match Decoder::new(cursor) {
                Ok(source) => sink.append(source),
                Err(e) => eprintln!("   {DIM}MP3 解码失败: {e}{RESET}"),
            }
        }
        if !stop_play.load(Ordering::Relaxed) {
            sink.sleep_until_end();
        }
    });

    // LLM 流式请求
    let url = format!(
        "{}/chat/completions",
        cfg.ai_base_url.trim_end_matches('/')
    );
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", cfg.ai_api_key))
        .header("Content-Type", "application/json")
        .json(&json!({
            "model": cfg.ai_model,
            "messages": history,
            "max_tokens": 1000,
            "stream": true
        }))
        .send()
        .await
        .context("LLM 请求失败")?
        .error_for_status()
        .context("LLM 返回错误状态码")?;

    let mut byte_stream = resp.bytes_stream();
    let mut buf = String::new();

    'outer: while let Some(chunk_result) = byte_stream.next().await {
        let chunk = chunk_result?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(newline_pos) = buf.find('\n') {
            let line = buf[..newline_pos].trim().to_string();
            buf = buf[newline_pos + 1..].to_string();

            let data = match line.strip_prefix("data: ") {
                Some(d) => d,
                None => continue,
            };

            if data == "[DONE]" {
                break 'outer;
            }

            let v: Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let content = match v
                .pointer("/choices/0/delta/content")
                .and_then(|c| c.as_str())
            {
                Some(c) if !c.is_empty() => c.to_string(),
                _ => continue,
            };

            if ttft_ms == 0.0 {
                ttft_ms = t0.elapsed().as_secs_f32() * 1000.0;
            }

            reply.push_str(&content);
            tokens += 1;
            pending.push_str(&content);
            print!("{content}");
            io::stdout().flush()?;

            if pending.chars().any(is_sentence_end) && !pending.trim().is_empty()
            {
                let text = pending.trim().to_string();
                pending.clear();
                let tx = audio_tx.clone();
                let c = client.clone();
                let cf = cfg.clone();
                let ms = tts_ms.clone();
                tokio::spawn(async move {
                    let t = Instant::now();
                    if let Ok(Some(mp3)) = doubao_tts(&c, &cf, &text).await {
                        ms.fetch_add(
                            (t.elapsed().as_secs_f32() * 1000.0) as u64,
                            Ordering::Relaxed,
                        );
                        let _ = tx.send(mp3);
                    }
                });
            }
        }
    }

    // 发送剩余文本
    if !pending.trim().is_empty() {
        let text = pending.trim().to_string();
        let t = Instant::now();
        if let Ok(Some(mp3)) = doubao_tts(client, cfg, &text).await {
            tts_ms.fetch_add(
                (t.elapsed().as_secs_f32() * 1000.0) as u64,
                Ordering::Relaxed,
            );
            let _ = audio_tx.send(mp3);
        }
    }

    println!();
    drop(audio_tx);
    let _ = play_handle.join();

    let total_ms = t0.elapsed().as_secs_f32() * 1000.0;

    history.push(json!({"role": "assistant", "content": reply}));

    Ok(TurnMetrics {
        stt_ms: 0.0, // 由调用方填充
        llm_ttft_ms: ttft_ms,
        llm_total_ms: total_ms,
        llm_tokens: tokens,
        tts_synth_ms: tts_ms.load(Ordering::Relaxed) as f32,
    })
}

// ============ 主循环 ============
const RED: &str = "\x1b[31m";

#[tokio::main]
async fn main() -> Result<()> {
    // 解析 --debug 参数
    if std::env::args().any(|a| a == "--debug") {
        DEBUG.store(true, Ordering::Relaxed);
    }

    // 加载 .env
    let script_dir = std::env::var("SCRIPT_DIR").ok();
    if let Some(ref d) = script_dir {
        let env_path = std::path::Path::new(d).join(".env");
        if env_path.exists() {
            dotenvy::from_path(&env_path).ok();
        }
    }
    dotenvy::dotenv().ok();

    let cfg = Config::from_env()?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    println!("\n🚀 启动远程语音助手 (Rust)\n");

    print!("   连接 LLM ({})...", cfg.ai_model);
    io::stdout().flush()?;
    println!(" ✅");

    print!("   豆包语音 API...");
    io::stdout().flush()?;
    println!(
        " ✅ (STT: {}, TTS: {})",
        cfg.doubao_asr_resource_id, cfg.doubao_voice_type
    );

    let (threshold, dev_info) = {
        let mut backoff_secs = 1_u64;
        loop {
            match tokio::task::spawn_blocking(calibrate_noise).await? {
                Ok(v) => break v,
                Err(e) => {
                    eprintln!(
                        "   {RED}❌ 麦克风初始化失败: {e}{RESET}"
                    );
                    eprintln!(
                        "   {DIM}请检查麦克风是否连接并授权，{backoff_secs}秒后重试...{RESET}"
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(60);
                }
            }
        }
    };

    let mut history: Vec<Value> = vec![json!({
        "role": "system",
        "content": "你是语音助手，每次回复不超过两句话，简短口语化，不用 markdown，不用表情符号。"
    })];

    println!("\n==========================================");
    println!("  🤖 远程语音助手已就绪");
    println!("  💡 说话即可对话");
    println!("  ⛔ Ctrl+C 退出");
    println!("==========================================");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
        println!("\n\n👋 下次再聊！");
        std::process::exit(0);
    })?;

    let mut mic_backoff_secs = 0_u64; // 0 = 正常，>0 = 退避中

    loop {
        if !running.load(Ordering::Relaxed) {
            break;
        }

        let audio = match tokio::task::spawn_blocking(move || record_speech(threshold, dev_info)).await? {
            Ok(a) => {
                mic_backoff_secs = 0; // 恢复正常
                a
            }
            Err(e) => {
                mic_backoff_secs = if mic_backoff_secs == 0 { 1 } else { (mic_backoff_secs * 2).min(60) };
                eprintln!(
                    "   {RED}❌ 录音失败: {e}{RESET}"
                );
                eprintln!(
                    "   {DIM}{mic_backoff_secs}秒后重试...{RESET}"
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(mic_backoff_secs)).await;
                continue;
            }
        };

        let min_samples =
            (TARGET_RATE as f32 * MIN_SPEECH_SECONDS) as usize;

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
                eprintln!("   {RED}❌ 麦克风断开，请重新连接后重启程序{RESET}");
                std::process::exit(1);
            }
        };

        // ASR
        let (text, stt_ms) = match doubao_asr(&cfg, &audio, dev_info).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("   {RED}❌ 语音识别失败: {e}{RESET}");
                continue;
            }
        };
        if text.is_empty() || text.len() < 2 {
            continue;
        }
        println!("\n   🗣️ 你: {text}");

        // LLM + TTS 流水线
        let mut metrics = match chat_and_speak(&client, &cfg, &mut history, &text).await {
            Ok(m) => m,
            Err(e) => {
                eprintln!("   {RED}❌ 对话失败: {e}{RESET}");
                continue;
            }
        };
        metrics.stt_ms = stt_ms;
        metrics.log();
    }

    Ok(())
}
