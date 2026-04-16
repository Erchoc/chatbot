use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::theme::*;

/// An animated spinner that runs on a background thread.
/// Call `stop()` to halt and clear the animation.
pub struct Spinner {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Spinner {
    /// Start a multi-line frame animation (e.g. robot face).
    /// Frames must NOT have leading newlines. Each frame must have the same line count.
    pub fn start_frames(frames: Vec<String>) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();

        let handle = thread::spawn(move || {
            let mut idx = 0;
            let mut printed_lines = 0_usize;

            while r.load(Ordering::Relaxed) {
                let frame = &frames[idx % frames.len()];

                // Clear previous frame by moving cursor up
                if printed_lines > 0 {
                    for _ in 0..printed_lines {
                        print!("\x1b[A\x1b[2K");
                    }
                }

                // Print new frame
                let lines: Vec<&str> = frame.lines().filter(|l| !l.is_empty()).collect();
                for line in &lines {
                    println!("{line}");
                }
                std::io::stdout().flush().ok();
                printed_lines = lines.len();

                thread::sleep(Duration::from_millis(350));
                idx += 1;
            }

            // Clear on exit
            if printed_lines > 0 {
                for _ in 0..printed_lines {
                    print!("\x1b[A\x1b[2K");
                }
                std::io::stdout().flush().ok();
            }
        });

        Self {
            running,
            handle: Some(handle),
        }
    }

    /// Start a simple inline spinner: ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
    pub fn start_inline(message: &str, color: &str) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        let msg = message.to_string();
        let col = color.to_string();

        let handle = thread::spawn(move || {
            let dots = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut idx = 0;
            while r.load(Ordering::Relaxed) {
                let dot = dots[idx % dots.len()];
                print!("\r   {col}{dot} {msg}{RESET}  ");
                std::io::stdout().flush().ok();
                thread::sleep(Duration::from_millis(80));
                idx += 1;
            }
            // Clear the spinner line
            print!("\r\x1b[2K");
            std::io::stdout().flush().ok();
        });

        Self {
            running,
            handle: Some(handle),
        }
    }

    /// Stop the spinner and clean up
    pub fn stop(mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    /// Stop and print a completion message on the same line
    pub fn stop_with(mut self, msg: &str) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        println!("{msg}");
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
