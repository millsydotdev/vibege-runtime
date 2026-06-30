use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::models::GameListing;

/// Cache for store metadata and search results.
pub struct StoreCache {
    /// Cached game listings by ID.
    listings: Mutex<HashMap<String, CachedEntry<GameListing>>>,
    /// Cached search results by query.
    search_results: Mutex<HashMap<String, CachedEntry<Vec<String>>>>,
    /// Cached store sections.
    sections: Mutex<HashMap<String, CachedEntry<Vec<String>>>>,
    /// Timestamp of last successful fetch.
    last_fetch: Mutex<Option<Instant>>,
    /// Offline mode flag.
    offline: Mutex<bool>,
}

struct CachedEntry<T> {
    data: T,
    cached_at: Instant,
    ttl: Duration,
}

impl<T> CachedEntry<T> {
    fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            cached_at: Instant::now(),
            ttl,
        }
    }

    fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < self.ttl
    }
}

impl StoreCache {
    pub fn new() -> Self {
        Self {
            listings: Mutex::new(HashMap::new()),
            search_results: Mutex::new(HashMap::new()),
            sections: Mutex::new(HashMap::new()),
            last_fetch: Mutex::new(None),
            offline: Mutex::new(false),
        }
    }

    // ── Listings ──

    pub fn cache_listings(&self, listings: Vec<GameListing>, ttl_secs: u64) {
        let mut cache = self.listings.lock().expect("cache lock");
        let ttl = Duration::from_secs(ttl_secs);
        for listing in listings {
            cache.insert(listing.id.clone(), CachedEntry::new(listing, ttl));
        }
        *self.last_fetch.lock().expect("cache lock") = Some(Instant::now());
    }

    pub fn get_listing(&self, id: &str) -> Option<GameListing> {
        let cache = self.listings.lock().expect("cache lock");
        cache.get(id).and_then(|e| {
            if e.is_valid() {
                Some(e.data.clone())
            } else {
                None
            }
        })
    }

    pub fn get_all_listings(&self) -> Vec<GameListing> {
        let cache = self.listings.lock().expect("cache lock");
        cache
            .values()
            .filter(|e| e.is_valid())
            .map(|e| e.data.clone())
            .collect()
    }

    pub fn invalidate_listings(&self) {
        self.listings.lock().expect("cache lock").clear();
    }

    // ── Search ──

    pub fn cache_search(&self, query: &str, result_ids: Vec<String>, ttl_secs: u64) {
        let mut cache = self.search_results.lock().expect("cache lock");
        cache.insert(
            query.to_string(),
            CachedEntry::new(result_ids, Duration::from_secs(ttl_secs)),
        );
    }

    pub fn get_cached_search(&self, query: &str) -> Option<Vec<String>> {
        let cache = self.search_results.lock().expect("cache lock");
        cache.get(query).and_then(|e| {
            if e.is_valid() {
                Some(e.data.clone())
            } else {
                None
            }
        })
    }

    // ── Sections ──

    pub fn cache_section(&self, key: &str, game_ids: Vec<String>, ttl_secs: u64) {
        let mut cache = self.sections.lock().expect("cache lock");
        cache.insert(
            key.to_string(),
            CachedEntry::new(game_ids, Duration::from_secs(ttl_secs)),
        );
    }

    pub fn get_cached_section(&self, key: &str) -> Option<Vec<String>> {
        let cache = self.sections.lock().expect("cache lock");
        cache.get(key).and_then(|e| {
            if e.is_valid() {
                Some(e.data.clone())
            } else {
                None
            }
        })
    }

    // ── Offline ──

    pub fn set_offline(&self, offline: bool) {
        *self.offline.lock().expect("cache lock") = offline;
    }

    pub fn is_offline(&self) -> bool {
        *self.offline.lock().expect("cache lock")
    }

    /// Returns true if cached data is available and recent enough to
    /// serve offline.
    pub fn has_recent_data(&self, max_age_secs: u64) -> bool {
        let last = self.last_fetch.lock().expect("cache lock");
        match *last {
            Some(t) => t.elapsed() < Duration::from_secs(max_age_secs),
            None => false,
        }
    }

    /// Clear all cached data.
    pub fn clear(&self) {
        self.listings.lock().expect("cache lock").clear();
        self.search_results.lock().expect("cache lock").clear();
        self.sections.lock().expect("cache lock").clear();
    }

    /// Number of cached listings.
    pub fn listing_count(&self) -> usize {
        self.listings.lock().expect("cache lock").len()
    }
}

impl Default for StoreCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_listing(id: &str, name: &str) -> GameListing {
        GameListing {
            id: id.to_string(),
            name: name.to_string(),
            description: "".into(),
            author: "".into(),
            publisher: "".into(),
            version: "0.1.0".into(),
            category: "".into(),
            genres: vec![],
            tags: vec![],
            status: "approved".into(),
            downloads: 0,
            file_size: 0,
            icon_url: None,
            hero_url: None,
            screenshots: vec![],
            created_at: "".into(),
            updated_at: "".into(),
            engine_version: None,
            rating: 0.0,
        }
    }

    #[test]
    fn test_cache_empty() {
        let cache = StoreCache::new();
        assert_eq!(cache.listing_count(), 0);
        assert!(!cache.has_recent_data(10));
    }

    #[test]
    fn test_cache_listing() {
        let cache = StoreCache::new();
        cache.cache_listings(vec![sample_listing("g1", "Game")], 60);
        assert_eq!(cache.listing_count(), 1);
        let listing = cache.get_listing("g1");
        assert!(listing.is_some());
        assert_eq!(listing.unwrap().name, "Game");
    }

    #[test]
    fn test_cache_listing_expired() {
        let cache = StoreCache::new();
        cache.cache_listings(vec![sample_listing("g1", "Game")], 0);
        // 0 TTL means expired immediately
        assert!(cache.get_listing("g1").is_none());
    }

    #[test]
    fn test_cache_search() {
        let cache = StoreCache::new();
        cache.cache_search("pong", vec!["g1".into(), "g2".into()], 60);
        let results = cache.get_cached_search("pong");
        assert!(results.is_some());
        assert_eq!(results.unwrap().len(), 2);
    }

    #[test]
    fn test_cache_search_miss() {
        let cache = StoreCache::new();
        assert!(cache.get_cached_search("missing").is_none());
    }

    #[test]
    fn test_cache_section() {
        let cache = StoreCache::new();
        cache.cache_section("featured", vec!["g1".into()], 60);
        let section = cache.get_cached_section("featured");
        assert!(section.is_some());
    }

    #[test]
    fn test_offline_mode() {
        let cache = StoreCache::new();
        assert!(!cache.is_offline());
        cache.set_offline(true);
        assert!(cache.is_offline());
    }

    #[test]
    fn test_clear() {
        let cache = StoreCache::new();
        cache.cache_listings(vec![sample_listing("g1", "Game")], 60);
        assert_eq!(cache.listing_count(), 1);
        cache.clear();
        assert_eq!(cache.listing_count(), 0);
    }

    #[test]
    fn test_get_all_listings() {
        let cache = StoreCache::new();
        cache.cache_listings(
            vec![sample_listing("g1", "A"), sample_listing("g2", "B")],
            60,
        );
        assert_eq!(cache.get_all_listings().len(), 2);
    }

    #[test]
    fn test_invalidate_listings() {
        let cache = StoreCache::new();
        cache.cache_listings(vec![sample_listing("g1", "Game")], 60);
        assert_eq!(cache.listing_count(), 1);
        cache.invalidate_listings();
        assert_eq!(cache.listing_count(), 0);
    }

    #[test]
    fn test_has_recent_data() {
        let cache = StoreCache::new();
        assert!(!cache.has_recent_data(10));
        cache.cache_listings(vec![sample_listing("g1", "Game")], 60);
        assert!(cache.has_recent_data(60));
    }
}
