//! Audio engine — the top-level audio system.
//!
//! `AudioSystem` owns the audio device, mixer, and sound cache. It is the
//! primary entry point for all audio operations in the runtime.

use std::sync::Arc;

use rodio::{OutputStream, OutputStreamHandle, Sink};
use tracing::{info, warn};

use vibege_asset::AudioAsset;

use crate::AudioError;
use crate::handle::PlaybackHandle;
use crate::mixer::{ChannelKind, Mixer};
use crate::sound_cache::SoundCache;

/// The audio engine.
///
/// Create via `AudioSystem::new()`. If no audio device is available, the
/// system returns `None` instead of crashing the runtime.
///
/// # Examples
///
/// ```ignore
/// let audio = AudioSystem::new().expect("audio device");
/// audio.set_channel_volume(ChannelKind::Music, 0.5);
/// let handle = audio.play("hit", ChannelKind::Sfx)?;
/// handle.set_looping(true);
/// ```
pub struct AudioSystem {
    #[allow(dead_code)]
    stream: OutputStream,
    handle: OutputStreamHandle,
    mixer: Arc<Mixer>,
    cache: SoundCache,
}

impl AudioSystem {
    /// Initialise the audio output device.
    /// Returns `None` if no audio device is available (non-fatal).
    pub fn new() -> Option<Self> {
        match OutputStream::try_default() {
            Ok((stream, handle)) => {
                let handle_ref = handle.clone();
                let mixer = Arc::new(Mixer::new(Box::new(move || {
                    Sink::try_new(&handle_ref).ok()
                })));
                info!("Audio system initialised");
                Some(Self {
                    stream,
                    handle,
                    mixer,
                    cache: SoundCache::new(),
                })
            }
            Err(e) => {
                warn!("No audio device available: {e}");
                None
            }
        }
    }

    // ── Playback (fire-and-forget) ──────────────────────────────────

    /// Play a sound effect on the SFX channel (fire-and-forget).
    ///
    /// This is the backward-compatible API. For more control, use
    /// [`play`](Self::play) or [`play_on`](Self::play_on).
    pub fn play_sfx(&self, data: &[i16]) {
        if let Ok(sink) = Sink::try_new(&self.handle) {
            let samples = data.to_vec();
            let format = rodio::buffer::SamplesBuffer::new(1, 44100, samples.clone());
            sink.append(format);
            self.mixer
                .register(ChannelKind::Sfx, sink, Arc::new(samples));
        }
    }

    // ── Playback (with handle) ──────────────────────────────────────

    /// Play a sound on the SFX channel and return a handle.
    ///
    /// The handle allows stopping, pausing, resuming, and adjusting
    /// volume/looping on the active sound.
    pub fn play(&self, data: Arc<Vec<i16>>) -> Result<PlaybackHandle, AudioError> {
        self.play_on(data, ChannelKind::Sfx)
    }

    /// Play a sound on a specific channel and return a handle.
    pub fn play_on(
        &self,
        data: Arc<Vec<i16>>,
        channel: ChannelKind,
    ) -> Result<PlaybackHandle, AudioError> {
        let sink =
            Sink::try_new(&self.handle).map_err(|e| AudioError::SinkFailed(e.to_string()))?;

        let format = rodio::buffer::SamplesBuffer::new(1, 44100, (*data).clone());
        sink.append(format);

        let id = self.mixer.register(channel, sink, Arc::clone(&data));
        Ok(PlaybackHandle::new(id, Arc::clone(&self.mixer)))
    }

    /// Play a sound from the cache on a specific channel.
    ///
    /// The sound must have been previously loaded into the cache via
    /// `load_sound` or `cache().load_raw()`.
    pub fn play_cached(
        &self,
        key: &str,
        channel: ChannelKind,
    ) -> Result<PlaybackHandle, AudioError> {
        let data = self
            .cache
            .get(key)
            .ok_or_else(|| AudioError::SoundNotFound(key.to_string()))?;
        let data_clone = Arc::clone(&data.samples);
        let sink =
            Sink::try_new(&self.handle).map_err(|e| AudioError::SinkFailed(e.to_string()))?;
        let format = rodio::buffer::SamplesBuffer::new(1, 44100, (*data.samples).clone());
        sink.append(format);
        let id = self.mixer.register(channel, sink, data_clone);
        Ok(PlaybackHandle::new(id, Arc::clone(&self.mixer)))
    }

    // ── Sound cache ─────────────────────────────────────────────────

    /// Access the sound cache.
    pub fn cache(&self) -> &SoundCache {
        &self.cache
    }

    /// Load raw PCM samples into the cache under the given key.
    pub fn load_sound(&self, key: &str, samples: Vec<i16>) {
        self.cache.load_raw(key, samples);
    }

    /// Load a test-tone sound into the cache for quick prototyping.
    pub fn load_test_tone(&self, key: &str, frequency: f32, duration_secs: f32) {
        let samples = crate::generate_test_tone(frequency, duration_secs);
        self.cache.load_raw(key, samples);
    }

    // ── Mixer control ───────────────────────────────────────────────

    /// Set the volume of a channel (0.0 – 1.0).
    pub fn set_channel_volume(&self, channel: ChannelKind, volume: f32) {
        self.mixer.set_volume(channel, volume);
    }

    /// Get the volume of a channel.
    pub fn channel_volume(&self, channel: ChannelKind) -> f32 {
        self.mixer.volume(channel)
    }

    /// Mute or unmute a channel.
    pub fn set_channel_mute(&self, channel: ChannelKind, muted: bool) {
        self.mixer.set_mute(channel, muted);
    }

    /// Is a channel muted?
    pub fn is_channel_muted(&self, channel: ChannelKind) -> bool {
        self.mixer.is_muted(channel)
    }

    // ── Backward-compatible volume API ──────────────────────────────

    /// Set the music volume (preserved from the original API).
    pub fn set_music_volume(&self, vol: f32) {
        self.set_channel_volume(ChannelKind::Music, vol);
    }

    /// Set the SFX volume (preserved from the original API).
    pub fn set_sfx_volume(&self, vol: f32) {
        self.set_channel_volume(ChannelKind::Sfx, vol);
    }

    // ── Global control ──────────────────────────────────────────────

    /// Stop all sounds on all channels.
    pub fn stop_all(&self) {
        self.mixer.stop_all();
    }

    /// Stop all sounds on a specific channel.
    pub fn stop_channel(&self, channel: ChannelKind) {
        self.mixer.stop_channel(channel);
    }

    /// Play a sound from the asset system's `AudioAsset`.
    ///
    /// This is the preferred way to play sounds loaded through the
    /// `AssetManager`. The asset data is cloned into rodio's buffer
    /// format for playback.
    pub fn play_asset(
        &self,
        asset: &AudioAsset,
        channel: ChannelKind,
    ) -> Result<PlaybackHandle, AudioError> {
        let sink =
            Sink::try_new(&self.handle).map_err(|e| AudioError::SinkFailed(e.to_string()))?;
        let samples = Arc::new(asset.samples.clone());
        let format = rodio::buffer::SamplesBuffer::new(1, 44100, asset.samples.clone());
        sink.append(format);
        let id = self.mixer.register(channel, sink, Arc::clone(&samples));
        Ok(PlaybackHandle::new(id, Arc::clone(&self.mixer)))
    }

    /// Remove finished sounds from the active list.
    /// Returns the number of sounds cleaned up.
    pub fn cleanup(&self) -> usize {
        self.mixer.cleanup()
    }

    /// Number of currently active (playing) sounds.
    pub fn active_count(&self) -> usize {
        self.mixer.active_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer::ChannelKind;

    #[test]
    fn test_engine_channel_volume_via_mixer() {
        // Test the mixer directly instead of going through the engine.
        let mixer = Mixer::new(Box::new(|| None));
        mixer.set_volume(ChannelKind::Music, 0.5);
        let vol = mixer.volume(ChannelKind::Music);
        assert!((vol - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_engine_mute_via_mixer() {
        let mixer = Mixer::new(Box::new(|| None));
        assert!(!mixer.is_muted(ChannelKind::Sfx));
        mixer.set_mute(ChannelKind::Sfx, true);
        assert!(mixer.is_muted(ChannelKind::Sfx));
    }

    #[test]
    fn test_engine_cleanup_via_mixer() {
        let mixer = Mixer::new(Box::new(|| None));
        assert_eq!(mixer.cleanup(), 0);
    }

    #[test]
    fn test_engine_stop_all_via_mixer() {
        let mixer = Mixer::new(Box::new(|| None));
        mixer.stop_all();
        assert_eq!(mixer.active_count(), 0);
    }

    #[test]
    fn test_engine_cache_operations() {
        let cache = SoundCache::new();
        cache.load_raw("test", vec![0i16; 44100]);
        let cached = cache.get("test");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().samples.len(), 44100);

        cache.load_raw("tone2", vec![0i16; 22050]);
        let cached2 = cache.get("tone2");
        assert!(cached2.is_some());
        assert_eq!(cached2.unwrap().samples.len(), 22050);

        let result = cache.get("missing");
        assert!(result.is_none());
    }

    #[test]
    fn test_engine_play_cached_missing_logic() {
        let cache = SoundCache::new();
        let data = cache.get("missing");
        match data {
            None => {} // expected
            Some(_) => panic!("Expected no data"),
        }
    }

    #[test]
    fn test_engine_set_music_volume_passthrough() {
        let mixer = Mixer::new(Box::new(|| None));
        mixer.set_volume(ChannelKind::Music, 0.3);
        assert!((mixer.volume(ChannelKind::Music) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_engine_set_sfx_volume_passthrough() {
        let mixer = Mixer::new(Box::new(|| None));
        mixer.set_volume(ChannelKind::Sfx, 0.7);
        assert!((mixer.volume(ChannelKind::Sfx) - 0.7).abs() < 1e-6);
    }
}
