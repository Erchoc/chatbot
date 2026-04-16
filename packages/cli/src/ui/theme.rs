// ANSI color codes for terminal output

// Regular colors
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const MAGENTA: &str = "\x1b[35m";
pub const CYAN: &str = "\x1b[36m";

// Bright colors
pub const BR_RED: &str = "\x1b[91m";
pub const BR_GREEN: &str = "\x1b[92m";
pub const BR_YELLOW: &str = "\x1b[93m";
pub const BR_BLUE: &str = "\x1b[94m";
pub const BR_MAGENTA: &str = "\x1b[95m";
pub const BR_CYAN: &str = "\x1b[96m";

// Styles
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[90m";
pub const ITALIC: &str = "\x1b[3m";
pub const RESET: &str = "\x1b[0m";

// Semantic aliases
pub const USER_COLOR: &str = BR_CYAN;
pub const BOT_COLOR: &str = BR_GREEN;
pub const ERROR_COLOR: &str = BR_RED;
pub const INFO_COLOR: &str = BR_BLUE;
pub const WARN_COLOR: &str = BR_YELLOW;
pub const ACCENT: &str = BR_MAGENTA;
pub const MUTED: &str = DIM;
