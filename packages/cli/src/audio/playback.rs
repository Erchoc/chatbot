use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rodio::{Decoder, OutputStream, Sink};

use crate::ui::theme::{MUTED, RESET};

/// Spawn a playback thread that receives MP3 bytes from a channel and plays them sequentially
pub fn spawn_player(
    audio_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let Ok((_stream, handle)) = OutputStream::try_default() else {
            eprintln!("   Failed to initialize audio output");
            return;
        };
        let Ok(sink) = Sink::try_new(&handle) else {
            eprintln!("   Failed to create audio player");
            return;
        };

        while let Ok(mp3_bytes) = audio_rx.recv() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let cursor = Cursor::new(mp3_bytes);
            match Decoder::new(cursor) {
                Ok(source) => sink.append(source),
                Err(e) => eprintln!("   {MUTED}MP3 decode failed: {e}{RESET}"),
            }
        }
        if !stop.load(Ordering::Relaxed) {
            sink.sleep_until_end();
        }
    })
}
