<h1 align="center">
  <br>
  cb
  <br>
</h1>

<p align="center">
  <strong>Cross-platform voice assistant that lives in your terminal.</strong>
</p>

<p align="center">
  Speak naturally, get answers instantly — powered by any LLM.
</p>

<p align="center">
  <a href="https://github.com/Erchoc/chatbot/releases/latest"><img src="https://img.shields.io/github/v/release/Erchoc/chatbot?style=flat-square&label=latest" alt="Latest Release"></a>
  <a href="https://github.com/Erchoc/chatbot/actions"><img src="https://img.shields.io/github/actions/workflow/status/Erchoc/chatbot/ci.yml?style=flat-square&label=CI" alt="CI"></a>
  <a href="https://github.com/Erchoc/chatbot/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License"></a>
</p>

---

## What is cb?

`cb` is a voice-first AI assistant for the terminal. Talk to it like a person — it listens, thinks, and speaks back. Works with any OpenAI-compatible LLM (DeepSeek, OpenAI, Claude, Ollama, etc).

**Features:**

- Real-time voice conversation with wake word detection
- Pinyin-aware wake word matching (handles homophones)
- Multi-LLM profile management — switch models on the fly
- Background daemon mode — always listening, always ready
- Sentence-level TTS streaming for natural response cadence
- Smart VAD that ignores keyboard noise and fan hum
- Self-updating binary with automatic daemon restart
- Web dashboard for conversation history
- Works on macOS (Universal) and Linux (x86_64 / arm64)

---

## Install

### Shell script (macOS / Linux)

```bash
curl -fsSL https://chatbot.longye.dev/install.sh | bash
```

Set `GITHUB_TOKEN` beforehand if you're hitting anonymous API rate limits (60 req/hr). Pin a version with `CB_VERSION=v0.1.0-beta.5 curl ... | bash`.

### Homebrew (macOS / Linux)

```bash
brew install erchoc/tap/cb
```

### npm

```bash
npm install -g @erchoc/chatbot
```

Binary is still `cb` — the npm package is named for registry discovery.

### Direct download

Grab the binary for your platform from [Releases](https://github.com/Erchoc/chatbot/releases/latest), `chmod +x`, and move it to your PATH.

---

## Quick Start

```bash
# First run — interactive setup wizard
cb config

# Start talking
cb

# Or send a text message
cb chat "what time is it in Tokyo?"
```

### Configuration

The setup wizard walks you through:

1. **Language** — Chinese or English
2. **Assistant name** — your assistant's identity
3. **Wake word** — customizable trigger phrase (default: "嘿小派")
4. **LLM provider** — DeepSeek, OpenAI, Ollama, or any OpenAI-compatible API
5. **Speech provider** — Doubao 2.0 (ByteDance) for ASR + TTS, via API Key or App ID + Token
6. **Voice** — choose from preset voices with live preview

Config lives at `~/.config/chatbot/config.toml`. Edit directly or use:

```bash
cb config set persona.name "Jarvis"
cb config set persona.wake_word.word "Hey Jarvis"
cb config show
```

---

## Background Daemon

Run `cb` as a system service that's always listening:

```bash
cb install      # Register and start (launchd / systemd)
cb status       # Check if running
cb logs -f      # Follow live logs
cb uninstall    # Stop and remove
```

On macOS, `cb install` will request microphone permission before registering the service.

---

## Update

```bash
cb update
```

Downloads the latest release and replaces the binary in place. If the daemon is running, it's automatically restarted with the new version.

---

## Commands

| Command | Description |
|---------|-------------|
| `cb` | Start voice conversation |
| `cb chat <message>` | Send text, get voice response |
| `cb config` | Interactive setup wizard |
| `cb config show` | Show current configuration |
| `cb config set <key> <value>` | Set a config value |
| `cb install` | Install as background daemon |
| `cb uninstall` | Remove background daemon |
| `cb status` | Show daemon status |
| `cb update` | Update to latest version |
| `cb logs` | View conversation logs |
| `cb logs -f` | Follow logs in real-time |
| `cb open` | Open web dashboard |

---

## Architecture

```
packages/
  cli/        Rust — voice assistant binary (cb)
  web/        Vite — landing page + install.sh (Cloudflare Workers static assets → chatbot.longye.dev)
  server/     Bun + Fastify — API server (no deploy target yet)
```

### Voice Pipeline

```
Microphone → VAD → ASR (Doubao) → Wake Word Check → LLM (streaming)
                                                        ↓
                                              TTS (sentence batching)
                                                        ↓
                                                    Speaker Queue
```

### Supported Providers

| Category | Providers |
|----------|-----------|
| LLM | DeepSeek, OpenAI, Claude, Ollama, any OpenAI-compatible API |
| ASR | 豆包流式语音识别模型 2.0 — Doubao Seed-ASR (ByteDance) |
| TTS | 豆包语音合成模型 2.0 — Doubao Seed-TTS (ByteDance) |

---

## Development

```bash
# Prerequisites: Rust, Bun (https://bun.sh)

# CLI
cargo run --manifest-path packages/cli/Cargo.toml
cargo run --manifest-path packages/cli/Cargo.toml -- --debug

# Web + Server
bun install
bun run dev         # server :7758 + web :3000

# Quality checks
bun run verify      # lint + typecheck + test + build

# Deploy landing page to Cloudflare (run `bunx wrangler login` once)
bun run deploy:web
```

### Release

Push a version tag to build and publish:

```bash
git tag v0.1.0-beta.5
git push origin main v0.1.0-beta.5
```

`.github/workflows/release.yml` then:

1. Builds macOS Universal (arm64 + x86_64), Linux x86_64, Linux aarch64
2. Creates a GitHub Release with those artifacts (pre-release is auto-flagged for any tag containing `-`)
3. Chains `publish-npm` — publishes `@erchoc/chatbot@<version>` to npm

**What's NOT automatic**: the Homebrew Formula at [`Erchoc/homebrew-tap/Formula/cb.rb`](https://github.com/Erchoc/homebrew-tap/blob/master/Formula/cb.rb) must be updated manually after each release (bump `version` + 3 sha256 values from the new artifacts).

**Plain commits to `main`** trigger CI only. They do *not* trigger releases, and they do *not* deploy the web site — run `bun run deploy:web` to publish the landing page (including the `install.sh` endpoint) to Cloudflare at `chatbot.longye.dev`.

### Verifying install channels

`scripts/verify-install-channels.sh` runs the same smoke test across all three install channels (curl / npm / brew) to confirm a release is healthy end-to-end. It backs up `~/.config/chatbot/` before running and restores afterward — safe to run any time.

```bash
# Test whatever the GitHub "latest" release resolves to
export GITHUB_TOKEN=$(gh auth token)   # avoids anonymous rate limit
./scripts/verify-install-channels.sh

# Lock the expected version (fails if any channel returns something else)
EXPECTED_VERSION=v0.1.0-beta.5 ./scripts/verify-install-channels.sh

# Narrow to one channel
SKIP_NPM=1 SKIP_BREW=1 ./scripts/verify-install-channels.sh
```

For each channel the script installs, checks `--version`, `--help` (Usage line), `cb config show` (config reachability), uninstalls, then emits a summary table plus any anomalies. The script is non-interactive — it never invokes `cb`, `cb chat`, `cb config` (wizard), or `cb install` (daemon).

---

## License

MIT
