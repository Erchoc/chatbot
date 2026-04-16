"""
语音对话机器人
麦克风 → Whisper(STT) → LLM(流式) → edge-tts(TTS) → 扬声器

流水线：LLM 每完成一句就立即合成并播放，无需等全部生成完。
"""

import sys
import time
import argparse
import re
import queue
import io
import wave
import asyncio
import subprocess

_BOOT_TIME = time.time()
print(f"[{time.strftime('%H:%M:%S')}] Python 启动，开始加载依赖...", flush=True)

import os
import warnings
warnings.filterwarnings("ignore", category=UserWarning)
warnings.filterwarnings("ignore", category=FutureWarning)

import pyaudio
import numpy as np
import sounddevice as sd
import threading
from dataclasses import dataclass
from faster_whisper import WhisperModel
from zhconv import convert
from openai import OpenAI
import edge_tts
from dotenv import load_dotenv

load_dotenv()

# ============ 前置检查 ============
def die(msg: str) -> None:
    print(f"\n❌ {msg}", file=sys.stderr)
    sys.exit(1)

def preflight():
    missing = [v for v in ("AI_API_KEY", "AI_BASE_URL", "AI_MODEL") if not os.environ.get(v)]
    if missing:
        die(f"缺少环境变量: {', '.join(missing)}\n   请复制 .env.example 为 .env 并填写后重试")

    p = pyaudio.PyAudio()
    try:
        p.get_default_input_device_info()
    except OSError:
        p.terminate()
        die("未检测到麦克风，请连接后重试")
    p.terminate()

preflight()

# ============ 配置 ============
AI_API_KEY  = os.environ["AI_API_KEY"]
AI_BASE_URL = os.environ["AI_BASE_URL"]
AI_MODEL    = os.environ["AI_MODEL"]

WHISPER_MODEL        = "tiny"
SILENCE_SECONDS      = 0.8
MIN_SPEECH_SECONDS   = 0.6
MIN_LOUD_CHUNKS      = 8
SPEECH_START_CHUNKS  = 5
INTERRUPT_LOUD_COUNT = 8       # consecutive loud chunks to trigger interrupt
INTERRUPT_THRESHOLD_MULT = 3.0 # interrupt threshold = speech threshold * this
RATE                 = 16000
CHUNK                = 1024
TTS_SPEED            = 1.3
TTS_VOICE            = "zh-CN-XiaoxiaoNeural"
TTS_SAMPLE_RATE      = 24000

def _tts_rate() -> str:
    """将 TTS_SPEED 映射为 edge-tts 的 rate 参数（如 +30%）"""
    pct = int((TTS_SPEED - 1.0) * 100)
    return f"+{pct}%" if pct >= 0 else f"{pct}%"

_SENT_END = re.compile(r"[。！？\n]")
_DONE     = object()  # producer/consumer 队列终止哨兵

HALLUCINATIONS = {
    "字幕", "订阅", "感谢观看", "谢谢观看", "感谢收看",
    "请不吝点赞", "关注", "下期再见", "点赞", "充值",
    "Subscribe", "Thank you", "Thanks for watching",
    "字幕by", "字幕制作", "欢迎收看", "再见", "...", "…",
}

DIM   = "\033[90m"
RESET = "\033[0m"
DEBUG = False

def debug_log(msg: str) -> None:
    if DEBUG:
        print(f"   [DEBUG] {msg}", flush=True)

def chunk_volume(data: bytes) -> float:
    return np.abs(np.frombuffer(data, dtype=np.int16)).mean()

# ============ 性能统计 ============
@dataclass
class TurnMetrics:
    stt_ms:       float = 0
    llm_ttft_ms:  float = 0
    llm_total_ms: float = 0   # 从 LLM 开始到最后一帧音频播完的总耗时
    llm_tokens:   int   = 0
    tts_synth_ms: float = 0   # TTS 合成计算时间（不含播放，因为与 LLM 并行）

    @property
    def e2e_ms(self) -> float:
        return self.stt_ms + self.llm_total_ms

    @property
    def tok_per_s(self) -> float:
        return self.llm_tokens / (self.llm_total_ms / 1000) if self.llm_total_ms > 0 else 0

    def log(self) -> None:
        print(
            f"   {DIM}"
            f"STT {self.stt_ms/1000:.1f}s │ "
            f"LLM首字 {self.llm_ttft_ms/1000:.1f}s · {self.llm_tokens}tok · {self.tok_per_s:.0f}tok/s │ "
            f"TTS合成 {self.tts_synth_ms/1000:.1f}s │ "
            f"总计 {self.e2e_ms/1000:.1f}s"
            f"{RESET}"
        )

# ============ 全局状态 ============
is_speaking   = False
stop_speaking = False

# ============ 设备列表 ============
def list_audio_devices() -> None:
    p = pyaudio.PyAudio()
    print("\n可用麦克风设备：")
    default_idx = p.get_default_input_device_info()["index"]
    for i in range(p.get_device_count()):
        info = p.get_device_info_by_index(i)
        if info["maxInputChannels"] > 0:
            marker = "  ← 默认" if i == default_idx else ""
            print(f"  [{i}] {info['name']}{marker}")
    p.terminate()

# ============ 初始化 ============
def init():
    print(f"\n🚀 启动语音助手  {DIM}(依赖加载耗时 {time.time() - _BOOT_TIME:.1f}s){RESET}\n")

    print("   加载 Whisper STT...", end="", flush=True)
    stt = WhisperModel(WHISPER_MODEL, compute_type="float32", local_files_only=True)
    print(" ✅")

    print("   连接 LLM...", end="", flush=True)
    llm = OpenAI(api_key=AI_API_KEY, base_url=AI_BASE_URL)
    print(" ✅")

    return stt, llm

# ============ edge-tts 音频合成 ============
async def _edge_tts_async(text: str) -> bytes:
    communicate = edge_tts.Communicate(text, voice=TTS_VOICE, rate=_tts_rate())
    chunks = []
    async for chunk in communicate.stream():
        if chunk["type"] == "audio":
            chunks.append(chunk["data"])
    return b"".join(chunks)

def edge_tts_to_audio(text: str) -> np.ndarray | None:
    """将文本转为 24kHz mono float32 numpy 数组"""
    try:
        mp3_data = asyncio.run(_edge_tts_async(text))
        if not mp3_data:
            return None
        # ffmpeg: mp3 -> wav 24kHz mono s16
        proc = subprocess.run(
            ["ffmpeg", "-hide_banner", "-loglevel", "error",
             "-i", "pipe:0", "-ar", str(TTS_SAMPLE_RATE), "-ac", "1",
             "-f", "wav", "pipe:1"],
            input=mp3_data, capture_output=True
        )
        wav_bytes = proc.stdout
        with wave.open(io.BytesIO(wav_bytes), "rb") as wf:
            frames = wf.readframes(wf.getnframes())
            audio = np.frombuffer(frames, dtype=np.int16).astype(np.float32) / 32768.0
        return audio
    except Exception:
        return None

# ============ 噪音校准 ============
def calibrate_noise(device: int | None) -> int:
    p = pyaudio.PyAudio()
    stream = p.open(format=pyaudio.paInt16, channels=1, rate=RATE,
                    input=True, input_device_index=device, frames_per_buffer=CHUNK)

    print("   🔇 校准环境噪音...", end="", flush=True)

    levels = []
    for _ in range(int(RATE / CHUNK * 2)):
        data = stream.read(CHUNK, exception_on_overflow=False)
        levels.append(chunk_volume(data))

    stream.stop_stream()
    stream.close()
    p.terminate()

    ambient = np.mean(levels)
    threshold = max(int(ambient * 3), 300)
    print(f" {DIM}噪音:{ambient:.0f} 阈值:{threshold}{RESET}")
    return threshold

# ============ 语音打断监听 ============
def listen_for_interrupt(threshold: int, device: int | None) -> None:
    """Monitor mic during playback. Only interrupt on genuine speech, not speaker echo."""
    global stop_speaking

    # Use a much higher threshold to avoid speaker-echo self-interruption
    interrupt_threshold = int(threshold * INTERRUPT_THRESHOLD_MULT)

    p = pyaudio.PyAudio()
    try:
        stream = p.open(format=pyaudio.paInt16, channels=1, rate=RATE,
                        input=True, input_device_index=device, frames_per_buffer=CHUNK)
    except OSError:
        p.terminate()
        return

    # Skip the first ~0.3s of playback to avoid initial burst
    warmup_chunks = int(RATE / CHUNK * 0.3)
    warmup = 0

    loud_count = 0
    while is_speaking:
        try:
            data = stream.read(CHUNK, exception_on_overflow=False)

            warmup += 1
            if warmup < warmup_chunks:
                continue

            volume = chunk_volume(data)
            if volume > interrupt_threshold:
                loud_count += 1
                if loud_count >= INTERRUPT_LOUD_COUNT:
                    stop_speaking = True
                    sd.stop()
                    break
            else:
                loud_count = 0
        except OSError:
            break

    stream.stop_stream()
    stream.close()
    p.terminate()

# ============ 录音 ============
def record_speech(threshold: int, device: int | None) -> np.ndarray | None:
    p = pyaudio.PyAudio()
    try:
        stream = p.open(format=pyaudio.paInt16, channels=1, rate=RATE,
                        input=True, input_device_index=device, frames_per_buffer=CHUNK)
    except OSError:
        p.terminate()
        return None

    print("\n🎤 说话吧...")
    buffer, pre_buffer = [], []
    silent_count = loud_count = pre_loud = 0
    silence_chunks = int(RATE / CHUNK * SILENCE_SECONDS)
    started = False

    while True:
        data = stream.read(CHUNK, exception_on_overflow=False)
        volume = chunk_volume(data)

        if not started:
            if volume > threshold:
                pre_loud += 1
                pre_buffer.append(data)
                if pre_loud >= SPEECH_START_CHUNKS:
                    started = True
                    buffer.extend(pre_buffer)
                    loud_count = pre_loud
                    pre_buffer.clear()
                    print("   🟢 检测到语音...")
            else:
                pre_loud = 0
                pre_buffer.append(data)
                if len(pre_buffer) > SPEECH_START_CHUNKS:
                    pre_buffer.pop(0)
        else:
            if volume > threshold:
                buffer.append(data)
                silent_count = 0
                loud_count += 1
            else:
                silent_count += 1
                buffer.append(data)
                if silent_count >= silence_chunks:
                    break

    stream.stop_stream()
    stream.close()
    p.terminate()

    if loud_count < MIN_LOUD_CHUNKS:
        print(f"   {DIM}太短，忽略{RESET}")
        return np.array([], dtype=np.float32)

    return np.frombuffer(b"".join(buffer), dtype=np.int16).astype(np.float32) / 32768.0

# ============ STT ============
def speech_to_text(stt, audio: np.ndarray) -> tuple[str, float]:
    t0 = time.time()
    segments, _ = stt.transcribe(
        audio, language="zh",
        initial_prompt="以下是普通话的句子。",
        beam_size=1,
        best_of=1,
        vad_filter=True,
        vad_parameters=dict(min_silence_duration_ms=300, speech_pad_ms=200),
        condition_on_previous_text=False,
        no_speech_threshold=0.5,
        log_prob_threshold=-0.8,
    )
    text = "".join(seg.text for seg in segments).strip()
    text = convert(text, "zh-cn")
    elapsed_ms = (time.time() - t0) * 1000

    stripped = text.rstrip("。，！？,.!?")
    if stripped in HALLUCINATIONS:
        return "", elapsed_ms
    if len(stripped) < 8 and any(h in stripped for h in HALLUCINATIONS):
        return "", elapsed_ms
    if not any(c.isalnum() for c in stripped):
        return "", elapsed_ms

    return text, elapsed_ms

# ============ LLM 流式 + TTS 流水线 ============
def chat_and_speak(
    llm, history: list, user_text: str,
    threshold: int, device: int | None,
) -> tuple[float, float, int, float]:
    """返回 (llm_ttft_ms, pipeline_total_ms, token数, tts_synth_ms)"""
    global is_speaking, stop_speaking

    history.append({"role": "user", "content": user_text})
    print("   🤖 助手: ", end="", flush=True)

    t0 = time.time()
    state = {"ttft_ms": 0.0, "reply": "", "tokens": 0, "tts_ms": 0.0}
    audio_q: queue.Queue = queue.Queue()

    is_speaking = True
    stop_speaking = False
    interrupt_thread = threading.Thread(
        target=listen_for_interrupt, args=(threshold, device), daemon=True
    )
    interrupt_thread.start()

    def _synth_and_enqueue(text: str) -> None:
        t = time.time()
        audio = edge_tts_to_audio(text)
        state["tts_ms"] += (time.time() - t) * 1000
        debug_log(f"TTS done, audio length: {len(audio) if audio is not None else 0}")
        if audio is not None and not stop_speaking:
            audio_q.put(audio)

    def producer() -> None:
        pending = ""
        try:
            stream = llm.chat.completions.create(
                model=AI_MODEL, messages=history, max_tokens=1000, stream=True,
            )
            for chunk in stream:
                if stop_speaking:
                    break
                token = chunk.choices[0].delta.content or ""
                if not token:
                    continue
                if not state["ttft_ms"]:
                    state["ttft_ms"] = (time.time() - t0) * 1000
                state["reply"] += token
                state["tokens"] += 1
                pending += token
                print(token, end="", flush=True)

                if _SENT_END.search(pending) and pending.strip():
                    _synth_and_enqueue(pending.strip())
                    pending = ""

            if pending.strip() and not stop_speaking:
                _synth_and_enqueue(pending.strip())
        finally:
            print()
            audio_q.put(_DONE)

    def consumer() -> None:
        while True:
            try:
                item = audio_q.get(timeout=0.2)
            except queue.Empty:
                if stop_speaking:
                    break
                continue
            if item is _DONE:
                break
            if not stop_speaking:
                try:
                    debug_log("Playing audio...")
                    sd.play(item, TTS_SAMPLE_RATE)
                    sd.wait()
                    debug_log("Audio playback done")
                except Exception as e:
                    debug_log(f"Playback error: {e}")

    prod_t = threading.Thread(target=producer, daemon=True)
    cons_t = threading.Thread(target=consumer, daemon=True)
    prod_t.start()
    cons_t.start()
    prod_t.join()
    cons_t.join()

    total_ms = (time.time() - t0) * 1000

    if stop_speaking:
        print("   ⏹ 被打断")

    is_speaking = False
    interrupt_thread.join(timeout=0.5)

    history.append({"role": "assistant", "content": state["reply"]})
    return state["ttft_ms"], total_ms, state["tokens"], state["tts_ms"]

# ============ 主循环 ============
def main():
    parser = argparse.ArgumentParser(description="语音对话机器人")
    parser.add_argument("--list-devices", action="store_true", help="列出麦克风设备后退出")
    parser.add_argument("--device", type=int, default=None, metavar="N", help="指定麦克风设备编号")
    parser.add_argument("--debug", action="store_true", help="启用调试日志")
    args = parser.parse_args()

    global DEBUG
    DEBUG = args.debug

    if args.list_devices:
        list_audio_devices()
        return

    stt, llm = init()
    threshold = calibrate_noise(args.device)

    history = [
        {"role": "system", "content": "你是语音助手，每次回复不超过两句话，简短口语化，不用 markdown，不用表情符号。"}
    ]

    print("\n" + "=" * 42)
    print("  🤖 语音助手已就绪")
    print("  💡 说话即可对话，对话中说话可打断")
    print("  ⛔ Ctrl+C 退出")
    print("=" * 42)

    try:
        while True:
            audio = record_speech(threshold, args.device)

            if audio is None:
                die("麦克风断开，请重新连接后重启程序")

            if len(audio) < RATE * MIN_SPEECH_SECONDS:
                continue

            metrics = TurnMetrics()

            text, metrics.stt_ms = speech_to_text(stt, audio)
            if not text or len(text) < 2:
                continue

            print(f"\n   🗣️ 你: {text}")

            metrics.llm_ttft_ms, metrics.llm_total_ms, metrics.llm_tokens, metrics.tts_synth_ms = \
                chat_and_speak(llm, history, text, threshold, args.device)

            if not stop_speaking:
                metrics.log()

    except KeyboardInterrupt:
        sd.stop()
        print("\n\n👋 下次再聊！")
        sys.exit(0)

if __name__ == "__main__":
    main()
