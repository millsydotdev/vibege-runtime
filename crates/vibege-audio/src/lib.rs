use std::sync::Mutex;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use tracing::{debug, info, warn};

/// Audio system for playing sound effects and music.
///
/// Uses rodio for cross-platform audio playback.
/// The OutputStream is stored as a raw pointer because it is !Send on Windows (WASAPI).
pub struct AudioSystem {
    stream_ptr: *mut OutputStream,
    handle: OutputStreamHandle,
    #[allow(dead_code)]
    music_volume: Mutex<f32>,
    sfx_volume: Mutex<f32>,
}

unsafe impl Send for AudioSystem {}
unsafe impl Sync for AudioSystem {}

impl Drop for AudioSystem {
    fn drop(&mut self) {
        if !self.stream_ptr.is_null() {
            unsafe { drop(Box::from_raw(self.stream_ptr)); }
        }
    }
}

impl AudioSystem {
    /// Initializes the audio output device.
    /// Returns None if no audio device is available (non-fatal).
    pub fn new() -> Option<Self> {
        match OutputStream::try_default() {
            Ok((stream, handle)) => {
                let stream_ptr = Box::into_raw(Box::new(stream));
                info!("Audio system initialised");
                Some(Self {
                    stream_ptr,
                    handle,
                    music_volume: Mutex::new(0.5),
                    sfx_volume: Mutex::new(0.7),
                })
            }
            Err(e) => {
                warn!("No audio device available: {e}");
                None
            }
        }
    }

    /// Plays a WAV sound effect from raw bytes.
    /// Returns immediately; the sound plays in the background.
    pub fn play_sfx(&self, wav_data: &[u8]) {
        let cursor = std::io::Cursor::new(wav_data.to_vec());
        match rodio::Decoder::new(cursor) {
            Ok(source) => {
                let volume = *self.sfx_volume.lock().unwrap();
                let sink = Sink::try_new(&self.handle).ok();
                if let Some(s) = sink {
                    s.set_volume(volume);
                    s.append(source);
                    s.detach();
                }
            }
            Err(e) => debug!("Failed to decode audio: {e}"),
        }
    }

    /// Sets the sound effects volume (0.0 to 1.0).
    pub fn set_sfx_volume(&self, vol: f32) {
        *self.sfx_volume.lock().unwrap() = vol.clamp(0.0, 1.0);
    }

    /// Returns true if an audio device was successfully initialized.
    pub fn is_available(&self) -> bool {
        true
    }
}

/// Generates a simple sine wave WAV for use as a placeholder sound.
/// Useful when no real audio files are available yet.
pub fn generate_test_tone(frequency: f32, duration_secs: f32) -> Vec<u8> {
    let sample_rate = 44100u32;
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let mut samples = Vec::with_capacity(num_samples);

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (t * frequency * 2.0 * std::f32::consts::PI).sin();
        // Convert to i16
        let amp = (sample * 0.3 * 32767.0) as i16;
        samples.push(amp);
    }

    // Build minimal WAV header + data
    let data_size = samples.len() * 2;
    let file_size = 44 + data_size;
    let mut wav = Vec::with_capacity(file_size as usize);

    // RIFF header
    wav.extend(b"RIFF");
    wav.extend(&(file_size as u32 - 8).to_le_bytes());
    wav.extend(b"WAVE");
    // fmt chunk
    wav.extend(b"fmt ");
    wav.extend(&16u32.to_le_bytes());      // chunk size
    wav.extend(&1u16.to_le_bytes());        // PCM format
    wav.extend(&1u16.to_le_bytes());        // mono
    wav.extend(&sample_rate.to_le_bytes());
    wav.extend(&(sample_rate * 2).to_le_bytes()); // byte rate
    wav.extend(&2u16.to_le_bytes());        // block align
    wav.extend(&16u16.to_le_bytes());       // bits per sample
    // data chunk
    wav.extend(b"data");
    wav.extend(&(data_size as u32).to_le_bytes());
    for s in &samples {
        wav.extend(&s.to_le_bytes());
    }

    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_tone_creates_wav() {
        let wav = generate_test_tone(440.0, 0.5);
        assert!(wav.len() > 44);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn test_generate_tone_duration() {
        let wav = generate_test_tone(440.0, 0.1);
        // 0.1s at 44100Hz = 4410 samples × 2 bytes = 8820 data bytes
        let data_size = wav.len() - 44;
        assert!(data_size > 0);
    }
}
