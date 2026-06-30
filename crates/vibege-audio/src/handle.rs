//! Playback handles — control active sounds after they are created.
//!
//! A `PlaybackHandle` lets the caller stop, pause, resume, set looping,
//! and adjust volume on a sound that is already playing. The handle is
//! backed by the mixer's active sound list. When the sound finishes
//! naturally, the handle becomes invalid — operations silently no-op.

use std::sync::Arc;

use crate::mixer::Mixer;

/// The current state of a playback handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    /// Sound is currently playing.
    Playing,
    /// Sound has been paused.
    Paused,
    /// Sound has finished or was stopped.
    Stopped,
}

/// A handle to an actively playing sound.
///
/// Handles are created by `AudioSystem` and can be used to control
/// playback. Drop the handle to stop tracking the sound (it continues
/// playing — use `stop()` to actually stop it).
pub struct PlaybackHandle {
    pub(crate) id: u64,
    pub(crate) mixer: Arc<Mixer>,
}

impl std::fmt::Debug for PlaybackHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaybackHandle")
            .field("id", &self.id)
            .finish()
    }
}

impl PlaybackHandle {
    /// Create a new handle. Internal — called by AudioSystem.
    pub(crate) fn new(id: u64, mixer: Arc<Mixer>) -> Self {
        Self { id, mixer }
    }

    /// Stop the sound immediately.
    /// After this, the handle is no longer valid.
    pub fn stop(&self) {
        self.mixer.stop(self.id);
    }

    /// Pause the sound.
    pub fn pause(&self) {
        self.mixer.with_sink(self.id, |sink| {
            sink.pause();
        });
    }

    /// Resume a paused sound.
    pub fn resume(&self) {
        self.mixer.with_sink(self.id, |sink| {
            sink.play();
        });
    }

    /// Set whether the sound loops. Looping sounds replay from the
    /// beginning when they reach the end.
    ///
    /// Internally this recreates the audio source with or without rodio's
    /// `.repeat_infinite()` adapter. The sound restarts when looping is
    /// toggled.
    pub fn set_looping(&self, looping: bool) {
        self.mixer.set_looping(self.id, looping);
    }

    /// Query whether the sound is currently set to loop.
    pub fn is_looping(&self) -> bool {
        self.mixer.is_looping(self.id)
    }

    /// Set the per-sound volume (0.0 – 1.0).
    /// This is multiplied with the channel and master volume.
    pub fn set_volume(&self, volume: f32) {
        let vol = volume.clamp(0.0, 1.0);
        self.mixer.with_sink(self.id, |sink| {
            sink.set_volume(vol);
        });
    }

    /// Query the current playback state.
    pub fn state(&self) -> PlaybackState {
        let mut state = PlaybackState::Stopped;
        self.mixer.with_sink(self.id, |sink| {
            if sink.is_paused() {
                state = PlaybackState::Paused;
            } else if !sink.empty() {
                state = PlaybackState::Playing;
            } else {
                state = PlaybackState::Stopped;
            }
        });
        state
    }

    /// Is this handle still valid? (i.e., the sound is still alive)
    pub fn is_valid(&self) -> bool {
        let mut valid = false;
        self.mixer.with_sink(self.id, |sink| {
            valid = !sink.empty();
        });
        valid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Note: Most handle tests require a real rodio device, so we test
    // only the logic that doesn't need hardware (state enum, creation).

    #[test]
    fn test_playback_state_equality() {
        assert_eq!(PlaybackState::Playing, PlaybackState::Playing);
        assert_eq!(PlaybackState::Paused, PlaybackState::Paused);
        assert_eq!(PlaybackState::Stopped, PlaybackState::Stopped);
        assert_ne!(PlaybackState::Playing, PlaybackState::Stopped);
    }

    #[test]
    fn test_playback_state_debug() {
        assert_eq!(format!("{:?}", PlaybackState::Playing), "Playing");
        assert_eq!(format!("{:?}", PlaybackState::Paused), "Paused");
        assert_eq!(format!("{:?}", PlaybackState::Stopped), "Stopped");
    }

    #[test]
    fn test_handle_invalid_when_stopped() {
        let mixer = Arc::new(Mixer::new(Box::new(|| None)));
        let handle = PlaybackHandle::new(999, Arc::clone(&mixer));
        // ID 999 doesn't exist, so is_valid should be false
        assert!(!handle.is_valid());
    }

    #[test]
    fn test_handle_state_when_invalid() {
        let mixer = Arc::new(Mixer::new(Box::new(|| None)));
        let handle = PlaybackHandle::new(999, Arc::clone(&mixer));
        assert_eq!(handle.state(), PlaybackState::Stopped);
    }

    #[test]
    fn test_handle_stop_invalid() {
        let mixer = Arc::new(Mixer::new(Box::new(|| None)));
        let handle = PlaybackHandle::new(999, Arc::clone(&mixer));
        handle.stop(); // should not panic
    }

    #[test]
    fn test_handle_pause_invalid() {
        let mixer = Arc::new(Mixer::new(Box::new(|| None)));
        let handle = PlaybackHandle::new(999, Arc::clone(&mixer));
        handle.pause(); // should not panic
    }

    #[test]
    fn test_handle_resume_invalid() {
        let mixer = Arc::new(Mixer::new(Box::new(|| None)));
        let handle = PlaybackHandle::new(999, Arc::clone(&mixer));
        handle.resume(); // should not panic
    }

    #[test]
    fn test_handle_set_looping_invalid() {
        let mixer = Arc::new(Mixer::new(Box::new(|| None)));
        let handle = PlaybackHandle::new(999, Arc::clone(&mixer));
        handle.set_looping(true); // should not panic
    }

    #[test]
    fn test_handle_set_volume_invalid() {
        let mixer = Arc::new(Mixer::new(Box::new(|| None)));
        let handle = PlaybackHandle::new(999, Arc::clone(&mixer));
        handle.set_volume(0.5); // should not panic
    }

    #[test]
    fn test_handle_clamp_volume() {
        let mixer = Arc::new(Mixer::new(Box::new(|| None)));
        let handle = PlaybackHandle::new(999, Arc::clone(&mixer));
        handle.set_volume(1.5); // clamped to 1.0, should not panic
        handle.set_volume(-0.5); // clamped to 0.0, should not panic
    }

    #[test]
    fn test_handle_debug() {
        let mixer = Arc::new(Mixer::new(Box::new(|| None)));
        let handle = PlaybackHandle::new(1, mixer);
        let debug = format!("{:?}", handle);
        assert!(debug.contains("PlaybackHandle"));
        assert!(debug.contains("id: 1"));
    }
}
