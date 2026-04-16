use super::theme::*;

/// Calculate the terminal display width of a string.
/// ASCII chars = 1 column, CJK/fullwidth chars = 2 columns.
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            let n = c as u32;
            if matches!(
                n,
                0x1100..=0x115F  // Hangul Jamo
                | 0x2E80..=0x303E  // CJK Radicals + Symbols
                | 0x3041..=0x33FF  // Hiragana / Katakana / CJK Symbols
                | 0x3400..=0x4DBF  // CJK Extension A
                | 0x4E00..=0x9FFF  // CJK Unified
                | 0xA000..=0xA4CF  // Yi
                | 0xAC00..=0xD7AF  // Hangul Syllables
                | 0xF900..=0xFAFF  // CJK Compatibility
                | 0xFE10..=0xFE1F  // Vertical Forms
                | 0xFE30..=0xFE6F  // CJK Compatibility Forms
                | 0xFF01..=0xFF60  // Fullwidth Latin
                | 0xFFE0..=0xFFE6  // Fullwidth Signs
                | 0x1F300..=0x1F9FF // Most Emoji
            ) {
                2
            } else {
                1
            }
        })
        .sum()
}

/// Print the startup banner with ASCII art
pub fn print_banner(version: &str) {
    println!(
        r#"
   {BR_CYAN}██████╗{BR_MAGENTA}██████╗ {RESET}
   {BR_CYAN}██╔════╝{BR_MAGENTA}██╔══██╗{RESET}
   {BR_CYAN}██║     {BR_MAGENTA}██████╔╝{RESET}   {BOLD}Chatbox Voice Assistant{RESET}
   {BR_CYAN}██║     {BR_MAGENTA}██╔══██╗{RESET}   {MUTED}v{version}{RESET}
   {BR_CYAN}╚██████╗{BR_MAGENTA}██████╔╝{RESET}
   {BR_CYAN} ╚═════╝{BR_MAGENTA}╚═════╝ {RESET}
"#
    );
}

/// Print the ready banner with robot face
pub fn print_ready(messages: &[&str]) {
    println!();
    println!(
        "   {BR_CYAN}╭───────────────────────────────────╮{RESET}"
    );
    for msg in messages {
        if msg.is_empty() {
            continue;
        }
        println!(
            "   {BR_CYAN}│{RESET}  {BOLD}{msg}{RESET}{}  {BR_CYAN}│{RESET}",
            " ".repeat(31_usize.saturating_sub(display_width(msg)))
        );
    }
    println!(
        "   {BR_CYAN}╰───────────────────────────────────╯{RESET}"
    );
    println!();
}

/// Print a colored separator line
pub fn separator() {
    println!(
        "   {MUTED}─────────────────────────────────────{RESET}"
    );
}
