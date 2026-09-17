# CB CLI 架构设计

> 跨平台语音助手 CLI，二进制名 `cb`。语音能力统一走豆包（火山引擎 openspeech）2.0，不含任何本地模型。

## 1. 分层总览

依赖方向**只能向下**（上层可以用下层，下层不知道上层存在）。`ui` 是横切的展示工具，除 `domain` 外都可以用。

```text
┌──────────────────────────────────────────────────────────────────┐
│ main.rs / cli.rs         进程入口 · clap 参数契约                  │
├──────────────────────────────────────────────────────────────────┤
│ cmd/                     接口层：子命令处理器（读参数、调下层、打印）  │
├──────────────────────────────────────────────────────────────────┤
│ pipeline/                应用层：语音对话编排                        │
│   voice.rs   主循环（录音 → ASR → 唤醒判定 → 一轮对话 → 记账）        │
│   speak.rs   一轮对话（LLM 流 → 分句 → TTS 并发合成 → 顺序播放）      │
├──────────────────────────────────────────────────────────────────┤
│ domain/                  领域层：纯逻辑，零 IO，全部可单测            │
│   wake.rs      唤醒词状态机 + 匹配（拼音同音）                       │
│   sentence.rs  分句规则 / 最短可合成长度                             │
│   metrics.rs   单轮耗时指标                                         │
│   semver.rs    版本比较（含预发布规则）                               │
│   health.rs    守护进程日志健康判定                                  │
├──────────────────────────────────────────────────────────────────┤
│ 基础设施层（端口 + 适配器）                                          │
│   speech/    trait Asr / Tts + 工厂 build_asr / build_tts           │
│     doubao/  protocol.rs（v3 二进制帧）asr.rs（识别 2.0）tts.rs（合成 2.0）│
│   llm/       OpenAI 兼容流式客户端                                   │
│   audio/     cpal 采集 + VAD、rodio 播放、重采样                     │
│   config/    TOML 存储、旧配置迁移、环境变量覆盖、预设（LLM / 音色）    │
│   storage/   对话历史（JSON）、事件日志（JSONL）                       │
│   platform/  launchd / systemd 原语、桌面通知、版本检查与安装渠道探测   │
├──────────────────────────────────────────────────────────────────┤
│ ui/                      展示层：主题、横幅、spinner、选择器、i18n 文案 │
└──────────────────────────────────────────────────────────────────┘
```

### 各层的规则

| 层 | 允许依赖 | 禁止 | 典型内容 |
|----|---------|------|---------|
| `cmd` | pipeline、所有基础设施、ui | 业务判断（唤醒规则、分句等） | `cb config` 向导、`cb install` 流程、`cb logs` 渲染 |
| `pipeline` | domain、基础设施、ui | 直接 `use` 某个 provider 类型 | 只认 `Box<dyn Asr>` / `Arc<dyn Tts>`，由 `speech::build_*` 提供 |
| `domain` | 标准库、纯计算 crate（`pinyin`） | 网络 / 文件 / 终端 / 系统调用 / `tokio` | 每个函数都能在 `cargo test` 里零依赖跑 |
| 基础设施 | config、domain（仅纯规则） | pipeline、cmd | 与外部世界打交道的全部代码 |
| `ui` | 无 | 业务判断 | 颜色、宽度计算、文案表 |

新增功能时先问："这段代码要不要碰 IO？"要 → 基础设施；不要 → `domain`。编排多个模块 → `pipeline`；只是把结果打给用户 → `cmd`。

## 2. 目录结构

```text
packages/cli/
├── Cargo.toml
├── README.md               用户文档（中英）
├── DESIGN.md               本文件
├── entitlements.plist      macOS 麦克风权限（codesign 用）
├── site/dashboard.html     `cb open` 内嵌的 Web 仪表盘
└── src/
    ├── main.rs             进程初始化 → cmd::dispatch
    ├── cli.rs              clap 结构体（Cli / Commands / ConfigAction）
    ├── cmd/                chat · config · daemon · logs · open · update
    ├── pipeline/           voice · speak
    ├── domain/             wake · sentence · metrics · semver · health
    ├── speech/             mod（trait + 工厂）· asr · tts · doubao/{protocol,asr,tts}
    ├── llm/                openai
    ├── audio/              capture · playback · resample
    ├── config/             store · presets
    ├── storage/            history · events
    ├── platform/           service · notify · update
    └── ui/                 theme · banner · spinner · select · art · i18n
```

## 3. 语音层（豆包 2.0）

| 能力 | 接口 | Resource ID | 说明 |
|------|------|-------------|------|
| ASR | `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async` | `volc.seedasr.sauc.duration`（小时版）/ `...concurrent` | v3 二进制帧；整段 PCM 按 200ms 分片发送，末包打 LAST 标记，开 `enable_nonstream` 拿二遍识别终句 |
| TTS | `https://openspeech.bytedance.com/api/v3/tts/unidirectional` | `seed-tts-2.0`（复刻音色用 `seed-icl-2.0`） | HTTP chunked JSON 行流，`data` 为 base64 mp3；一句一请求，磁盘缓存 `cache/tts/<hash>.mp3` |

- **鉴权**：新版控制台 `X-Api-Key`（`speech.doubao.api_key`）优先；否则旧版 `App ID + Access Token`。
- **语速**：用户面向的是倍率 `tts_speed`（0.5~2.0），发送前线性映射到接口的 `speech_rate`（-50~100）。
- **音色**：仅 2.0 音色（`*_uranus_bigtts`）。1.0 的 `BV*` / `*_mars_bigtts` / `*_moon_bigtts` 在加载配置时自动迁移到最接近的 2.0 音色（`config::presets::migrate_voice`），复刻音色（`S_*`）不动。
- **旧配置**：`tts_cluster`、`/api/v1/tts`、`volc.bigasr.*`、`volc.service_type.10029` 在 `DoubaoConfig::migrate_to_v2` 里静默升级，用户不需要重跑向导。
- **回环验证**：`cargo test --manifest-path packages/cli/Cargo.toml doubao_loopback -- --ignored --nocapture` 用真实凭证跑 TTS 2.0 → 解码 → ASR 2.0，断言识别回原句。

### 扩展点

- 换语音供应商：在 `speech/<provider>/` 实现 `Asr` / `Tts`，在 `speech::build_asr` / `build_tts` 的 `match` 里加一个分支，配置里 `speech.provider = "<provider>"`。pipeline / cmd 一行不改。
- 换 LLM：任何 OpenAI 协议兼容接口，改 `llm_profiles` 即可。

## 4. 配置

优先级从高到低：环境变量（`AI_*`、`DOUBAO_*`）→ `~/.config/chatbot/config.toml` → 内置默认值。

```toml
[persona]
name = "小派"
language = "zh"
[persona.wake_word]
enabled = true
word = "小派小派"

active_llm = "DeepSeek"
[[llm_profiles]]
name = "DeepSeek"
base_url = "https://api.deepseek.com/v1"
model = "deepseek-chat"
api_key = "sk-xxx"

[speech]
provider = "doubao"
[speech.doubao]
api_key = ""                      # 新版控制台 API Key（填了就不需要下面两项）
app_id = "xxx"
access_token = "xxx"
asr_resource_id = "volc.seedasr.sauc.duration"
tts_resource_id = "seed-tts-2.0"
voice_type = "zh_female_vv_uranus_bigtts"
tts_speed = 1.3

[audio]
silence_seconds = 1.0
min_speech_seconds = 1.0
```

所有路径统一 `~/.config/chatbot/`（不用 `dirs::config_dir()`，见 CLAUDE.md 错误记录 #11）。任何配置写入若守护进程在跑，都会触发重启（`cmd::config::save_and_reload`）。

## 5. 一轮对话的数据流

```text
audio::capture ──raw PCM──▶ audio::resample ──16k mono WAV──▶ speech::Asr
                                                                   │ text
                                                       domain::wake::decide
                                                                   │ Proceed / JustWoke
                                                          pipeline::speak::speak_turn
                          ┌────────────────────────────────────────┤
                          │ llm::OpenAiClient (SSE token 流)        │
                          │   └▶ domain::sentence::SentenceBuffer   │ 句
                          │         └▶ speech::Tts (并发 ≤3, 有序)   │ mp3
                          │               └▶ audio::playback (顺序) │
                          └────────────────────────────────────────┘
                                                                   │ reply + 指标
                                             storage::{history, events} 落盘
```

## 6. 跨平台与发布

| 组件 | macOS | Linux |
|------|-------|-------|
| 采集 / 播放 | cpal / rodio (CoreAudio) | cpal / rodio (ALSA)，编译需 `libasound2-dev` |
| 守护进程 | launchd (`~/Library/LaunchAgents/com.erchoc.chatbot.plist`) | systemd user (`chatbot.service`) |
| 安装渠道 | curl `install.sh` / brew tap / npm / 直接下载 | 同左 |

- CI：macOS universal（lipo arm64 + x86_64）+ Linux x86_64 + aarch64，见 `.github/workflows/release.yml`。
- 安装脚本：`curl -fsSL https://chatbot.longye.dev/install.sh | bash`。
- 升级：`cb update` 只服务 curl / 直接下载用户；brew / npm 用户会被引导到各自的包管理器（`platform::update::detect_channel`）。

## 7. 依赖选型

| Crate | 用途 | 理由 |
|-------|------|------|
| clap (derive) | 命令行解析 | 生态标准 |
| tokio (full) | 异步运行时 | 最成熟 |
| cpal / rodio | 采集 / 播放 | 唯一成熟的跨平台音频栈 |
| reqwest (rustls, stream) | HTTP（LLM SSE、TTS chunked、更新下载） | 异步 + 流式 |
| tokio-tungstenite | WebSocket（ASR） | v3 协议要求 |
| serde + toml + serde_json | 配置与协议 | TOML 便于手改 |
| flate2 | gzip | ASR 帧协议要求 |
| pinyin | 唤醒词同音匹配 | 纯计算，domain 可用 |
| crossterm | 方向键选择器 | raw mode |
| anyhow | 错误 | 应用层标准 |
