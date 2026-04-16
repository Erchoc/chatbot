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
