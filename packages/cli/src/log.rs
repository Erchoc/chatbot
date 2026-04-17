//! Structured event logging to ~/.local/share/chatbot/events/
//!
//! Three separate append-only JSONL files per date, each with a size cap:
//!   turns-YYYY-MM-DD.jsonl   — conversation turns  (max 10 MB)
//!   errors-YYYY-MM-DD.jsonl  — errors only         (max  2 MB)
//!   events-YYYY-MM-DD.jsonl  — session + skip       (max  5 MB)
//!
//! When a file exceeds its limit the oldest 30 % of lines are dropped and
//! the file is rewritten atomically (write tmp → rename).

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ─── Size limits ──────────────────────────────────────────────────────────────

const MAX_TURNS_BYTES: u64  = 10 * 1024 * 1024; //  10 MB
const MAX_ERRORS_BYTES: u64 =  2 * 1024 * 1024; //   2 MB
const MAX_EVENTS_BYTES: u64 =  5 * 1024 * 1024; //   5 MB

/// Keep this fraction of lines when rotating (drop oldest 30 %).
const KEEP_RATIO: f64 = 0.70;

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Unix milliseconds — authoritative, tz-agnostic. Always render from
    /// this via `millis_to_time(ts)` at display time; never trust `time`
    /// for rendering (historical entries were stored in UTC).
    pub ts: u64,
    /// Pre-formatted "HH:MM:SS" in the writer's local timezone at write time.
    /// Kept for dashboard consumption; display code should recompute from `ts`.
    pub time: String,
    /// ISO date string used for file grouping, e.g. "2026-04-16".
    pub date: String,
    #[serde(flatten)]
    pub event: LogEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogEvent {
    SessionStart {
        session_id: String,
        model: String,
        voice: String,
        language: String,
    },
    Turn {
        session_id: String,
        asr: String,
        reply: String,
        stt_ms: f32,
        ttft_ms: f32,
        llm_ms: f32,
        tts_ms: f32,
    },
    Skip {
        session_id: String,
        /// "too_short" | "empty_asr" | "wake_word" | "after_wake_word"
        reason: String,
        asr: Option<String>,
    },
    Error {
        session_id: String,
        /// "asr" | "llm" | "tts" | "audio"
        stage: String,
        msg: String,
    },
    SessionEnd {
        session_id: String,
        turn_count: usize,
    },
}

impl LogEvent {
    /// Which file bucket does this event belong to?
    fn bucket(&self) -> Bucket {
        match self {
            LogEvent::Turn { .. } => Bucket::Turns,
            LogEvent::Error { .. } => Bucket::Errors,
            _ => Bucket::Events,
        }
    }
}

#[derive(Clone, Copy)]
enum Bucket {
    Turns,
    Errors,
    Events,
}

impl Bucket {
    fn prefix(self) -> &'static str {
        match self {
            Bucket::Turns  => "turns",
            Bucket::Errors => "errors",
            Bucket::Events => "events",
        }
    }
    fn max_bytes(self) -> u64 {
        match self {
            Bucket::Turns  => MAX_TURNS_BYTES,
            Bucket::Errors => MAX_ERRORS_BYTES,
            Bucket::Events => MAX_EVENTS_BYTES,
        }
    }
}

// ─── Logger ───────────────────────────────────────────────────────────────────

pub struct EventLogger {
    session_id: String,
}

impl EventLogger {
    pub fn new(session_id: String) -> Self {
        Self { session_id }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Write an event. Silently ignores I/O errors so logging never crashes cb.
    pub fn log(&self, event: LogEvent) {
        let now = now_millis();
        let entry = LogEntry {
            ts: now,
            time: millis_to_time(now),
            date: millis_to_date(now),
            event,
        };
        let bucket = entry.event.bucket();
        if let Ok(line) = serde_json::to_string(&entry) {
            let path = bucket_path(bucket, &millis_to_date(now));
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Rotate before appending if the file is already too large.
            rotate_if_needed(&path, bucket.max_bytes());
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(f, "{line}");
            }
        }
    }

    // ── Convenience wrappers ──────────────────────────────────────────────────

    pub fn session_start(&self, model: &str, voice: &str, language: &str) {
        self.log(LogEvent::SessionStart {
            session_id: self.session_id.clone(),
            model: model.to_string(),
            voice: voice.to_string(),
            language: language.to_string(),
        });
    }

    pub fn turn(
        &self,
        asr: &str,
        reply: &str,
        stt_ms: f32,
        ttft_ms: f32,
        llm_ms: f32,
        tts_ms: f32,
    ) {
        self.log(LogEvent::Turn {
            session_id: self.session_id.clone(),
            asr: asr.to_string(),
            reply: reply.to_string(),
            stt_ms,
            ttft_ms,
            llm_ms,
            tts_ms,
        });
    }

    pub fn skip(&self, reason: &str, asr: Option<&str>) {
        self.log(LogEvent::Skip {
            session_id: self.session_id.clone(),
            reason: reason.to_string(),
            asr: asr.map(str::to_string),
        });
    }

    pub fn error(&self, stage: &str, msg: &str) {
        self.log(LogEvent::Error {
            session_id: self.session_id.clone(),
            stage: stage.to_string(),
            msg: msg.to_string(),
        });
    }

    pub fn session_end(&self, turn_count: usize) {
        self.log(LogEvent::SessionEnd {
            session_id: self.session_id.clone(),
            turn_count,
        });
    }
}

// ─── Query helpers (used by cmd/open.rs) ──────────────────────────────────────

/// List available log dates, newest first, as "YYYY-MM-DD" strings.
pub fn list_dates() -> Vec<String> {
    let dir = log_dir();
    if !dir.exists() {
        return vec![];
    }
    let mut dates: Vec<String> = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // Match "turns-YYYY-MM-DD.jsonl", "errors-…", "events-…"
            for prefix in &["turns-", "errors-", "events-"] {
                if let Some(rest) = name.strip_prefix(prefix) {
                    if let Some(date) = rest.strip_suffix(".jsonl") {
                        if date.len() == 10 && date.contains('-') {
                            return Some(date.to_string());
                        }
                    }
                }
            }
            None
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    dates.sort_by(|a, b| b.cmp(a));
    dates
}

/// Read all entries for the given date, merged and sorted by ts.
pub fn read_date(date: &str) -> Vec<LogEntry> {
    merge_buckets(date)
}

/// Read today's entries, merged and sorted by ts.
pub fn today_events() -> Vec<LogEntry> {
    merge_buckets(&millis_to_date(now_millis()))
}

// ─── Internals ────────────────────────────────────────────────────────────────

pub fn log_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("chatbot")
        .join("events")
}

fn bucket_path(bucket: Bucket, date: &str) -> PathBuf {
    log_dir().join(format!("{}-{}.jsonl", bucket.prefix(), date))
}

/// Read and merge all three bucket files for a date, sorted ascending by ts.
fn merge_buckets(date: &str) -> Vec<LogEntry> {
    let mut all: Vec<LogEntry> = Vec::new();
    for bucket in &[Bucket::Turns, Bucket::Errors, Bucket::Events] {
        let path = bucket_path(*bucket, date);
        all.extend(read_jsonl(&path));
    }
    all.sort_by_key(|e| e.ts);
    all
}

fn read_jsonl(path: &PathBuf) -> Vec<LogEntry> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// If `path` is larger than `max_bytes`, drop the oldest 30 % of lines and
/// rewrite the file. Uses a temp file + rename for atomicity.
fn rotate_if_needed(path: &PathBuf, max_bytes: u64) {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return, // file doesn't exist yet
    };
    if meta.len() <= max_bytes {
        return;
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return;
    }

    let keep = ((lines.len() as f64) * KEEP_RATIO).ceil() as usize;
    let drop = lines.len().saturating_sub(keep);
    let kept = &lines[drop..];

    // Write to a sibling temp file then rename atomically.
    let tmp_path = path.with_extension("jsonl.tmp");
    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp_path)?;
        for line in kept {
            writeln!(f, "{line}")?;
        }
        f.flush()?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
}

pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Break unix seconds into a local-time `struct tm` via `localtime_r(3)`.
///
/// Returns `None` if the platform's `localtime_r` fails (shouldn't happen
/// on macOS/Linux for any reasonable time_t). Callers fall back to UTC.
fn local_tm(secs: i64) -> Option<libc::tm> {
    use std::mem::MaybeUninit;
    let t: libc::time_t = secs as libc::time_t;
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    let ok = unsafe { !libc::localtime_r(&t, tm.as_mut_ptr()).is_null() };
    if ok {
        Some(unsafe { tm.assume_init() })
    } else {
        None
    }
}

/// Convert unix milliseconds to "HH:MM:SS" in the process's local timezone.
///
/// Falls back to UTC if the host can't resolve the local offset.
pub fn millis_to_time(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    if let Some(tm) = local_tm(secs) {
        return format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec);
    }
    // UTC fallback
    let s = (ms / 1000) % 86400;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Convert unix milliseconds to "YYYY-MM-DD" in the process's local timezone.
///
/// Files are grouped by this value, so rotation, file lookup, and the UI
/// all share the same notion of "today" as the user's wall clock. Falls
/// back to UTC if the host can't resolve the local offset.
pub fn millis_to_date(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    if let Some(tm) = local_tm(secs) {
        return format!(
            "{:04}-{:02}-{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday
        );
    }
    // UTC fallback
    let days = (ms / 1000) / 86400;
    let (y, m, d) = epoch_days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn epoch_days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    days += 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
