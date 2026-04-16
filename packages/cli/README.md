# cb - Cross-platform Voice Assistant CLI

[English](#english) | [中文](#中文)

---

## English

A cross-platform voice assistant CLI that runs on macOS and Linux. Talk to an AI assistant using your microphone — speech is recognized, sent to an LLM, and the response is spoken back to you.

### Install

```bash
# Homebrew (macOS)
brew install erchoc/tap/chatbox

# Install script
curl https://chatbox.longye.site/install.sh | bash

# Build from source
cd packages/cli && cargo install --path .
```

### Usage

```bash
# Start voice chat (default)
cb

# Interactive config wizard
cb config

# Show current config
cb config show

# Set a config value directly
cb config set llm.model deepseek-chat
cb config set speech.doubao.voice_type BV700_V2_streaming

# Daemon mode (coming soon)
cb up

# Open local web UI (coming soon)
cb open

# Debug mode
cb --debug
```

### Configuration

Config file: `~/.config/chatbox/config.toml`. Environment variables override config file values (compatible with `.env`):

| Env Var | Config Key | Description |
|---------|-----------|-------------|
| AI_API_KEY | llm.api_key | LLM API key |
| AI_BASE_URL | llm.base_url | LLM service URL |
| AI_MODEL | llm.model | Model name |
| DOUBAO_APP_ID | speech.doubao.app_id | Doubao App ID |
| DOUBAO_ACCESS_TOKEN | speech.doubao.access_token | Doubao token |
| DOUBAO_VOICE_TYPE | speech.doubao.voice_type | TTS voice type |

### Architecture

See [DESIGN.md](./DESIGN.md) for the full architecture document.

```
src/
├── main.rs         # CLI entry (clap)
├── cmd/            # Subcommands: chat, config, up, open
├── audio/          # Audio capture, playback, resampling
├── speech/         # Speech service abstraction (trait Asr/Tts) + Doubao impl
├── llm/            # OpenAI-compatible streaming client
├── config/         # TOML config management + env var fallback
└── pipeline/       # Voice chat orchestration (audio→ASR→LLM→TTS→playback)
```

### Extensibility

- **Swappable speech providers**: Implement `Asr` / `Tts` traits to plug in new STT/TTS services (e.g. Whisper, Azure, edge-tts)
- **LLM provider agnostic**: Any OpenAI-compatible API works (DeepSeek, Claude, GPT, etc.)
- **Config priority**: CLI args > env vars > config file > defaults

### Cross-platform Support

| Platform | Audio Backend | Status |
|----------|--------------|--------|
| macOS (Apple Silicon) | CoreAudio | Supported |
| macOS (Intel) | CoreAudio | Supported |
| Linux (x86_64) | ALSA | Supported |
| Linux (aarch64) | ALSA | Supported |

Linux builds require `libasound2-dev`.

### Development

```bash
# Run from project root
pnpm cli              # Run (auto-compile + start)
pnpm cli:debug        # Debug mode
pnpm cli -- config    # Pass subcommands with --

# Run from packages/cli
cd packages/cli
cargo run
cargo run -- --debug

# Build release
pnpm cli:build        # Output: packages/cli/target/release/cb
pnpm cli:install      # Install to ~/.cargo/bin/cb
```

---

## 中文

跨平台语音助手 CLI，支持 macOS 和 Linux。通过麦克风与 AI 助手对话 — 语音识别后发送给 LLM，响应通过语音播放。

### 安装

```bash
# Homebrew (macOS)
brew install erchoc/tap/chatbox

# 安装脚本
curl https://chatbox.longye.site/install.sh | bash

# 从源码编译
cd packages/cli && cargo install --path .
```

### 使用方法

```bash
# 前台语音对话（默认）
cb

# 交互式配置向导
cb config

# 查看当前配置
cb config show

# 直接设置配置项
cb config set llm.model deepseek-chat
cb config set speech.doubao.voice_type BV700_V2_streaming

# 守护进程模式（开发中）
cb up

# 打开本地网页（开发中）
cb open

# 调试模式
cb --debug
```

### 配置

配置文件位于 `~/.config/chatbox/config.toml`，也支持环境变量覆盖（兼容 `.env`）：

| 环境变量 | 配置项 | 说明 |
|----------|--------|------|
| AI_API_KEY | llm.api_key | LLM API Key |
| AI_BASE_URL | llm.base_url | LLM 服务地址 |
| AI_MODEL | llm.model | 模型名称 |
| DOUBAO_APP_ID | speech.doubao.app_id | 豆包 App ID |
| DOUBAO_ACCESS_TOKEN | speech.doubao.access_token | 豆包 Token |
| DOUBAO_VOICE_TYPE | speech.doubao.voice_type | TTS 音色 |

### 架构

详见 [DESIGN.md](./DESIGN.md)。

```
src/
├── main.rs         # CLI 入口 (clap)
├── cmd/            # 子命令：chat, config, up, open
├── audio/          # 音频采集、播放、重采样
├── speech/         # 语音服务抽象 (trait Asr/Tts) + 豆包实现
├── llm/            # OpenAI 兼容流式客户端
├── config/         # TOML 配置管理 + 环境变量 fallback
└── pipeline/       # 语音对话编排 (audio→ASR→LLM→TTS→playback)
```

### 扩展性设计

- **语音提供方可替换**：实现 `Asr` / `Tts` trait 即可接入新的 STT/TTS 服务
- **LLM 已兼容 OpenAI 协议**：DeepSeek、Claude、GPT 等均可使用
- **配置层级**：命令行参数 > 环境变量 > 配置文件 > 默认值

### 跨平台支持

| 平台 | 音频后端 | 状态 |
|------|----------|------|
| macOS (Apple Silicon) | CoreAudio | 已支持 |
| macOS (Intel) | CoreAudio | 已支持 |
| Linux (x86_64) | ALSA | 已支持 |
| Linux (aarch64) | ALSA | 已支持 |

Linux 编译需安装 `libasound2-dev`。

### 开发

```bash
# 根目录运行
pnpm cli              # 运行（自动编译 + 启动）
pnpm cli:debug        # 调试模式
pnpm cli -- config    # 传子命令用 -- 分隔

# cli 目录下运行
cd packages/cli
cargo run
cargo run -- --debug

# 构建发布版
pnpm cli:build        # 产物: packages/cli/target/release/cb
pnpm cli:install      # 安装到 ~/.cargo/bin/cb
```
