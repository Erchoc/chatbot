# CB CLI Architecture

> Cross-platform voice assistant CLI. Binary name: `cb`. Install via `brew install erchoc/tap/chatbot`.

## Directory Structure

```
packages/cli/
├── Cargo.toml              # Package config, binary name = "cb"
├── README.md               # User docs (bilingual EN/CN)
├── DESIGN.md               # This file
├── src/
│   ├── main.rs             # Entry: clap parsing -> command routing
│   ├── cmd/                # Subcommand handlers
│   │   ├── mod.rs
│   │   ├── chat.rs         # `cb` default: foreground voice chat
│   │   ├── up.rs           # `cb up` daemon mode (placeholder)
│   │   ├── open.rs         # `cb open` open web UI (placeholder)
│   │   └── config.rs       # `cb config` interactive config wizard
│   ├── audio/              # Audio I/O abstraction
│   │   ├── mod.rs          # Public interface
│   │   ├── capture.rs      # Mic capture + VAD speech detection
│   │   ├── playback.rs     # Speaker playback (MP3 decode + queue)
│   │   └── resample.rs     # Multi-channel mix + downsampling
│   ├── speech/             # Speech service abstraction
│   │   ├── mod.rs          # Trait definitions + provider registry
│   │   ├── asr.rs          # trait Asr
│   │   ├── tts.rs          # trait Tts
│   │   └── doubao/         # Doubao implementation
│   │       ├── mod.rs
│   │       ├── asr.rs      # Doubao ASR WebSocket v3
│   │       └── tts.rs      # Doubao TTS REST
│   ├── llm/                # LLM client
│   │   ├── mod.rs
│   │   └── openai.rs       # OpenAI-compatible API (streaming SSE)
│   ├── config/             # Config management
│   │   ├── mod.rs
│   │   └── store.rs        # TOML file read/write + env var fallback
│   └── pipeline/           # Voice chat orchestration
│       ├── mod.rs
│       └── voice.rs        # audio -> ASR -> LLM -> TTS -> playback
```

## Core Design

### 1. Trait Abstraction (Key Extensibility Point)

```rust
// speech/asr.rs
#[async_trait]
pub trait Asr: Send + Sync {
    /// Audio data -> text, returns (text, latency_ms)
    async fn recognize(&self, wav_data: &[u8]) -> Result<(String, f32)>;
}

// speech/tts.rs
#[async_trait]
pub trait Tts: Send + Sync {
    /// Text -> MP3 audio bytes
    async fn synthesize(&self, text: &str) -> Result<Option<Vec<u8>>>;
}
```

**Extension scenarios**:
- Swap ASR provider: implement `Asr` trait (e.g. Whisper, Google STT)
- Swap TTS provider: implement `Tts` trait (e.g. edge-tts, Azure)
- Swap LLM provider: already compatible with any OpenAI-protocol API

### 2. Config Hierarchy

Priority (highest to lowest):
1. CLI arguments (`--model`, `--voice`, etc.)
2. Environment variables (`AI_API_KEY`, etc. — compatible with existing `.env`)
3. Config file `~/.config/chatbot/config.toml`
4. Built-in defaults

```toml
# ~/.config/chatbot/config.toml
[llm]
api_key = "sk-xxx"
base_url = "https://api.deepseek.com"
model = "deepseek-chat"

[speech]
provider = "doubao"           # Future: "whisper", "azure"

[speech.doubao]
app_id = "xxx"
access_token = "xxx"
voice_type = "BV700_V2_streaming"
tts_speed = 1.3

[audio]
silence_seconds = 1.0
min_speech_seconds = 1.0
```

### 3. Cross-platform Strategy

| Component | macOS | Linux |
|-----------|-------|-------|
| Audio capture | cpal (CoreAudio) | cpal (ALSA/PulseAudio) |
| Audio playback | rodio (CoreAudio) | rodio (ALSA/PulseAudio) |
| Config path | `~/.config/chatbot/` | `~/.config/chatbot/` |
| Install method | brew tap | curl install.sh / cargo install |

Linux builds require `libasound2-dev` (ALSA), handled in CI.

### 4. Command Design

```
cb                  # Default: foreground voice chat (interactive)
cb chat             # Same as above, explicit subcommand
cb up               # Daemon mode (future)
cb open             # Open local web UI (future)
cb config           # Interactive config wizard
cb config show      # Show current config
cb config set <k> <v>  # Set config value directly
cb --version        # Version
cb --debug          # Enable debug logging
```

### 5. Pipeline Orchestration

```
┌─────────────────┐
│  audio::capture  │──> raw PCM (native rate)
└────────┬────────┘
         │ resample to 16kHz mono
         ▼
┌─────────────────┐
│  speech::asr    │──> text
└────────┬────────┘
         ▼
┌─────────────────┐
│  llm::stream    │──> token stream (SSE)
└────────┬────────┘
         │ sentence batching
         ▼
┌─────────────────┐
│  speech::tts    │──> MP3 bytes (concurrent per sentence)
└────────┬────────┘
         ▼
┌─────────────────┐
│  audio::playback│──> speaker (sequential)
└─────────────────┘
```

### 6. Release Pipeline

- GitHub Actions CI: macOS (aarch64 + x86_64) + Linux (x86_64 + aarch64)
- Release artifacts: `cb-{os}-{arch}` binary
- Homebrew tap: `erchoc/tap/chatbot` formula pulls release binary
- Install script: `curl https://chatbot.longye.site/install.sh | bash`

### 7. Dependency Choices

| Crate | Purpose | Rationale |
|-------|---------|-----------|
| clap (derive) | CLI parsing | Rust ecosystem standard, derive macros are concise |
| tokio (full) | Async runtime | Most mature ecosystem |
| cpal | Audio capture | Only mature cross-platform audio input library |
| rodio | Audio playback | Built on cpal, friendly API |
| reqwest (stream) | HTTP client | Async + streaming support |
| tokio-tungstenite | WebSocket | Required for ASR |
| serde + toml | Config serialization | TOML is user-editable |
| dirs | Cross-platform paths | `~/.config` resolution |
| async-trait | Async traits | Required for speech traits |
| hound | WAV encoding | Lightweight |
| flate2 | GZIP | Required by ASR protocol |
| anyhow | Error handling | Application-layer standard |
