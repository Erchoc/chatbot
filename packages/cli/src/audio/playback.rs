//! 扬声器播放队列：按顺序播 mp3，可被立刻叫停。
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::{Decoder, OutputStream, Sink};

use crate::ui::theme::{MUTED, RESET};

/// 一段要播的音频，附带它对应的文本（开播时回调给 UI 打印）。
pub struct PlayItem {
    pub text: String,
    pub audio: Vec<u8>,
}

/// 播放线程共享的状态。
#[derive(Clone, Default)]
pub struct PlayerState {
    /// 置位后立刻停播并退出（打断）。
    pub stop: Arc<AtomicBool>,
    /// 当前是否真的在出声（句与句之间为 false）。
    pub playing: Arc<AtomicBool>,
    /// 已经开始播放（对用户"说出口"）的句子。
    pub spoken: Arc<Mutex<Vec<String>>>,
}

impl PlayerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spoken_text(&self) -> String {
        self.spoken.lock().map(|v| v.join("")).unwrap_or_default()
    }
}

/// 起一个播放线程：从队列取 [`PlayItem`] 顺序播放；每句开播前调 `on_start(text)`。
/// `state.stop` 置位后 20ms 内停止。
pub fn spawn_player<F>(
    rx: std::sync::mpsc::Receiver<PlayItem>,
    state: PlayerState,
    on_start: F,
) -> std::thread::JoinHandle<()>
where
    F: Fn(&str) + Send + 'static,
{
    std::thread::spawn(move || {
        let Ok((_stream, handle)) = OutputStream::try_default() else {
            eprintln!("   Failed to initialize audio output");
            return;
        };
        let Ok(sink) = Sink::try_new(&handle) else {
            eprintln!("   Failed to create audio player");
            return;
        };

        while let Ok(item) = rx.recv() {
            if state.stop.load(Ordering::Relaxed) {
                break;
            }
            on_start(&item.text);
            if let Ok(mut v) = state.spoken.lock() {
                v.push(item.text.clone());
            }
            match Decoder::new(Cursor::new(item.audio)) {
                Ok(source) => sink.append(source),
                Err(e) => {
                    eprintln!("   {MUTED}MP3 decode failed: {e}{RESET}");
                    continue;
                }
            }
            state.playing.store(true, Ordering::Relaxed);
            while !sink.empty() {
                if state.stop.load(Ordering::Relaxed) {
                    sink.stop();
                    state.playing.store(false, Ordering::Relaxed);
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            state.playing.store(false, Ordering::Relaxed);
        }
        state.playing.store(false, Ordering::Relaxed);
    })
}
