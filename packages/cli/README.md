# cb - Cross-platform Voice Assistant CLI

[English](#english) | [中文](#中文)

---

## English

A cross-platform voice assistant CLI that runs on macOS and Linux. Talk to an AI assistant using your microphone — speech is recognized, sent to an LLM, and the response is spoken back to you.

### Install

```bash
# One-line install (macOS & Linux)
curl -fsSL https://chatbot.longye.site/install.sh | bash

# Build from source
cd packages/cli && cargo install --path . --root ~/.local
```

The binary is installed to `~/.local/bin/cb`. Add it to PATH if needed:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc  # or ~/.bashrc
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

# Open local web dashboard
cb open

# Run as background daemon
cb up

# Debug mode
cb --debug
```

### Configuration

Config file: `~/.config/chatbot/config.toml`. Run `cb config` for the interactive wizard.

| Env Var | Config Key | Description |
|---------|-----------|-------------|
| AI_API_KEY | llm.profiles[].api_key | LLM API key |
| AI_BASE_URL | llm.profiles[].base_url | LLM service URL |
| AI_MODEL | llm.profiles[].model | Model name |
| DOUBAO_APP_ID | speech.doubao.app_id | Doubao App ID |
| DOUBAO_ACCESS_TOKEN | speech.doubao.access_token | Doubao token |
| DOUBAO_VOICE_TYPE | speech.doubao.voice_type | TTS voice type |

### Dashboard

Run `cb open` to launch a local web dashboard at `http://localhost:<port>/dashboard`.

- Real-time conversation timeline with STT/LLM/TTS latency metrics
- Per-session event log with error and skip details
- Historical log browsing by date
- Auto-polls every 2.5 seconds; live indicator turns green when active

### Wake Word

Enable in `cb config` → Persona. Once set, the assistant sleeps until it hears the wake word (e.g. `嘿小派`). After activation it stays awake for 5 minutes, renewing on each interaction. Homophone matching (e.g. `黑小派` → `嘿小派`) is supported via pinyin comparison.

### Architecture

See [DESIGN.md](./DESIGN.md) for the full architecture document.

```
src/
├── main.rs         # CLI entry (clap)
├── cmd/            # Subcommands: chat, config, install, open
├── audio/          # Audio capture, playback, resampling
├── speech/         # Speech service abstraction (trait Asr/Tts) + Doubao impl
├── llm/            # OpenAI-compatible streaming client
├── config/         # TOML config + env var fallback + multi-profile LLM
├── pipeline/       # Voice chat orchestration (audio→ASR→LLM→TTS→playback)
├── log/            # Structured event logging (JSONL, per-file rotation)
└── ui/             # Terminal UI: spinner, arrow-key selector, theme
```

### Cross-platform Support

| Platform | Audio Backend | Status |
|----------|--------------|--------|
| macOS (Apple Silicon) | CoreAudio | ✓ |
| macOS (Intel) | CoreAudio | ✓ |
| Linux (x86_64) | ALSA | ✓ |
| Linux (aarch64) | ALSA | ✓ |

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

# Build release binary
cargo build --release
# Binary: target/release/cb
```

---

## 中文

跨平台语音助手 CLI，支持 macOS 和 Linux。通过麦克风与 AI 助手对话 — 语音识别后发送给 LLM，响应通过语音播放。

### 安装

```bash
# 一键安装（macOS & Linux）
curl -fsSL https://chatbot.longye.site/install.sh | bash

# 从源码编译
cd packages/cli && cargo install --path . --root ~/.local
```

二进制安装到 `~/.local/bin/cb`，如未在 PATH 中请添加：

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc  # 或 ~/.bashrc
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

# 打开本地网页控制台
cb open

# 以守护进程运行
cb up

# 调试模式
cb --debug
```

### 配置

配置文件位于 `~/.config/chatbot/config.toml`，运行 `cb config` 进入交互向导。

| 环境变量 | 配置项 | 说明 |
|----------|--------|------|
| AI_API_KEY | llm.profiles[].api_key | LLM API Key |
| AI_BASE_URL | llm.profiles[].base_url | LLM 服务地址 |
| AI_MODEL | llm.profiles[].model | 模型名称 |
| DOUBAO_APP_ID | speech.doubao.app_id | 豆包 App ID |
| DOUBAO_ACCESS_TOKEN | speech.doubao.access_token | 豆包 Token |
| DOUBAO_VOICE_TYPE | speech.doubao.voice_type | TTS 音色 |

### 控制台

运行 `cb open` 在本地启动 `http://localhost:<port>/dashboard`：

- 实时对话时间线，含 STT/LLM/TTS 延迟指标
- 分会话事件日志，含错误和跳过详情
- 历史日志按日期浏览
- 每 2.5 秒自动轮询，活跃时绿色指示灯亮起

### 唤醒词

在 `cb config` → 助手设置中开启。设定后，助手进入待机，听到唤醒词（如 `嘿小派`）才响应，激活后 5 分钟内持续对话，每次交互自动续期。支持同音字识别（如 `黑小派` = `嘿小派`）。

### 架构

详见 [DESIGN.md](./DESIGN.md)。

```
src/
├── main.rs         # CLI 入口 (clap)
├── cmd/            # 子命令：chat, config, install, open
├── audio/          # 音频采集、播放、重采样
├── speech/         # 语音服务抽象 (trait Asr/Tts) + 豆包实现
├── llm/            # OpenAI 兼容流式客户端
├── config/         # TOML 配置 + 环境变量 + 多 LLM Profile
├── pipeline/       # 语音对话编排 (audio→ASR→LLM→TTS→playback)
├── log/            # 结构化事件日志（JSONL 分文件轮转）
└── ui/             # 终端 UI：spinner、箭头选择器、主题色
```

### 跨平台支持

| 平台 | 音频后端 | 状态 |
|------|----------|------|
| macOS (Apple Silicon) | CoreAudio | ✓ |
| macOS (Intel) | CoreAudio | ✓ |
| Linux (x86_64) | ALSA | ✓ |
| Linux (aarch64) | ALSA | ✓ |

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
cargo build --release
# 产物：target/release/cb
```
