//! Sound cache — loads, deduplicates, and manages audio assets.
//!
//! Sounds are identified by a string key (typically a file path). Loading
//! the same key twice returns the cached data. Cache statistics track
//! hits, misses, and total memory usage.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::AudioError;

/// Raw PCM sound data.
///
/// All sounds are stored as 16-bit signed integer samples at 44100 Hz,
/// currently mono. This keeps the cache simple and avoids tying it to a
/// specific file format.
#[derive(Debug, Clone)]
pub struct SoundData {
    /// PCM samples (i16, 44100 Hz, mono).
    pub samples: Arc<Vec<i16>>,
    /// Duration in seconds.
    pub duration_secs: f32,
}

impl SoundData {
    /// Create `SoundData` from raw PCM samples.
    pub fn from_samples(samples: Vec<i16>) -> Self {
        let duration_secs = samples.len() as f32 / 44100.0;
        Self {
            samples: Arc::new(samples),
            duration_secs,
        }
    }

    /// Memory used by this sound's sample data in bytes.
    pub fn memory_bytes(&self) -> usize {
        self.samples.len() * 2 // i16 = 2 bytes
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Number of cache hits (sound was already loaded).
    pub hits: u64,
    /// Number of cache misses (sound was loaded for the first time).
    pub misses: u64,
    /// Number of unique sounds currently cached.
    pub unique_sounds: usize,
    /// Total memory used by cached sound data in bytes.
    pub memory_bytes: usize,
}

/// A cache of loaded sound data with deduplication.
///
/// Thread-safe: all operations are behind a single internal mutex.
pub struct SoundCache {
    sounds: Mutex<HashMap<String, SoundData>>,
    hits: Mutex<u64>,
    misses: Mutex<u64>,
}

impl SoundCache {
    /// Create an empty sound cache.
    pub fn new() -> Self {
        Self {
            sounds: Mutex::new(HashMap::new()),
            hits: Mutex::new(0),
            misses: Mutex::new(0),
        }
    }

    /// Retrieve a sound by key. Returns `None` if not cached.
    pub fn get(&self, key: &str) -> Option<SoundData> {
        let sounds = self.sounds.lock().expect("cache lock");
        if let Some(data) = sounds.get(key) {
            let mut hits = self.hits.lock().expect("cache lock");
            *hits += 1;
            Some(data.clone())
        } else {
            None
        }
    }

    /// Insert a sound into the cache.
    pub fn insert(&self, key: String, data: SoundData) {
        let mut sounds = self.sounds.lock().expect("cache lock");
        let mut misses = self.misses.lock().expect("cache lock");
        *misses += 1;
        sounds.insert(key, data);
    }

    /// Get or load a sound. If already cached, returns the cached data.
    /// If not, calls `loader` to produce the data, caches it, and returns it.
    ///
    /// The loader is only called when the key is not already cached.
    pub fn get_or_load<F>(&self, key: &str, loader: F) -> Result<SoundData, AudioError>
    where
        F: FnOnce() -> Result<SoundData, AudioError>,
    {
        if let Some(data) = self.get(key) {
            return Ok(data);
        }

        let data = loader()?;
        self.insert(key.to_string(), data.clone());
        Ok(data)
    }

    /// Remove a sound from the cache.
    pub fn remove(&self, key: &str) {
        let mut sounds = self.sounds.lock().expect("cache lock");
        sounds.remove(key);
    }

    /// Clear all cached sounds.
    pub fn clear(&self) {
        let mut sounds = self.sounds.lock().expect("cache lock");
        sounds.clear();
    }

    /// Number of unique sounds in the cache.
    pub fn len(&self) -> usize {
        let sounds = self.sounds.lock().expect("cache lock");
        sounds.len()
    }

    /// Is the cache empty?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Current cache statistics.
    pub fn stats(&self) -> CacheStats {
        let sounds = self.sounds.lock().expect("cache lock");
        let hits = *self.hits.lock().expect("cache lock");
        let misses = *self.misses.lock().expect("cache lock");
        let memory_bytes: usize = sounds.values().map(|d| d.memory_bytes()).sum();
        CacheStats {
            hits,
            misses,
            unique_sounds: sounds.len(),
            memory_bytes,
        }
    }

    /// Preload a sound into the cache from raw PCM data.
    pub fn load_raw(&self, key: &str, samples: Vec<i16>) {
        let data = SoundData::from_samples(samples);
        self.insert(key.to_string(), data);
    }
}

impl Default for SoundCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cache() -> SoundCache {
        SoundCache::new()
    }

    #[test]
    fn test_cache_empty() {
        let cache = test_cache();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let cache = test_cache();
        let data = SoundData::from_samples(vec![0i16; 44100]);
        cache.insert("test".into(), data);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
        assert!(cache.get("test").is_some());
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_deduplication() {
        let cache = test_cache();
        cache.load_raw("hit", vec![1i16; 100]);
        cache.load_raw("hit", vec![2i16; 100]);
        // Second load replaces the first
        assert_eq!(cache.len(), 1);
        let data = cache.get("hit").unwrap();
        assert_eq!(data.samples[0], 2);
    }

    #[test]
    fn test_cache_stats() {
        let cache = test_cache();
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.unique_sounds, 0);
        assert_eq!(stats.memory_bytes, 0);
    }

    #[test]
    fn test_cache_stats_hits() {
        let cache = test_cache();
        cache.load_raw("a", vec![0i16; 100]);
        cache.load_raw("b", vec![0i16; 200]);

        // Hit
        cache.get("a");
        cache.get("b");
        cache.get("a");

        let stats = cache.stats();
        assert_eq!(stats.hits, 3);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.unique_sounds, 2);
    }

    #[test]
    fn test_cache_stats_memory() {
        let cache = test_cache();
        cache.load_raw("a", vec![0i16; 100]);
        cache.load_raw("b", vec![0i16; 200]);

        let stats = cache.stats();
        // 100 * 2 + 200 * 2 = 600 bytes
        assert_eq!(stats.memory_bytes, 600);
    }

    #[test]
    fn test_cache_remove() {
        let cache = test_cache();
        cache.load_raw("temp", vec![0i16; 100]);
        assert_eq!(cache.len(), 1);
        cache.remove("temp");
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_clear() {
        let cache = test_cache();
        cache.load_raw("a", vec![0i16; 100]);
        cache.load_raw("b", vec![0i16; 200]);
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_get_or_load_hit() {
        let cache = test_cache();
        cache.load_raw("existing", vec![42i16; 10]);
        let result = cache.get_or_load("existing", || {
            Err(AudioError::Io(std::io::Error::other(
                "should not be called",
            )))
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap().samples[0], 42);
    }

    #[test]
    fn test_cache_get_or_load_miss() {
        let cache = test_cache();
        let result = cache.get_or_load("new", || Ok(SoundData::from_samples(vec![99i16; 10])));
        assert!(result.is_ok());
        assert_eq!(cache.len(), 1);
        assert_eq!(result.unwrap().samples[0], 99);
    }

    #[test]
    fn test_cache_get_or_load_loader_error() {
        let cache = test_cache();
        let result = cache.get_or_load("broken", || {
            Err(AudioError::UnsupportedFormat("bad format".into()))
        });
        assert!(result.is_err());
        assert!(cache.is_empty());
    }

    #[test]
    fn test_sound_data_memory_bytes() {
        let data = SoundData::from_samples(vec![0i16; 44100]);
        assert_eq!(data.memory_bytes(), 44100 * 2);
    }

    #[test]
    fn test_sound_data_duration() {
        let data = SoundData::from_samples(vec![0i16; 44100]);
        assert!((data.duration_secs - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cache_stale_get_after_remove() {
        let cache = test_cache();
        cache.load_raw("key", vec![5i16; 10]);
        assert!(cache.get("key").is_some());
        cache.remove("key");
        assert!(cache.get("key").is_none());
    }
}
