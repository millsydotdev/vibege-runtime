use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::AssetId;
use crate::handle::{AssetHandle, ResourceLifetime};
use crate::metadata::AssetMetadata;
use crate::statistics::TypeStats;

fn lock_entries<T>(
    mtx: &Mutex<HashMap<String, CachedEntry<T>>>,
) -> MutexGuard<'_, HashMap<String, CachedEntry<T>>> {
    mtx.lock().unwrap_or_else(|e| {
        tracing::warn!("Cache entries mutex poisoned — recovering");
        e.into_inner()
    })
}

fn lock_id_map(mtx: &Mutex<HashMap<AssetId, String>>) -> MutexGuard<'_, HashMap<AssetId, String>> {
    mtx.lock().unwrap_or_else(|e| {
        tracing::warn!("Cache id_map mutex poisoned — recovering");
        e.into_inner()
    })
}

/// Internal cached entry with lifetime tracking.
pub(crate) struct CachedEntry<T> {
    pub id: AssetId,
    #[allow(dead_code)]
    pub key: String,
    pub data: T,
    pub metadata: AssetMetadata,
    pub lifetime: Arc<ResourceLifetime>,
}

/// A typed cache for assets of type `T`.
///
/// Provides deduplication by key, reference counting via handles,
/// and statistics tracking.
pub struct AssetCache<T> {
    /// Map from key to cached entry.
    entries: Mutex<HashMap<String, CachedEntry<T>>>,
    /// Map from AssetId to key for reverse lookup.
    id_to_key: Mutex<HashMap<AssetId, String>>,
    next_id: AtomicU64,
    /// Statistics counters.
    hits: AtomicU64,
    misses: AtomicU64,
    loads: AtomicU64,
    releases: AtomicU64,
    failed_loads: AtomicU64,
    /// Memory estimate per entry (caller-provided function).
    memory_fn: Box<dyn Fn(&T) -> u64 + Send + Sync>,
}

impl<T: Clone + Send + Sync + 'static> AssetCache<T> {
    pub fn new(memory_fn: Box<dyn Fn(&T) -> u64 + Send + Sync>) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            id_to_key: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            loads: AtomicU64::new(0),
            releases: AtomicU64::new(0),
            failed_loads: AtomicU64::new(0),
            memory_fn,
        }
    }

    /// Returns a new unique asset ID.
    pub fn next_id(&self) -> AssetId {
        AssetId::new(self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Check if an asset with the given key is already cached.
    pub fn contains(&self, key: &str) -> bool {
        lock_entries(&self.entries).contains_key(key)
    }

    /// Retrieve a handle to a cached asset by key.
    /// Returns `None` if not cached.
    pub fn get(&self, key: &str) -> Option<AssetHandle<T>> {
        let entries = lock_entries(&self.entries);
        if let Some(entry) = entries.get(key) {
            entry.lifetime.increment();
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(AssetHandle::new(
                entry.id,
                key.to_string(),
                Arc::clone(&entry.lifetime),
            ))
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Retrieve a reference to the cached data by key.
    pub fn get_data(&self, key: &str) -> Option<T> {
        let entries = lock_entries(&self.entries);
        entries.get(key).map(|e| {
            self.hits.fetch_add(1, Ordering::Relaxed);
            e.data.clone()
        })
    }

    /// Insert an asset into the cache. If the key already exists,
    /// replaces it and returns the old entry's cleanup handle.
    pub fn insert(
        &self,
        key: String,
        data: T,
        metadata: AssetMetadata,
        lifetime: Arc<ResourceLifetime>,
        id: AssetId,
    ) {
        let mut entries = lock_entries(&self.entries);
        let mut id_map = lock_id_map(&self.id_to_key);
        self.loads.fetch_add(1, Ordering::Relaxed);
        let cache_key = key.clone();
        id_map.insert(id, cache_key.clone());
        entries.insert(
            cache_key,
            CachedEntry {
                id,
                key: key.clone(),
                data,
                metadata,
                lifetime,
            },
        );
    }

    /// Remove an asset from the cache by key.
    pub fn remove(&self, key: &str) {
        let mut entries = lock_entries(&self.entries);
        let mut id_map = lock_id_map(&self.id_to_key);
        if let Some(entry) = entries.remove(key) {
            self.releases.fetch_add(1, Ordering::Relaxed);
            id_map.remove(&entry.id);
        }
    }

    /// Clear all cached assets.
    pub fn clear(&self) {
        let mut entries = lock_entries(&self.entries);
        let mut id_map = lock_id_map(&self.id_to_key);
        let count = entries.len();
        self.releases.fetch_add(count as u64, Ordering::Relaxed);
        entries.clear();
        id_map.clear();
    }

    /// Number of unique assets in the cache.
    pub fn len(&self) -> usize {
        lock_entries(&self.entries).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Gather statistics for this cache.
    pub fn stats(&self, _asset_type: &'static str) -> TypeStats {
        let entries = lock_entries(&self.entries);
        let memory_bytes: u64 = entries.values().map(|e| (self.memory_fn)(&e.data)).sum();
        TypeStats {
            count: entries.len(),
            memory_bytes,
            cache_hits: self.hits.load(Ordering::Relaxed),
            cache_misses: self.misses.load(Ordering::Relaxed),
            loads: self.loads.load(Ordering::Relaxed),
            releases: self.releases.load(Ordering::Relaxed),
            failed_loads: self.failed_loads.load(Ordering::Relaxed),
        }
    }

    /// Record that an asset load failed.
    pub fn record_failure(&self) {
        self.failed_loads.fetch_add(1, Ordering::Relaxed);
    }

    /// Get all metadata entries.
    pub fn all_metadata(&self) -> Vec<AssetMetadata> {
        let entries = lock_entries(&self.entries);
        entries.values().map(|e| e.metadata.clone()).collect()
    }

    /// Cache hit count.
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Cache miss count.
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Total load operations.
    pub fn loads(&self) -> u64 {
        self.loads.load(Ordering::Relaxed)
    }

    /// Total release operations.
    pub fn releases(&self) -> u64 {
        self.releases.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AssetTypeId;
    use crate::metadata::AssetSource;

    fn test_cache() -> AssetCache<String> {
        AssetCache::new(Box::new(|s: &String| s.len() as u64))
    }

    fn insert_test(cache: &AssetCache<String>, key: &str, data: &str) -> AssetHandle<String> {
        let id = cache.next_id();
        let lifetime = ResourceLifetime::new();
        let meta = AssetMetadata::new(
            id,
            key.into(),
            AssetTypeId::Raw,
            AssetSource::Memory,
            data.len() as u64,
            "text".into(),
        );
        cache.insert(
            key.into(),
            data.to_string(),
            meta,
            Arc::clone(&lifetime),
            id,
        );
        AssetHandle::new(id, key.into(), lifetime)
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
        let handle = insert_test(&cache, "test", "hello world");
        assert_eq!(cache.len(), 1);
        assert!(cache.contains("test"));

        let got = cache.get("test");
        assert!(got.is_some());
        assert_eq!(got.unwrap().key(), "test");
        assert_eq!(cache.get_data("test"), Some("hello world".to_string()));
        drop(handle);
    }

    #[test]
    fn test_cache_deduplication() {
        let cache = test_cache();
        let _h1 = insert_test(&cache, "dup", "first");
        assert_eq!(cache.len(), 1);
        let _h2 = insert_test(&cache, "dup", "second");
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get_data("dup"), Some("second".to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let cache = test_cache();
        assert!(cache.get("nonexistent").is_none());
        assert!(!cache.contains("nonexistent"));
    }

    #[test]
    fn test_cache_remove() {
        let cache = test_cache();
        let _h = insert_test(&cache, "temp", "data");
        assert_eq!(cache.len(), 1);
        cache.remove("temp");
        assert_eq!(cache.len(), 0);
        assert!(cache.get("temp").is_none());
    }

    #[test]
    fn test_cache_clear() {
        let cache = test_cache();
        let _h1 = insert_test(&cache, "a", "data1");
        let _h2 = insert_test(&cache, "b", "data2");
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_stats() {
        let cache = test_cache();
        let stats = cache.stats("raw");
        assert_eq!(stats.count, 0);
        assert_eq!(stats.memory_bytes, 0);

        let _h = insert_test(&cache, "key", "hello");
        let stats = cache.stats("raw");
        assert_eq!(stats.count, 1);
        assert_eq!(stats.memory_bytes, 5);

        cache.get("key");
        cache.get("key");
        cache.get("missing");
        let stats = cache.stats("raw");
        assert_eq!(stats.cache_hits, 2);
        assert_eq!(stats.cache_misses, 1);
    }
}
