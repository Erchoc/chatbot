use super::{DeviceInfo, TARGET_RATE};

/// Calculate mean absolute amplitude of an audio chunk (used for VAD)
pub fn chunk_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|s| s.abs()).sum::<f32>() / samples.len() as f32
}

/// Mix multi-channel to mono + linear-interpolation downsample to TARGET_RATE
pub fn downsample_to_mono_16k(input: &[f32], info: DeviceInfo) -> Vec<f32> {
    // 1. Mix to mono
    let mono: Vec<f32> = if info.channels > 1 {
        input
            .chunks(info.channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / info.channels as f32)
            .collect()
    } else {
        input.to_vec()
    };

    // 2. Resample
    if info.sample_rate == TARGET_RATE {
        return mono;
    }

    let ratio = info.sample_rate as f64 / TARGET_RATE as f64;
    let out_len = (mono.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;

        let sample = if idx + 1 < mono.len() {
            mono[idx] * (1.0 - frac) + mono[idx + 1] * frac
        } else {
            mono[idx.min(mono.len().saturating_sub(1))]
        };
        output.push(sample);
    }

    output
}

/// Encode f32 samples to WAV (16kHz mono 16-bit)
pub fn encode_wav(samples: &[f32]) -> anyhow::Result<Vec<u8>> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
    for &s in samples {
        writer.write_sample((s * 32767.0) as i16)?;
    }
    writer.finalize()?;
    Ok(cursor.into_inner())
}
