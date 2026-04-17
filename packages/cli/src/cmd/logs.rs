use anyhow::Result;

use crate::log::{self, LogEvent};
use crate::ui::theme::*;

/// `cb logs` — print recent event logs to stdout.
/// `cb logs -f` — tail the log, polling every 2 seconds.
/// `cb logs --date 2026-04-16` — view a specific date.
pub async fn run(follow: bool, date: Option<String>) -> Result<()> {
    let target_date = date.unwrap_or_else(|| log::millis_to_date(log::now_millis()));

    let log_path = log::log_dir();
    let entries = log::read_date(&target_date);
    if entries.is_empty() && !follow {
        println!("   {MUTED}No logs for {target_date}{RESET}");
        println!("   {MUTED}Log directory: {}{RESET}", log_path.display());
        println!("   {MUTED}Run `cb` to start a voice chat session{RESET}");
        return Ok(());
    }

    // Print header
    println!("   {BOLD}cb logs{RESET}  {MUTED}{target_date}{RESET}");
    println!("   {MUTED}{}{RESET}", log_path.display());
    println!("   {MUTED}─────────────────────────────────────{RESET}");

    let mut printed = 0;
    for entry in &entries {
        print_entry(entry);
        printed += 1;
    }

    if !follow {
        println!(
            "   {MUTED}─────────────────────────────────────{RESET}"
        );
        println!("   {MUTED}{printed} events{RESET}");
        return Ok(());
    }

    // Follow mode: poll for new entries
    println!("   {BR_GREEN}▸ Following (Ctrl+C to stop)...{RESET}");
    let mut last_ts = entries.last().map(|e| e.ts).unwrap_or(0);

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let fresh = log::read_date(&target_date);
        for entry in &fresh {
            if entry.ts > last_ts {
                print_entry(entry);
                last_ts = entry.ts;
            }
        }
    }
}

fn print_entry(entry: &log::LogEntry) {
    // Always recompute from the unix-ms ts so historical entries written
    // in UTC render in the user's local timezone along with new ones.
    let time = log::millis_to_time(entry.ts);
    let time = &time;
    match &entry.event {
        LogEvent::SessionStart {
            session_id,
            model,
            voice,
            language,
        } => {
            println!(
                "   {BR_MAGENTA}{time}{RESET}  {BOLD}● session start{RESET}  {MUTED}{session_id}{RESET}"
            );
            println!(
                "           {MUTED}model={model}  voice={voice}  lang={language}{RESET}"
            );
        }
        LogEvent::SessionEnd {
            turn_count, ..
        } => {
            println!(
                "   {MUTED}{time}  ● session end  ({turn_count} turns){RESET}"
            );
        }
        LogEvent::Turn {
            asr,
            reply,
            stt_ms,
            llm_ms,
            tts_ms,
            ..
        } => {
            // Truncate long replies for terminal readability
            let reply_short = if reply.len() > 80 {
                format!("{}...", &reply[..reply.floor_char_boundary(77)])
            } else {
                reply.clone()
            };
            println!(
                "   {BR_CYAN}{time}{RESET}  🎤 {asr}"
            );
            println!(
                "           🤖 {reply_short}"
            );
            println!(
                "           {MUTED}STT {:.1}s  LLM {:.1}s  TTS {:.1}s{RESET}",
                stt_ms / 1000.0,
                llm_ms / 1000.0,
                tts_ms / 1000.0,
            );
        }
        LogEvent::Skip { reason, asr, .. } => {
            let asr_str = asr.as_deref().unwrap_or("");
            println!(
                "   {BR_YELLOW}{time}{RESET}  ⏭ {reason}  {MUTED}{asr_str}{RESET}"
            );
        }
        LogEvent::Error { stage, msg, .. } => {
            println!(
                "   {BR_RED}{time}{RESET}  ❌ [{stage}] {msg}"
            );
        }
    }
}
