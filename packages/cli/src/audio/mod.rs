pub mod capture;
pub mod playback;
pub mod resample;

/// Audio device info
#[derive(Clone, Copy, Debug)]
pub struct DeviceInfo {
    pub sample_rate: u32,
    pub channels: u16,
}

/// Target sample rate for ASR
pub const TARGET_RATE: u32 = 16000;
