//! 多声道混音 + 抗混叠降采样到 16kHz，以及 WAV 编码。
use super::{DeviceInfo, TARGET_RATE};

/// 一块音频的平均绝对幅度（VAD 用的"能量"）。
pub fn chunk_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|s| s.abs()).sum::<f32>() / samples.len() as f32
}

/// 混成单声道。
pub fn mix_to_mono(input: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return input.to_vec();
    }
    let n = channels as usize;
    input.chunks(n).map(|frame| frame.iter().sum::<f32>() / n as f32).collect()
}

/// 混音 + 降采样到 `TARGET_RATE`。
///
/// 之前是纯线性插值，48k → 16k 时 8kHz 以上的齿音 / 键盘噪声会折叠进语音频带，
/// ASR 听到的是"带毛刺"的音频。现在每个输出样本取源信号对应区间的**面积平均**
/// （等价于一个 box 低通再抽取），便宜且足够压掉混叠。
pub fn downsample_to_mono_16k(input: &[f32], info: DeviceInfo) -> Vec<f32> {
    let mono = mix_to_mono(input, info.channels);
    resample_box(&mono, info.sample_rate, TARGET_RATE)
}

/// 面积平均重采样（仅用于降采样；升采样退化为线性插值）。
pub fn resample_box(mono: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || mono.is_empty() {
        return mono.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (mono.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);

    if ratio < 1.0 {
        for i in 0..out_len {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = mono[idx.min(mono.len() - 1)];
            let b = mono[(idx + 1).min(mono.len() - 1)];
            out.push(a * (1.0 - frac) + b * frac);
        }
        return out;
    }

    for i in 0..out_len {
        let start = i as f64 * ratio;
        let end = start + ratio;
        let mut acc = 0.0_f64;
        let mut weight = 0.0_f64;
        let mut j = start.floor() as usize;
        while (j as f64) < end && j < mono.len() {
            let seg_start = start.max(j as f64);
            let seg_end = end.min(j as f64 + 1.0);
            let w = seg_end - seg_start;
            if w > 0.0 {
                acc += mono[j] as f64 * w;
                weight += w;
            }
            j += 1;
        }
        out.push(if weight > 0.0 { (acc / weight) as f32 } else { 0.0 });
    }
    out
}

/// f32 样本 → 16kHz 单声道 16bit WAV。
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
        writer.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    writer.finalize()?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize)
            .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn keeps_speech_band_and_attenuates_aliasing_band() {
        // 1kHz（语音频带）48k→16k 后能量基本不变
        let low = resample_box(&tone(48_000, 1_000.0, 0.5), 48_000, 16_000);
        assert!((rms(&low) - 0.707).abs() < 0.05, "1kHz rms={}", rms(&low));
        // 15kHz（会折叠到 1kHz 的混叠带）被明显压掉
        let high = resample_box(&tone(48_000, 15_000.0, 0.5), 48_000, 16_000);
        assert!(rms(&high) < 0.25, "15kHz rms={} should be attenuated", rms(&high));
    }

    #[test]
    fn output_length_matches_ratio() {
        let out = resample_box(&vec![0.0; 44_100], 44_100, 16_000);
        assert_eq!(out.len(), 16_000);
        assert_eq!(resample_box(&[1.0, 2.0], 16_000, 16_000), vec![1.0, 2.0]);
    }

    #[test]
    fn stereo_mixes_to_mono() {
        assert_eq!(mix_to_mono(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
    }
}
