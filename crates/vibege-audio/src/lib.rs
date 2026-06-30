use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::sync::Mutex;
use tracing::{info, warn};

/// Audio system for playing sound effects and music.
///
/// Uses rodio for cross-platform audio playback.
pub struct AudioSystem {
    #[allow(dead_code)]
    stream: OutputStream,
    handle: OutputStreamHandle,
    #[allow(dead_code)]
    music_volume: Mutex<f32>,
    sfx_volume: Mutex<f32>,
}

impl AudioSystem {
    /// Initializes the audio output device.
    /// Returns None if no audio device is available (non-fatal).
    pub fn new() -> Option<Self> {
        match OutputStream::try_default() {
            Ok((stream, handle)) => {
                info!("Audio system initialised");
                Some(Self {
                    stream,
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

    /// Play a pre-loaded sound effect.
    pub fn play_sfx(&self, data: &[i16]) {
        let format = rodio::buffer::SamplesBuffer::new(1, 44100, data.to_vec());
        if let Ok(sink) = Sink::try_new(&self.handle) {
            sink.append(format);
            sink.detach();
        }
    }

    /// Set the music volume (0.0 – 1.0).
    pub fn set_music_volume(&self, vol: f32) {
        *self.music_volume.lock().expect("lock") = vol.clamp(0.0, 1.0);
    }

    /// Set the sound effect volume (0.0 – 1.0).
    pub fn set_sfx_volume(&self, vol: f32) {
        *self.sfx_volume.lock().expect("lock") = vol.clamp(0.0, 1.0);
    }
}

/// Generate a test tone as an i16 sample buffer.
/// Generates a sine wave at the given frequency and duration.
pub fn generate_test_tone(frequency: f32, duration_secs: f32) -> Vec<i16> {
    let sample_rate = 44100u32;
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let mut samples = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (t * frequency * 2.0 * std::f32::consts::PI).sin();
        samples.push((sample * i16::MAX as f32) as i16);
    }
    samples
}
