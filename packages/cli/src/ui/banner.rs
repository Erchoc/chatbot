use super::theme::*;

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
            " ".repeat(31_usize.saturating_sub(msg.chars().count()))
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
