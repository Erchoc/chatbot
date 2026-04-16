use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

fn history_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("chatbot")
        .join("history")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub role: String,   // "user" | "assistant"
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub created_at: String,
    pub turns: Vec<Turn>,
}

impl Conversation {
    pub fn new() -> Self {
        let now = chrono_now();
        Self {
            id: now.replace([':', '-', 'T', ' '], "").chars().take(14).collect(),
            created_at: now,
            turns: Vec::new(),
        }
    }

    pub fn add_turn(&mut self, role: &str, content: &str) {
        self.turns.push(Turn {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono_now(),
        });
    }

    /// Save conversation to disk
    pub fn save(&self) -> Result<()> {
        let dir = history_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", self.id));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }
}

/// List all saved conversations (newest first), returning summary info
pub fn list_conversations() -> Result<Vec<ConversationSummary>> {
    let dir = history_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .collect();

    // Sort by modification time, newest first
    entries.sort_by(|a, b| {
        let ta = a.metadata().and_then(|m| m.modified()).ok();
        let tb = b.metadata().and_then(|m| m.modified()).ok();
        tb.cmp(&ta)
    });

    let mut summaries = Vec::new();
    for entry in entries {
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            if let Ok(conv) = serde_json::from_str::<Conversation>(&content) {
                let preview = conv
                    .turns
                    .iter()
                    .find(|t| t.role == "user")
                    .map(|t| {
                        let s = t.content.chars().take(80).collect::<String>();
                        if t.content.chars().count() > 80 {
                            format!("{s}...")
                        } else {
                            s
                        }
                    })
                    .unwrap_or_default();

                summaries.push(ConversationSummary {
                    id: conv.id,
                    created_at: conv.created_at,
                    turn_count: conv.turns.len(),
                    preview,
                });
            }
        }
    }

    Ok(summaries)
}

/// Load a specific conversation by ID
pub fn load_conversation(id: &str) -> Result<Conversation> {
    let path = history_dir().join(format!("{id}.json"));
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Conversation {id} not found"))?;
    let conv: Conversation = serde_json::from_str(&content)?;
    Ok(conv)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub created_at: String,
    pub turn_count: usize,
    pub preview: String,
}

/// Simple timestamp without chrono dependency
fn chrono_now() -> String {
    // Use std::time for a basic ISO-ish timestamp
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();

    // Convert epoch seconds to readable format
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Rough year/month/day from epoch (good enough for filenames)
    let (year, month, day) = epoch_days_to_ymd(days);

    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z"
    )
}

fn epoch_days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
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
