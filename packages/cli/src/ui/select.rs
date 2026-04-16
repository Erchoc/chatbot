use std::io::{self, IsTerminal, Write};

use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute, queue,
    style::Print,
    terminal::{self, ClearType},
};

pub struct SelectOption {
    pub label: String,
    pub hint: String,
    pub badge: Option<String>,
}

impl SelectOption {
    pub fn new(label: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            hint: hint.into(),
            badge: None,
        }
    }

    pub fn with_badge(mut self, badge: impl Into<String>) -> Self {
        let b = badge.into();
        self.badge = if b.is_empty() { None } else { Some(b) };
        self
    }
}

/// Interactive arrow-key selector.
/// Returns `Some(index)` on Enter, `None` on Esc.
/// Falls back to number input when stdin is not a TTY.
pub fn run(options: &[SelectOption], initial: usize) -> Result<Option<usize>> {
    if options.is_empty() {
        return Ok(None);
    }
    if !io::stdin().is_terminal() {
        return fallback_number(options, initial);
    }

    let mut pos = initial.min(options.len() - 1);
    let mut stdout = io::stdout();

    terminal::enable_raw_mode()?;
    execute!(stdout, cursor::Hide)?;

    // Draw once before entering the event loop
    draw(&mut stdout, options, pos, true)?;

    let result = (|| -> Result<Option<usize>> {
        loop {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        pos = if pos == 0 { options.len() - 1 } else { pos - 1 };
                        draw(&mut stdout, options, pos, false)?;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        pos = (pos + 1) % options.len();
                        draw(&mut stdout, options, pos, false)?;
                    }
                    KeyCode::Enter => return Ok(Some(pos)),
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                    _ => {}
                },
                _ => {}
            }
        }
    })();

    // Restore terminal and clear the drawn selector
    execute!(stdout, cursor::Show)?;
    terminal::disable_raw_mode()?;
    clear_drawn(&mut stdout, options.len())?;

    result
}

// ─── Drawing ──────────────────────────────────────────────────────────────────

/// Total lines drawn by the selector: items + 1 footer.
fn drawn_lines(n: usize) -> u16 {
    (n + 1) as u16
}

/// On redraw: move cursor up to the first drawn line, reset to column 0, clear down.
/// On first draw: just print in place.
fn draw(
    stdout: &mut impl Write,
    options: &[SelectOption],
    pos: usize,
    first: bool,
) -> Result<()> {
    if !first {
        queue!(
            stdout,
            cursor::MoveUp(drawn_lines(options.len())),
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::FromCursorDown),
        )?;
    }

    for (i, opt) in options.iter().enumerate() {
        let line = format_option(i, pos, opt);
        // \r\n required in raw mode — \n alone does not reset column to 0
        queue!(stdout, Print(line), Print("\r\n"))?;
    }

    // Footer hint
    queue!(
        stdout,
        Print("\x1b[90m   ↑↓ 移动  Enter 确认  Esc 跳过\x1b[0m"),
        Print("\r\n"),
    )?;

    stdout.flush()?;
    Ok(())
}

/// After the selector exits, move up and clear all drawn lines.
fn clear_drawn(stdout: &mut impl Write, n: usize) -> Result<()> {
    queue!(
        stdout,
        cursor::MoveUp(drawn_lines(n)),
        cursor::MoveToColumn(0),
        terminal::Clear(ClearType::FromCursorDown),
    )?;
    stdout.flush()?;
    Ok(())
}

fn format_option(i: usize, pos: usize, opt: &SelectOption) -> String {
    let badge = opt
        .badge
        .as_deref()
        .map(|b| format!("  \x1b[96m{b}\x1b[0m"))
        .unwrap_or_default();

    if i == pos {
        // Highlighted row: cyan arrow, bold label, dim hint
        let hint = if opt.hint.is_empty() {
            String::new()
        } else {
            format!("  \x1b[90m{}\x1b[0m", opt.hint)
        };
        format!(
            "   \x1b[96m►\x1b[0m \x1b[1m{}\x1b[0m{hint}{badge}",
            opt.label
        )
    } else {
        let hint = if opt.hint.is_empty() {
            String::new()
        } else {
            format!("  \x1b[90m{}\x1b[0m", opt.hint)
        };
        format!("     \x1b[0m{}{hint}{badge}", opt.label)
    }
}

// ─── Non-TTY fallback ─────────────────────────────────────────────────────────

fn fallback_number(options: &[SelectOption], default: usize) -> Result<Option<usize>> {
    for (i, opt) in options.iter().enumerate() {
        println!("   {}  {}", i + 1, opt.label);
    }
    print!("   选择 [{}]: ", default + 1);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Some(default));
    }
    match trimmed.parse::<usize>() {
        Ok(n) if n >= 1 && n <= options.len() => Ok(Some(n - 1)),
        _ => Ok(Some(default)),
    }
}
