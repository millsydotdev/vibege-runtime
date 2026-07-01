//! Minimal WAV file parser.
//!
//! Supports 16-bit PCM mono/stereo WAV files at any sample rate.
//! Stereo files are mixed down to mono by averaging channels.
//! All output is converted to 44100Hz mono i16 for the audio engine.

use crate::AudioError;

/// Parsed WAV audio data.
pub struct WavData {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
}

/// Parse a WAV file from raw bytes.
/// Supports: PCM 16-bit, mono or stereo, any sample rate.
pub fn parse_wav(data: &[u8]) -> Result<WavData, AudioError> {
    if data.len() < 44 {
        return Err(AudioError::WavError("File too short".into()));
    }

    // RIFF header
    if &data[0..4] != b"RIFF" {
        return Err(AudioError::WavError("Not a RIFF file".into()));
    }
    if &data[8..12] != b"WAVE" {
        return Err(AudioError::WavError("Not a WAVE file".into()));
    }

    // Find fmt chunk
    let mut fmt_found = false;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut offset = 12;

    while offset + 8 <= data.len() {
        let chunk_id = &data[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;

        if chunk_id == b"fmt " {
            if chunk_size < 16 || offset + 8 + chunk_size > data.len() {
                return Err(AudioError::WavError("Invalid fmt chunk".into()));
            }
            let audio_format = u16::from_le_bytes([data[offset + 8], data[offset + 9]]);
            if audio_format != 1 {
                return Err(AudioError::WavError(
                    "Only PCM format supported (format=1)".into(),
                ));
            }
            channels = u16::from_le_bytes([data[offset + 10], data[offset + 11]]);
            sample_rate = u32::from_le_bytes([
                data[offset + 12],
                data[offset + 13],
                data[offset + 14],
                data[offset + 15],
            ]);
            bits_per_sample =
                u16::from_le_bytes([data[offset + 22], data[offset + 23]]);
            if channels != 1 && channels != 2 {
                return Err(AudioError::WavError("Only mono/stereo supported".into()));
            }
            if bits_per_sample != 16 {
                return Err(AudioError::WavError("Only 16-bit PCM supported".into()));
            }
            fmt_found = true;
        }

        if chunk_id == b"data" {
            if !fmt_found {
                return Err(AudioError::WavError("fmt chunk must come before data".into()));
            }
            let data_start = offset + 8;
            let data_end = (data_start + chunk_size).min(data.len());
            let raw = &data[data_start..data_end];
            let samples = samples_from_bytes(raw, channels as usize, bits_per_sample);
            return Ok(WavData { samples, sample_rate });
        }

        // Skip to next chunk (padding byte if odd size)
        let advance = 8 + chunk_size + (chunk_size % 2);
        offset += advance;
    }

    Err(AudioError::WavError("No data chunk found".into()))
}

fn samples_from_bytes(raw: &[u8], channels: usize, bits: u16) -> Vec<i16> {
    let bytes_per_sample = (bits / 8) as usize;
    let frame_size = channels * bytes_per_sample;
    let frames = raw.len() / frame_size;
    let mut samples = Vec::with_capacity(frames);

    for f in 0..frames {
        let frame_start = f * frame_size;
        let mut frame_sum = 0i32;
        for ch in 0..channels {
            let ch_start = frame_start + ch * bytes_per_sample;
            if ch_start + 2 <= raw.len() {
                let s = i16::from_le_bytes([raw[ch_start], raw[ch_start + 1]]);
                frame_sum += s as i32;
            }
        }
        // Mix stereo down to mono by averaging
        samples.push((frame_sum / channels as i32) as i16);
    }

    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_wav() {
        let result = parse_wav(b"not a wav file");
        assert!(result.is_err());
    }

    #[test]
    fn test_too_short() {
        let result = parse_wav(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_and_parse_mono() {
        // Build a minimal valid WAV: 16-bit PCM mono, 44100Hz, 1 second
        let sample_count = 44100usize;
        let data_size = sample_count * 2;
        let mut wav = Vec::new();
        // RIFF header
        wav.extend(b"RIFF");
        wav.extend(&((36 + data_size) as u32).to_le_bytes());
        wav.extend(b"WAVE");
        // fmt chunk
        wav.extend(b"fmt ");
        wav.extend(&16u32.to_le_bytes()); // chunk size
        wav.extend(&1u16.to_le_bytes());  // PCM
        wav.extend(&1u16.to_le_bytes());  // mono
        wav.extend(&44100u32.to_le_bytes()); // sample rate
        wav.extend(&(88200u32).to_le_bytes()); // byte rate
        wav.extend(&2u16.to_le_bytes());  // block align
        wav.extend(&16u16.to_le_bytes()); // bits per sample
        // data chunk
        wav.extend(b"data");
        wav.extend(&(data_size as u32).to_le_bytes());
        // Generate a 440Hz sine wave
        for i in 0..sample_count {
            let sample = (f32::sin(2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0)
                * 16000.0) as i16;
            wav.extend(&sample.to_le_bytes());
        }

        let result = parse_wav(&wav);
        assert!(result.is_ok());
        let wav_data = result.unwrap();
        assert_eq!(wav_data.sample_rate, 44100);
        assert_eq!(wav_data.samples.len(), sample_count);
    }

    #[test]
    fn test_generate_and_parse_stereo() {
        let sample_count = 44100usize;
        let data_size = sample_count * 4; // 2 channels * 2 bytes
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend(&((36 + data_size) as u32).to_le_bytes());
        wav.extend(b"WAVE");
        wav.extend(b"fmt ");
        wav.extend(&16u32.to_le_bytes());
        wav.extend(&1u16.to_le_bytes());
        wav.extend(&2u16.to_le_bytes()); // stereo
        wav.extend(&44100u32.to_le_bytes());
        wav.extend(&(176400u32).to_le_bytes());
        wav.extend(&4u16.to_le_bytes());
        wav.extend(&16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(&(data_size as u32).to_le_bytes());
        for i in 0..sample_count {
            let s = (f32::sin(2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0)
                * 16000.0) as i16;
            wav.extend(&s.to_le_bytes()); // left
            wav.extend(&s.to_le_bytes()); // right
        }

        let result = parse_wav(&wav);
        assert!(result.is_ok());
        let wav_data = result.unwrap();
        assert_eq!(wav_data.samples.len(), sample_count); // mixed to mono
    }
}
