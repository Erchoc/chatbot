// Terminal color/style constants.
//
// Each constant is a `Color` newtype that implements `Display`.
// In format strings nothing changes — `{CYAN}` still works.
//
// Call `init_colors()` once at startup. Colors are automatically
// disabled when stdout is not a TTY, when NO_COLOR is set, or
// when TERM=dumb.

use std::fmt;
use std::sync::OnceLock;

static COLORS_ON: OnceLock<bool> = OnceLock::new();

/// Must be called once before any printing, typically at the top of `main`.
pub fn init_colors() {
    use std::io::IsTerminal;
    let on = std::io::stdout().is_terminal()
        && std::env::var("NO_COLOR").is_err()
        && std::env::var("TERM").as_deref() != Ok("dumb");
    COLORS_ON.get_or_init(|| on);
}

/// A terminal color/style token. Emits an ANSI escape when colors are enabled,
/// emits nothing otherwise. Implements `Display` so it works in format strings.
#[derive(Clone, Copy)]
pub struct Color(pub &'static str);

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *COLORS_ON.get().unwrap_or(&true) {
            f.write_str(self.0)
        } else {
            Ok(())
        }
    }
}

// ── Regular colors ────────────────────────────────────────────────────────────
pub const RED: Color     = Color("\x1b[31m");
pub const GREEN: Color   = Color("\x1b[32m");
pub const YELLOW: Color  = Color("\x1b[33m");
pub const BLUE: Color    = Color("\x1b[34m");
pub const MAGENTA: Color = Color("\x1b[35m");
pub const CYAN: Color    = Color("\x1b[36m");

// ── Bright colors ─────────────────────────────────────────────────────────────
pub const BR_RED: Color     = Color("\x1b[91m");
pub const BR_GREEN: Color   = Color("\x1b[92m");
pub const BR_YELLOW: Color  = Color("\x1b[93m");
pub const BR_BLUE: Color    = Color("\x1b[94m");
pub const BR_MAGENTA: Color = Color("\x1b[95m");
pub const BR_CYAN: Color    = Color("\x1b[96m");

// ── Styles ────────────────────────────────────────────────────────────────────
pub const BOLD: Color   = Color("\x1b[1m");
pub const DIM: Color    = Color("\x1b[90m");
pub const ITALIC: Color = Color("\x1b[3m");
pub const RESET: Color  = Color("\x1b[0m");

// ── Semantic aliases ──────────────────────────────────────────────────────────
pub const USER_COLOR: Color  = BR_CYAN;
pub const BOT_COLOR: Color   = BR_GREEN;
pub const ERROR_COLOR: Color = BR_RED;
pub const INFO_COLOR: Color  = BR_BLUE;
pub const WARN_COLOR: Color  = BR_YELLOW;
pub const ACCENT: Color      = BR_MAGENTA;
pub const MUTED: Color       = DIM;

// ── Text utilities ────────────────────────────────────────────────────────────

/// Terminal display width of a string.
/// ASCII = 1 column, CJK/fullwidth = 2 columns.
pub fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            let n = c as u32;
            if matches!(
                n,
                0x1100..=0x115F
                | 0x2E80..=0x303E
                | 0x3041..=0x33FF
                | 0x3400..=0x4DBF
                | 0x4E00..=0x9FFF
                | 0xA000..=0xA4CF
                | 0xAC00..=0xD7AF
                | 0xF900..=0xFAFF
                | 0xFE10..=0xFE1F
                | 0xFE30..=0xFE6F
                | 0xFF01..=0xFF60
                | 0xFFE0..=0xFFE6
                | 0x1F300..=0x1F9FF
            ) { 2 } else { 1 }
        })
        .sum()
}

/// Pad a string to a target display width with spaces.
pub fn pad_display(s: &str, target_width: usize) -> String {
    let w = display_width(s);
    if w >= target_width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(target_width - w))
    }
}
