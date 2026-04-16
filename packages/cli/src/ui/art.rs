use super::theme::*;

/// Robot face frames for different states
pub struct Face;

impl Face {
    /// Idle/ready state - friendly face
    pub fn idle() -> String {
        format!(
            r#"
   {CYAN}╭───────────╮{RESET}
   {CYAN}│{RESET} {BR_CYAN}◉{RESET}     {BR_CYAN}◉{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {BR_GREEN}▽{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}"#
        )
    }

    /// Listening - ears up, eyes wide
    pub fn listen_frames() -> Vec<String> {
        vec![
            format!(
                r#"   {CYAN}╭───{BR_MAGENTA}◈{CYAN}───────╮{RESET}
   {CYAN}│{RESET} {BR_CYAN}◉{RESET}     {BR_CYAN}◉{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {MUTED}◡{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}"#
            ),
            format!(
                r#"   {CYAN}╭───{BR_YELLOW}◈{CYAN}───────╮{RESET}
   {CYAN}│{RESET} {BR_CYAN}◎{RESET}     {BR_CYAN}◎{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {MUTED}◡{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}"#
            ),
            format!(
                r#"   {CYAN}╭───{BR_GREEN}◈{CYAN}───────╮{RESET}
   {CYAN}│{RESET} {BR_CYAN}◉{RESET}     {BR_CYAN}◉{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {MUTED}◡{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}"#
            ),
            format!(
                r#"   {CYAN}╭───{BR_CYAN}◈{CYAN}───────╮{RESET}
   {CYAN}│{RESET} {BR_CYAN}◎{RESET}     {BR_CYAN}◎{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {MUTED}◡{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}"#
            ),
        ]
    }

    /// Thinking - looking sideways, dots
    pub fn think_frames() -> Vec<String> {
        vec![
            format!(
                r#"   {CYAN}╭───────────╮{RESET}
   {CYAN}│{RESET} {BR_YELLOW}◔{RESET}     {BR_YELLOW}◔{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {BR_YELLOW}~{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}  {BR_YELLOW}thinking .{RESET}"#
            ),
            format!(
                r#"   {CYAN}╭───────────╮{RESET}
   {CYAN}│{RESET} {BR_YELLOW}◑{RESET}     {BR_YELLOW}◑{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {BR_YELLOW}~{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}  {BR_YELLOW}thinking ..{RESET}"#
            ),
            format!(
                r#"   {CYAN}╭───────────╮{RESET}
   {CYAN}│{RESET} {BR_YELLOW}◕{RESET}     {BR_YELLOW}◕{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {BR_YELLOW}~{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}  {BR_YELLOW}thinking ...{RESET}"#
            ),
        ]
    }

    /// Speaking - mouth animates
    pub fn speak_frames() -> Vec<String> {
        vec![
            format!(
                r#"   {CYAN}╭───────────╮{RESET}
   {CYAN}│{RESET} {BR_GREEN}◉{RESET}     {BR_GREEN}◉{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {BR_GREEN}○{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}"#
            ),
            format!(
                r#"   {CYAN}╭───────────╮{RESET}
   {CYAN}│{RESET} {BR_GREEN}◉{RESET}     {BR_GREEN}◉{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {BR_GREEN}◎{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}"#
            ),
            format!(
                r#"   {CYAN}╭───────────╮{RESET}
   {CYAN}│{RESET} {BR_GREEN}◉{RESET}     {BR_GREEN}◉{RESET} {CYAN}│{RESET}
   {CYAN}│{RESET}    {BR_GREEN}▽{RESET}    {CYAN}│{RESET}
   {CYAN}╰───────────╯{RESET}"#
            ),
        ]
    }

    /// Error - distressed
    pub fn error() -> String {
        format!(
            r#"
   {RED}╭───────────╮{RESET}
   {RED}│{RESET} {BR_RED}✖{RESET}     {BR_RED}✖{RESET} {RED}│{RESET}
   {RED}│{RESET}    {BR_RED}△{RESET}    {RED}│{RESET}
   {RED}╰───────────╯{RESET}"#
        )
    }
}

/// Audio level visualizer bar
pub fn level_bar(level: f32, width: usize) -> String {
    let filled = ((level * width as f32).round() as usize).min(width);
    let empty = width - filled;
    let bar_char = if filled > width * 3 / 4 {
        format!("{BR_RED}█{RESET}")
    } else if filled > width / 2 {
        format!("{BR_YELLOW}█{RESET}")
    } else {
        format!("{BR_GREEN}█{RESET}")
    };
    let empty_char = format!("{MUTED}░{RESET}");
    format!(
        "{}{}",
        bar_char.repeat(filled),
        empty_char.repeat(empty)
    )
}
