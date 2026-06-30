//! Audio mixer with independent channel control.
//!
//! The mixer routes all playback through named channels. Each channel has
//! independent volume and mute state. Changing a channel's volume
//! immediately affects all currently playing sounds on that channel.
//!
//! # Locking Order
//!
//! To avoid deadlocks, always acquire `channels` before `active` and never
//! hold both locks across a function boundary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rodio::{Sink, Source};

/// Logical audio channels.
///
/// Each channel has independent volume and mute. `Master` affects
/// all channels — its volume is multiplied with each sub-channel's volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    Master,
    Music,
    Sfx,
    Ui,
    Ambient,
    Voice,
}

impl ChannelKind {
    /// Returns all available sub-channels (excluding Master).
    pub const fn all_sub() -> [ChannelKind; 5] {
        [
            ChannelKind::Music,
            ChannelKind::Sfx,
            ChannelKind::Ui,
            ChannelKind::Ambient,
            ChannelKind::Voice,
        ]
    }
}

/// Per-channel state.
#[derive(Debug, Clone)]
struct ChannelState {
    volume: f32,
    muted: bool,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
        }
    }
}

/// Handle to an active sound, stored inside the mixer.
pub(crate) struct ActiveSound {
    pub(crate) id: u64,
    pub(crate) channel: ChannelKind,
    pub(crate) sink: Sink,
    pub(crate) data: Arc<Vec<i16>>,
    pub(crate) looping: bool,
}

impl std::fmt::Debug for ActiveSound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveSound")
            .field("id", &self.id)
            .field("channel", &self.channel)
            .finish()
    }
}

/// The audio mixer.
///
/// Manages channel state and active sounds. Thread-safe via internal mutex.
pub struct Mixer {
    channels: Mutex<HashMap<ChannelKind, ChannelState>>,
    active: Mutex<Vec<ActiveSound>>,
    next_id: Mutex<u64>,
    sink_factory: Box<dyn Fn() -> Option<Sink> + Send + Sync>,
}

impl Mixer {
    /// Create a new mixer with default settings.
    /// Takes a sink factory — a closure that creates new rodio Sinks.
    pub fn new(sink_factory: Box<dyn Fn() -> Option<Sink> + Send + Sync>) -> Self {
        let mut channels = HashMap::new();
        channels.insert(ChannelKind::Master, ChannelState::default());
        for ch in ChannelKind::all_sub() {
            channels.insert(ch, ChannelState::default());
        }
        Self {
            channels: Mutex::new(channels),
            active: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
            sink_factory,
        }
    }

    // ── Channel control ─────────────────────────────────────────────

    /// Set a channel's volume (0.0 – 1.0).
    /// Returns the previous volume.
    pub fn set_volume(&self, channel: ChannelKind, volume: f32) -> f32 {
        let vol = volume.clamp(0.0, 1.0);
        let mut channels = self.channels.lock().expect("mixer lock");
        let state = channels.get_mut(&channel).expect("unknown channel");
        let prev = state.volume;
        state.volume = vol;
        // Drop channels lock before updating active sound volumes
        drop(channels);

        self.refresh_active_volumes();
        prev
    }

    /// Get a channel's current volume.
    pub fn volume(&self, channel: ChannelKind) -> f32 {
        let channels = self.channels.lock().expect("mixer lock");
        channels.get(&channel).map(|s| s.volume).unwrap_or(1.0)
    }

    /// Mute or unmute a channel.
    /// When muted, the channel's effective volume is 0.
    pub fn set_mute(&self, channel: ChannelKind, muted: bool) {
        let mut channels = self.channels.lock().expect("mixer lock");
        if let Some(state) = channels.get_mut(&channel) {
            state.muted = muted;
        }
        drop(channels);
        self.refresh_active_volumes();
    }

    /// Is a channel muted?
    pub fn is_muted(&self, channel: ChannelKind) -> bool {
        let channels = self.channels.lock().expect("mixer lock");
        channels.get(&channel).map(|s| s.muted).unwrap_or(false)
    }

    /// Snapshot all channel volumes (considering mute).
    /// Returns a map of channel → effective volume.
    fn channel_volumes(&self) -> HashMap<ChannelKind, f32> {
        let channels = self.channels.lock().expect("mixer lock");
        let master_muted = channels
            .get(&ChannelKind::Master)
            .map(|s| s.muted)
            .unwrap_or(false);
        let master_vol = if master_muted {
            0.0
        } else {
            channels
                .get(&ChannelKind::Master)
                .map(|s| s.volume)
                .unwrap_or(1.0)
        };

        let mut result = HashMap::new();
        result.insert(ChannelKind::Master, master_vol);
        for (kind, state) in channels.iter() {
            if *kind == ChannelKind::Master {
                continue;
            }
            let ch_vol = if state.muted { 0.0 } else { state.volume };
            result.insert(*kind, master_vol * ch_vol);
        }
        result
    }

    /// Update all active sound sink volumes to reflect current channel settings.
    fn refresh_active_volumes(&self) {
        let vol_map = self.channel_volumes();
        let active = self.active.lock().expect("mixer lock");
        for sound in active.iter() {
            if let Some(&effective) = vol_map.get(&sound.channel) {
                sound.sink.set_volume(effective);
            }
        }
    }

    // ── Active sound management ─────────────────────────────────────

    /// Register a new active sound. Returns a unique ID.
    pub(crate) fn register(&self, channel: ChannelKind, sink: Sink, data: Arc<Vec<i16>>) -> u64 {
        let vol_map = self.channel_volumes();
        let vol = vol_map.get(&channel).copied().unwrap_or(1.0);
        sink.set_volume(vol);

        let mut active = self.active.lock().expect("mixer lock");
        let mut id_gen = self.next_id.lock().expect("mixer lock");
        let id = *id_gen;
        *id_gen += 1;

        active.push(ActiveSound {
            id,
            channel,
            sink,
            data,
            looping: false,
        });
        id
    }

    /// Find an active sound by ID and apply a closure to its sink.
    pub(crate) fn with_sink<F>(&self, id: u64, f: F)
    where
        F: FnOnce(&Sink),
    {
        let mut active = self.active.lock().expect("mixer lock");
        if let Some(sound) = active.iter_mut().find(|s| s.id == id) {
            f(&sound.sink);
        }
    }

    /// Remove finished sounds and return the count removed.
    pub fn cleanup(&self) -> usize {
        let mut active = self.active.lock().expect("mixer lock");
        let before = active.len();
        active.retain(|s| !s.sink.empty());
        before - active.len()
    }

    /// Stop a specific active sound.
    pub fn stop(&self, id: u64) {
        let mut active = self.active.lock().expect("mixer lock");
        if let Some(pos) = active.iter().position(|s| s.id == id) {
            active[pos].sink.stop();
            active.remove(pos);
        }
    }

    /// Stop all sounds on a channel.
    pub fn stop_channel(&self, channel: ChannelKind) {
        let mut active = self.active.lock().expect("mixer lock");
        active.retain(|s| {
            if s.channel == channel {
                s.sink.stop();
                false
            } else {
                true
            }
        });
    }

    /// Stop all sounds.
    pub fn stop_all(&self) {
        let mut active = self.active.lock().expect("mixer lock");
        for sound in active.iter() {
            sound.sink.stop();
        }
        active.clear();
    }

    /// Number of currently active (playing) sounds.
    pub fn active_count(&self) -> usize {
        let active = self.active.lock().expect("mixer lock");
        active.len()
    }

    /// Set whether a sound should loop.
    /// Recreates the internal sink with or without `.repeat_infinite()`.
    pub(crate) fn set_looping(&self, id: u64, looping: bool) {
        let mut active = self.active.lock().expect("mixer lock");
        if let Some(sound) = active.iter_mut().find(|s| s.id == id) {
            if sound.looping == looping {
                return;
            }
            sound.looping = looping;
            sound.sink.stop();
            if let Some(new_sink) = (self.sink_factory)() {
                let format = rodio::buffer::SamplesBuffer::new(1, 44100, (*sound.data).clone());
                if looping {
                    new_sink.append(format.repeat_infinite());
                } else {
                    new_sink.append(format);
                }
                sound.sink = new_sink;
            }
        }
    }

    /// Query whether a sound is set to loop.
    pub(crate) fn is_looping(&self, id: u64) -> bool {
        let active = self.active.lock().expect("mixer lock");
        active
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.looping)
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for Mixer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mixer")
            .field("active_count", &self.active_count())
            .finish()
    }
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new(Box::new(|| None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_mixer() -> Mixer {
        Mixer::new(Box::new(|| None))
    }

    #[test]
    fn test_mixer_initial_volumes() {
        let mixer = test_mixer();
        assert!((mixer.volume(ChannelKind::Master) - 1.0).abs() < 1e-6);
        assert!((mixer.volume(ChannelKind::Music) - 1.0).abs() < 1e-6);
        assert!((mixer.volume(ChannelKind::Sfx) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_mixer_set_volume() {
        let mixer = test_mixer();
        let prev = mixer.set_volume(ChannelKind::Music, 0.5);
        assert!((prev - 1.0).abs() < 1e-6);
        assert!((mixer.volume(ChannelKind::Music) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_mixer_volume_clamping() {
        let mixer = test_mixer();
        mixer.set_volume(ChannelKind::Sfx, 2.5);
        assert!((mixer.volume(ChannelKind::Sfx) - 1.0).abs() < 1e-6);
        mixer.set_volume(ChannelKind::Sfx, -0.5);
        assert!((mixer.volume(ChannelKind::Sfx) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_mixer_mute() {
        let mixer = test_mixer();
        assert!(!mixer.is_muted(ChannelKind::Sfx));
        mixer.set_mute(ChannelKind::Sfx, true);
        assert!(mixer.is_muted(ChannelKind::Sfx));
        mixer.set_mute(ChannelKind::Sfx, false);
        assert!(!mixer.is_muted(ChannelKind::Sfx));
    }

    #[test]
    fn test_mixer_set_volume_returns_previous() {
        let mixer = test_mixer();
        mixer.set_volume(ChannelKind::Music, 0.3);
        let prev = mixer.set_volume(ChannelKind::Music, 0.7);
        assert!((prev - 0.3).abs() < 1e-6);
        assert!((mixer.volume(ChannelKind::Music) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn test_mixer_active_count_starts_zero() {
        let mixer = test_mixer();
        assert_eq!(mixer.active_count(), 0);
    }

    #[test]
    fn test_mixer_cleanup_no_active() {
        let mixer = test_mixer();
        assert_eq!(mixer.cleanup(), 0);
    }

    #[test]
    fn test_mixer_all_sub_channels() {
        let subs = ChannelKind::all_sub();
        assert_eq!(subs.len(), 5);
        assert!(subs.contains(&ChannelKind::Music));
        assert!(subs.contains(&ChannelKind::Sfx));
        assert!(subs.contains(&ChannelKind::Ui));
        assert!(subs.contains(&ChannelKind::Ambient));
        assert!(subs.contains(&ChannelKind::Voice));
        assert!(!subs.contains(&ChannelKind::Master));
    }

    #[test]
    fn test_mixer_channel_kind_debug() {
        let ch = format!("{:?}", ChannelKind::Master);
        assert_eq!(ch, "Master");
    }

    #[test]
    fn test_mixer_stop_all_empty() {
        let mixer = test_mixer();
        mixer.stop_all();
        assert_eq!(mixer.active_count(), 0);
    }

    #[test]
    fn test_mixer_stop_channel_empty() {
        let mixer = test_mixer();
        mixer.stop_channel(ChannelKind::Sfx);
        assert_eq!(mixer.active_count(), 0);
    }
}
