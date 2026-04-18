# CB - Cross-platform Voice Assistant CLI

## Project Overview

Monorepo with three packages:

| Package | Tech | Purpose |
|---------|------|---------|
| `packages/cli` | Rust (tokio, cpal, clap) | Voice assistant CLI binary `cb` |
| `packages/web` | Vite (static HTML) | Promotional landing page (Vercel) |
| `packages/server` | Node.js | API server (Fly.io) |

## Quick Commands

```bash
# CLI development
pnpm cli              # cargo run (dev build)
pnpm cli:debug        # cargo run -- --debug
pnpm cli:build        # cargo build --release

# Web
pnpm --filter @chatbot/web dev    # Vite dev server on :3000
pnpm --filter @chatbot/web build  # Build to packages/web/dist

# Quality
pnpm lint              # Biome check
pnpm typecheck         # tsc --noEmit across packages
pnpm verify            # lint + typecheck + test + build
```

## Architecture (packages/cli)

```
audio/capture.rs    VAD + mic recording (cpal)
audio/playback.rs   MP3 decode + sequential speaker queue
audio/resample.rs   Multichannel mix → mono 16kHz
speech/doubao/      Doubao ASR (WebSocket v3) + TTS (REST)
llm/openai.rs       OpenAI-compatible streaming client
pipeline/voice.rs   Main loop: record → ASR → wake check → LLM → TTS → play
config/store.rs     TOML config with legacy migration + env var override
cmd/                Subcommands: chat, config, install, logs, open
ui/                 Theme (Color struct), banners, spinners, arrow-key selector
log.rs              JSONL event logging with per-file rotation
i18n.rs             zh/en localization + system prompt builder
```

## Key Design Decisions

- **Config path**: `dirs::config_dir()` (macOS: `~/Library/Application Support/chatbot/`, Linux: `~/.config/chatbot/`)
- **Log path**: `dirs::data_local_dir()` / `chatbot/events/` (3 files: turns/errors/events per date)
- **Color system**: `Color` struct with `Display` impl, gated by `init_colors()` (checks isatty + NO_COLOR + TERM=dumb)
- **Wake word**: 2-pass matching (exact → pinyin) + 5-min active session + deactivation phrases
- **TTS batching**: Sentence-end triggers synthesis, concurrent dispatch, sequential playback
- **Dashboard**: Embedded via `include_str!()`, served by raw TCP (no framework)
- **Release**: macOS universal binary (lipo arm64+x86_64), Linux x86_64 + aarch64 (cross)

## VAD Tuning (CRITICAL)

Current parameters in `audio/capture.rs`:

| Param | Value | Meaning |
|-------|-------|---------|
| NOISE_MULTIPLIER | 5.0 | threshold = ambient_rms x 5.0 |
| NOISE_FLOOR | 0.014 | Hard minimum threshold |
| VAD_WINDOW | 30 | Sliding window size (~0.6s at 48kHz) |
| VAD_TRIGGER | 22 | Need 22/30 (73%) loud chunks to trigger |
| MIN_LOUD_CHUNKS | 15 | Min loud chunks to accept recording |

**Sliding window VAD**: Instead of counting consecutive loud chunks (which keyboard
typing can game), we track a window of the last 30 chunks (~0.6s) and require 73%
to be loud. Speech (sustained) easily reaches 90%+; keyboard (percussive spikes
with gaps) peaks at ~40% and never triggers. The 0.6s window is long enough to
reject percussive noise but short enough for responsive speech onset.

When wake word session is active, `threshold_scale = 0.8` (20% more sensitive).

## Error Log (do NOT repeat these mistakes)

### 1. VAD threshold too aggressive (NOISE_MULTIPLIER 4.0)
**Symptom**: Keyboard typing and fan noise constantly triggers ASR.
**Root cause**: Lowered from 6.0 to 4.0 in one step to fix "speech not detected" — overcorrected.
**Fix**: 5.0 is the balanced value. SPEECH_START_CHUNKS=6, not 4. Test with actual keyboard typing before shipping.
**Rule**: Never change VAD constants by more than 1.0 per iteration. Always test with ambient noise.

### 2. CONFIG_PATH_DISPLAY hardcoded as `~/.config/chatbot/`
**Symptom**: macOS shows wrong path — actual is `~/Library/Application Support/chatbot/`.
**Root cause**: `dirs::config_dir()` returns platform-specific path, but display string was hardcoded.
**Fix**: Dynamic `config_path_display()` function with `~` substitution.
**Rule**: Never hardcode paths that come from `dirs::*`. Always derive display strings from actual paths.

### 3. `chatbox` vs `chatbot` naming inconsistency
**Symptom**: Config files created in `chatbox/` directory but project is called `chatbot`.
**Root cause**: Original code used "chatbox" everywhere, name was changed but not grep'd globally.
**Fix**: Global find-replace with verification scan.
**Rule**: After renaming anything, run `grep -r` across entire repo to catch all occurrences.

### 4. Chinese text breaks box alignment in terminal
**Symptom**: Ready banner box right-side `│` misaligned when containing CJK text.
**Root cause**: `chars().count()` treats CJK chars as width 1, but terminal renders them as width 2.
**Fix**: `display_width()` function that counts CJK chars as 2 columns.
**Rule**: Any fixed-width terminal rendering with user-facing text must use display width, not char count.

### 5. Raw mode println only outputs \n, not \r\n
**Symptom**: Arrow-key selector items scattered horizontally instead of vertically.
**Root cause**: crossterm raw mode disables \r in \n. `println!` outputs \n only.
**Fix**: Use `queue!(stdout, Print(line), Print("\r\n"))` in raw mode.
**Rule**: All terminal output in raw mode must use explicit `\r\n`.

### 6. ESC key inserts escape sequences in text input
**Symptom**: Pressing ESC during `cb config` wizard inserts `^[^[` into input field.
**Root cause**: `stdin().read_line()` doesn't filter ANSI escape sequences.
**Fix**: Check `input.contains('\x1b')` and bail with "已取消".
**Rule**: All text input from terminal must filter escape sequences.

### 7. `const &str` theme colors cannot be conditionally disabled
**Symptom**: ANSI codes output even when piped or in non-TTY context.
**Root cause**: Rust `const &str` cannot check runtime state.
**Fix**: `Color` struct implementing `Display`, checked via `OnceLock<bool>`.
**Rule**: Any value that depends on runtime environment must not be `const`.

### 8. skip events logged for noise (empty_asr, too_short)
**Symptom**: Keyboard noise creates useless log entries in events JSONL.
**Root cause**: Every skip path called `logger.skip()` including noise-triggered ones.
**Fix**: Only log meaningful skips (wake_word, user_deactivated). Noise skips just print to terminal.
**Rule**: Log files are for debugging user-facing issues, not system-level noise.

### 9. Consecutive-chunk VAD gamed by keyboard typing
**Symptom**: Fast keyboard typing triggers "speech detected" because percussive spikes accumulate through PRE_QUIET_RESET tolerance.
**Root cause**: Consecutive loud chunk counter with quiet-gap tolerance (PRE_QUIET_RESET=3) allowed keyboard pulses (loud-quiet-loud-quiet) to accumulate. Each key press contributed 1-2 loud chunks, gaps were forgiven.
**Fix**: Replace consecutive counting with sliding window (VAD_WINDOW=12, VAD_TRIGGER=8). Require 67% of last 12 chunks to be loud. Keyboard typing only gets 20-30% loud chunks and never triggers.
**Rule**: Percussive noise needs ratio-based detection, not consecutive counting. Always test with keyboard typing at normal speed.

### 10. TTS 500 on short text fragments
**Symptom**: `Init Engine Instance failed` from Doubao TTS on fragments like "呢。" or pure punctuation.
**Root cause**: Sentence splitting sends very short fragments. The TTS engine cannot initialize for text with < 2 meaningful characters.
**Fix**: Use `chars().count() < 2` (not `len() < 2` — one Chinese char is 3 bytes). Also remove comma from `is_sentence_end()` — commas create mid-sentence splits that are too short.
**Rule**: Always validate minimum text length before external API calls. Use `chars().count()` for character count, not `.len()` (byte count). TTS engines have undocumented minimum input requirements.

### 11. `dirs::config_dir()` returns platform-specific surprise path
**Symptom**: User expects `~/.config/chatbot/` but macOS puts config in `~/Library/Application Support/chatbot/` (path with spaces).
**Root cause**: `dirs::config_dir()` follows OS conventions — macOS = `~/Library/Application Support/`, Linux = `~/.config/`.
**Fix**: Hardcode `~/.config/chatbot/` on all platforms for CLI tool. Add auto-migration from old macOS path at startup.
**Rule**: CLI tools should use `~/.config/<name>/` universally. Don't use `dirs::config_dir()` for user-facing CLI config — it's designed for GUI apps.

### 12. `len() < 2` doesn't catch single Chinese characters
**Symptom**: TTS still got 500 on "呢" even after adding length check.
**Root cause**: `"呢".len()` returns 3 (UTF-8 bytes), passes the `< 2` check. Should use `chars().count()` which returns 1.
**Fix**: `text.chars().filter(|c| c.is_alphanumeric()).count() < 2`
**Rule**: In Rust, `.len()` is byte length. For character count, always use `.chars().count()`. This is especially critical for CJK text.

### 13. Comma in `is_sentence_end()` causes over-splitting
**Symptom**: TTS gets fragments like "哦" and "。" separately, both too short.
**Root cause**: Comma `，` was treated as sentence end, causing TTS dispatch at every comma. Combined with token-by-token LLM streaming, this creates tiny fragments.
**Fix**: Only split on real sentence-ending punctuation: `。！？.!?\n`. Not commas.
**Rule**: TTS sentence splitting should only break on full stops, not mid-sentence pauses.

### 14. f32 precision destroys decimal values in TOML config
**Symptom**: User enters speed 1.6, config file stores `1.600000023841858`.
**Root cause**: `tts_speed: f32` — f32 cannot represent 1.6 exactly (binary float). TOML serializer writes full precision.
**Fix**: Change to `f64`. f64 represents 1.6 cleanly as `1.6` in TOML.
**Rule**: Config fields that hold user-entered decimal values must be `f64`, never `f32`. f32 only for internal computation where exact representation doesn't matter.
**Rule**: TTS sentence splitting should only break on full stops, not mid-sentence pauses.

### 15. Wizard "confirm" prompt accepts arbitrary input as voice_type
**Symptom**: User types "d" at "Enter 确认 / r 重选" prompt, voice_type gets saved as "d", ALL subsequent TTS calls fail with 401.
**Root cause**: Any input that wasn't "r" was treated as confirmation — but the voice_type was already updated before the prompt. Mistyped input = broken config saved.
**Fix**: Save previous voice_type before changing. Only `Enter` (empty) confirms. `r` reverts and re-selects. ESC reverts and bails. Any other input also confirms (voice was already set correctly by selection).
**Rule**: When a wizard step modifies config then asks for confirmation, ALWAYS save the old value first and restore it on cancel/failure. Never let a text prompt accidentally overwrite a structured field.

### 16. launchd daemon silently stuck on "麦克风不可用" but `cb status` says running
**Symptom**: User reports "daemon 听不到，不说话". `cb status` reports 后台服务运行中. But daemon stderr shows "Failed to get default microphone config" + "麦克风仍不可用" repeating every 60s for hours. No response to speech because no recording happens.
**Root cause**: Two compounding factors:
  1. Daemon binary was `/Users/longye/Desktop/cb-macos-universal` — a hand-downloaded old v0.1.0-beta artifact. It never went through `cb install`'s codesign + entitlement path, so it lacked `com.apple.security.device.audio-input`. TCC refused mic access. launchd has no UI to prompt — daemon just retries forever.
  2. `cb status` only checked `launchctl list`, didn't read stderr. A daemon stuck in a retry loop looked "healthy" from status's view.
**Fix**: 
  - `cb status` now tails `cb.stderr.log` (macOS) or `journalctl` (linux), detects mic-failure patterns in the last 80 lines, warns in red with recovery hint.
  - `cb install` now does a post-load smoke test: waits 3s, scans stderr for mic failures written after install started, rolls back plist + launchctl unload if found. Prevents leaving a broken daemon behind.
**Rule**: Daemons have no interactive UI — every required permission must be triggered in the foreground install session, AND verified post-install. "Process is alive" is not "process is working" — status must read downstream signals (logs, event timestamps). Never trust launchctl/systemctl alone as proof of health.

### 17. Log timestamps rendered in UTC made 10-minute-old events look like afternoon history
**Symptom**: User says "我 10 分钟前触发的", logs display "16:00:31". In Shanghai (UTC+8) at local 00:10 AM, that UTC timestamp IS 10 minutes ago — but it reads as "4 PM" which feels like afternoon. This led to misdiagnosing the state of the daemon (is it really stuck now? or was that hours ago?).
**Root cause**: `millis_to_time()` and `millis_to_date()` in `log.rs` formatted UTC wall clock. Logs stored UTC strings; `cb logs` displayed them as-is.
**Fix**: Both helpers now use `libc::localtime_r` to render in the process's local timezone; fallback to UTC if the host can't resolve the offset. `cb logs` recomputes time from `entry.ts` at display time (not the stored `entry.time` string) so historical UTC-written entries also render in local time.
**Rule**: Timestamps persisted to disk should always be tz-agnostic (unix ms). Human-facing strings should always be rendered in the viewer's local timezone at display time, not baked at write time. Never show UTC to an end user unless the UI explicitly labels it.

### 18. `cb config` saved to disk but running daemon kept using old config
**Symptom**: User enables wake word via wizard, but the backgrounded `cb` daemon keeps behaving as if wake word were off. No error — the daemon is just running on its in-memory snapshot of config from whenever it booted.
**Root cause**: `AppConfig::load()` is called once at daemon startup. There's no watcher on the TOML file, and the daemon holds its config for its entire lifetime. Config saves from a different process (the wizard) never reach the daemon.
**Fix**: New `save_and_reload` helper in `cmd/config.rs` wraps `cfg.save()` — after writing, if `is_daemon_running()`, it calls `restart_daemon()` (a SIGTERM to the launchd/systemd-supervised process, which auto-restarts via `KeepAlive` / `Restart=always` and reads fresh config on boot). Wired into wizard, first-run `ensure_config`, and `cb config set`. Also promoted the daemon helpers in `cmd/update.rs` from private to `pub` and replaced the hardcoded `gui/501/...` launchd target with `libc::getuid()` so it works for non-501 users.
**Rule**: Any config write that could affect a long-running daemon must trigger a restart when the daemon is active. Never assume "config file on disk = config in memory" — only processes that reread do. If the daemon path is optional (not every user has one), gate on `is_daemon_running()` so foreground-only users don't see a bogus "restarted" message.

### 19. DeepSeek `base_url` without `/v1` 404s on `/chat/completions`
**Symptom**: User picks "DeepSeek" in the LLM preset wizard, saves, then every chat turn errors 404.
**Root cause**: Preset was `https://api.deepseek.com`. This client builds requests as `${base_url}/chat/completions`, which becomes `https://api.deepseek.com/chat/completions` — DeepSeek serves the OpenAI-compatible API at `/v1/chat/completions`, so the bare host 404s. DeepSeek's docs list both forms but only `/v1` actually works with raw OpenAI-compatible clients.
**Fix**: Preset now ships `https://api.deepseek.com/v1`. `migrate_legacy` also auto-patches existing profiles whose `base_url` is exactly `https://api.deepseek.com` (trailing slash tolerated) so users who ran the old wizard don't need to re-enter anything.
**Rule**: For OpenAI-compatible providers, always include the version prefix (`/v1`) in the preset URL — clients append path segments directly, they don't canonicalize. Test each preset end-to-end before shipping: "does `${base_url}/chat/completions` return 200?"

### 20. Update hint assumed every user was a curl user
**Symptom**: brew users who ran `cb update` on the prompt ended up with a binary that didn't match what `brew info cb` reported. The update notice also told everyone "运行 `cb update` 升级" regardless of how they actually installed.
**Root cause**: `pending_notice()` emitted a hardcoded `cb update` string; `cmd/update::run()` did a self-replace via `canonicalize() + rename`, which silently overwrites the file inside `/opt/homebrew/Cellar/cb/X.Y.Z/bin/cb`. brew keeps a manifest of the version it installed — rewriting the binary underneath desyncs that, so `brew list --versions` still reports the old version and `brew upgrade` tries to go from old→new even though the file on disk is already new. Same failure mode for npm.
**Fix**: New `detect_channel()` in `update_check.rs` reads the canonical exe path (`/Cellar/`, `/node_modules/@erchoc/`, `~/.local/bin/cb`) and returns `Curl | Brew | Npm | Direct`. `upgrade_hint()` returns the right command per channel. `cmd/update::run()` now refuses on brew/npm and redirects to `brew upgrade erchoc/tap/cb` / `npm install -g @erchoc/chatbot@latest`. The notice banner in `cb chat`, `cb status`, and the voice daemon all show the channel-correct command.
**Rule**: A binary that ships through multiple install channels must never assume it owns its own location. Before any self-modifying operation (update, uninstall, file mv under the binary's dir), detect the channel and refuse for package-manager installs. Always check `current_exe().canonicalize()` against known manager paths.

### 21. Daemon ran for weeks without re-checking for updates
**Symptom**: User running `cb` as a background voice daemon (launchd/systemd) never saw update notices even months after a new release. Foreground `cb chat` users saw them normally.
**Root cause**: `spawn_background_check()` fired once at daemon startup. `pending_notice()` reads a 24h-cached result, so if the daemon booted before a new release, the cache said "nothing new" and nothing refreshed it until the daemon restarted. Worst case: daemon runs 3 months without a restart, update cache never touched since day 1.
**Fix**: Call `spawn_background_check()` at the top of the daemon's main voice-turn loop. It's self-throttled (inner guard returns immediately if the cache is <24h old), so the per-turn cost is a cheap file-stat + comparison. Also added `notify_desktop()` — macOS `osascript` / Linux `notify-send` — so when the daemon's stdout banner lands in `cb.stderr.log` (invisible to the user), an OS-level toast actually reaches them.
**Rule**: Long-lived daemons can't rely on startup-only state. Anything time-sensitive (update checks, cert rotation, config reload) needs to be re-evaluated in the main loop, gated by its own interval cache. Also: daemon stdout is a log file, not a user-facing channel — if a user action is expected, route through the OS notification system, not `println!`.

### 22. `cb update` compared versions as strings, would silently downgrade beta users
**Symptom**: User on v0.1.1-beta.1 runs `cb update`. Latest stable is v0.1.0. String compare: `"0.1.1-beta.1" != "0.1.0"` → code falls through to the download branch, overwriting the beta with the older stable. Silent downgrade.
**Root cause**: `cmd/update::run()` used raw `latest == current` string equality to decide "already on latest". No awareness that a prerelease sits *between* two stables in semver ordering.
**Fix**: New `compare_versions()` in `update_check.rs` implements semver precedence (including the rule that `0.1.1-beta.1 < 0.1.1`). `cb update` now uses it: `Equal` → already-on-latest; `Greater` → local is ahead (don't downgrade, suggest `--force` if they actually want to switch channels); `Less` → upgrade. Added tests covering stable/beta/beta-vs-older-stable/v-prefix cases.
**Also**: added `cb update -f/--force` that (a) widens the resolver to include prereleases via `/releases?per_page=1` instead of `/releases/latest`, and (b) downloads regardless of comparison result. Lets beta追新 users roll forward without the stable-channel safety net.
**Rule**: Never do string equality on version numbers. Semver has explicit precedence rules — use them, or at minimum compare parsed numeric tuples and treat `-suffix` as "older than same x.y.z without suffix".

## Release Cadence (phased rollout)

每次版本变更按三段式铺开，给追新用户和求稳用户不同节奏：

| 阶段 | 时机 | 动作 | 触达的用户 |
|------|------|------|-----------|
| 1. curl | 立即 | `git push` 到 main（Vercel 自动部署 install.sh）+ tag 一个 prerelease（或让 curl 走 `CB_CHANNEL=any`） | 追新用户：`curl ... \| bash` 抓最新 release |
| 2. 正式版 | 次日无反馈 | tag 稳定版（如 `v0.1.1`），推触发 release.yml → npm 同步发布 | `CB_CHANNEL=stable` 的 curl 用户 + `npm install -g @erchoc/chatbot` |
| 3. brew | 正式版发布一周后无反馈 | 在 `Erchoc/homebrew-tap` 手动运行 `bump formulae` workflow（tool=cb） | `brew install erchoc/tap/cb` 的求稳用户 |

**规则**：
- 任何阶段收到用户负反馈 → 回滚到上一阶段，修复后重新从阶段 1 开始。
- 阶段 2、3 之间不要跳步。brew 是最慢的那一档，默认一周 soak。
- 修 bug（即使是紧急）也走一样的流程 —— curl 先上，次日升正式版，一周升 brew。
- `CB_CHANNEL` 默认值 `any`（含 prerelease），所以 curl 用户会自动拿到阶段 1 的版本。

## Rules

- **Breaking changes after v1.0.0**: Any change that alters user-facing behavior, config format, file paths, or CLI interface must be confirmed with the user before proceeding.
- **Naming**: Always use `chatbot` (not chatbox). After any rename, run `grep -r` across the entire repo.
- **Paths**: All user-facing paths use `~/.config/chatbot/`. Never use `dirs::config_dir()` or `dirs::data_local_dir()`.
- **VAD tuning**: Never change more than one parameter at a time. Always test with keyboard typing.
- **TTS text**: Minimum 2 alphanumeric characters before sending to API. Use `chars().count()` not `len()`.
- **Error logging**: Record every mistake in CLAUDE.md with symptom/cause/fix/rule.

## Naming Conventions

- Package: `chatbot` (not chatbox)
- Binary: `cb`
- Config dir: `~/.config/chatbot/` (all platforms)
- Log dir: `~/.config/chatbot/events/`
- History dir: `~/.config/chatbot/history/`
- Service label: `com.erchoc.chatbot`
- Systemd unit: `chatbot.service`

## Current Progress

- [x] Core voice pipeline (record → ASR → LLM → TTS → play)
- [x] Interactive config wizard with arrow-key selection
- [x] Multi-LLM profile management
- [x] Wake word with pinyin homophone matching
- [x] Web dashboard with event timeline + light/dark theme
- [x] Structured JSONL logging with rotation
- [x] `cb logs` terminal log viewer with `-f` follow mode
- [x] `cb chat <message>` single-shot text mode
- [x] Release workflow (macOS universal + Linux x86/arm)
- [x] Color system with NO_COLOR / TTY detection
- [x] Config path unified to `~/.config/chatbot/` with auto-migration
- [x] Passive update notifier (daily background check, minor-version bumps only)
- [x] `cb status` surfaces daemon mic-failure retry loop (reads stderr/journald)
- [x] `cb install` post-load smoke test rolls back broken installs
- [x] Logs render in local timezone (was UTC, confused users about event recency)
- [x] **v0.1.0 released** (2026-04-18) — 四端生效: GitHub Release / curl install.sh / Homebrew tap / npm `@erchoc/chatbot`
