//! VibeGE Audio Engine — game audio framework built on rodio.
//!
//! # Architecture
//!
//! The audio system is split into four subsystems:
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                   AudioSystem                    │
//! │  • owns OutputStream (audio device)             │
//! │  • routes all playback through the Mixer        │
//! │  • manages SoundCache for loaded assets         │
//! └──────┬──────────────┬───────────────┬───────────┘
//!        │              │               │
//!        ▼              ▼               ▼
//! ┌───────────┐ ┌────────────┐ ┌──────────────┐
//! │   Mixer   │ │ SoundCache │ │ PlaybackHndl │
//! │ • Master  │ │ • dedup    │ │ • stop       │
//! │ • Music   │ │ • lazy     │ │ • pause      │
//! │ • SFX     │ │ • stats    │ │ • resume     │
//! │ • UI      │ │            │ │ • looping    │
//! │ • Ambient │ │            │ │ • volume     │
//! │ • Voice   │ │            │ │ • state      │
//! └───────────┘ └────────────┘ └──────────────┘
//! ```
//!
//! # Mixer Channels
//!
//! All playback is routed through a channel. Each channel has independent
//! volume and mute state. Changing a channel's volume immediately affects
//! all currently playing sounds on that channel.
//!
//! # Playback Lifecycle
//!
//! 1. **Load** — Sound data is loaded into the cache (dedup by key).
//! 2. **Play** — A new `Sink` is created on the target channel with the
//!    channel's current volume. A `PlaybackHandle` is returned.
//! 3. **Control** — The handle can stop, pause, resume, set looping, or
//!    change volume directly on the sound.
//! 4. **Complete** — When the sound finishes, it is removed from the
//!    active-sounds list. The handle becomes invalid.
//!
//! # Error Model
//!
//! Audio failures never crash the runtime. `AudioSystem::new()` returns
//! `None` when no device is available. All playback methods return
//! `Result` types that the caller can safely ignore or log.
//!
//! # Thread Safety
//!
//! `AudioSystem` is `Send + Sync`. All internal state is behind `Mutex`.
//! rodio's `Sink` and `OutputStreamHandle` are both `Send`.

mod engine;
mod handle;
mod mixer;
mod sound_cache;
mod wav;

pub use engine::AudioSystem;
pub use handle::{PlaybackHandle, PlaybackState};
pub use mixer::{ChannelKind, Mixer};
pub use sound_cache::{SoundCache, SoundData};
pub use wav::parse_wav;

use thiserror::Error;

/// Errors that can occur during audio operations.
#[derive(Debug, Error)]
pub enum AudioError {
    #[error("Audio device not available")]
    NoDevice,
    #[error("Failed to create playback sink: {0}")]
    SinkFailed(String),
    #[error("Sound not found in cache: {0}")]
    SoundNotFound(String),
    #[error("Playback handle is no longer valid")]
    InvalidHandle,
    #[error("Unsupported audio format: {0}")]
    UnsupportedFormat(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("WAV error: {0}")]
    WavError(String),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_test_tone_length() {
        let tone = generate_test_tone(440.0, 1.0);
        assert_eq!(tone.len(), 44100);
    }

    #[test]
    fn test_generate_test_tone_frequency() {
        let tone = generate_test_tone(440.0, 0.1);
        assert_eq!(tone.len(), 4410);

        let _mid = tone.len() / 2;
        // Not silenced (rough sanity — amplitude should be non-zero)
        let amplitude: f64 =
            tone.iter().map(|&s| (s as f64).abs()).sum::<f64>() / tone.len() as f64;
        assert!(amplitude > 1000.0, "Expected non-zero amplitude");
    }

    #[test]
    fn test_audio_error_display() {
        let err = AudioError::NoDevice;
        assert_eq!(err.to_string(), "Audio device not available");

        let err = AudioError::SinkFailed("oops".into());
        assert!(err.to_string().contains("oops"));
    }

    #[test]
    fn test_generate_test_tone_different_frequencies() {
        let low = generate_test_tone(220.0, 0.01);
        let high = generate_test_tone(880.0, 0.01);
        assert_eq!(low.len(), high.len());
        // Different frequencies should produce different samples
        assert_ne!(low, high);
    }
}
