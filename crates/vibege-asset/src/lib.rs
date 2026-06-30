//! # VibeGE Asset & Resource Management System
//!
//! Centralised asset loading, caching, and lifecycle management for all
//! engine assets. Every asset flows through the `AssetManager`, ensuring
//! deduplication, reference counting, and deterministic cleanup.
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │                   AssetManager                     │
//! │  ┌──────────┐  ┌──────────┐  ┌──────────────────┐ │
//! │  │ Texture  │  │  Audio   │  │  LuaSource / Raw  │ │
//! │  │  Cache   │  │  Cache   │  │     Cache         │ │
//! │  └────┬─────┘  └────┬─────┘  └────────┬─────────┘ │
//! │       │              │                 │           │
//! │  ┌────▼──────────────▼─────────────────▼────────┐  │
//! │  │           AssetHandle<T>                      │  │
//! │  │         (ref-counted, typed)                  │  │
//! │  └───────────────────────────────────────────────┘  │
//! └────────────────────────────────────────────────────┘
//! ```
//!
//! ## Asset Lifecycle
//!
//! 1. **Load** — Call `AssetManager::load_*()` to load an asset by key.
//! 2. **Cache** — The asset is stored in the typed cache. Subsequent
//!    loads with the same key return the existing handle (dedup).
//! 3. **Reference** — Each `AssetHandle::clone()` increments the ref count.
//! 4. **Release** — Each `AssetHandle::drop()` decrements the ref count.
//! 5. **Cleanup** — When the ref count hits zero, the resource is freed.
//! 6. **Shutdown** — `AssetManager::shutdown()` clears all caches.

pub mod cache;
pub mod handle;
pub mod loader;
pub mod metadata;
pub mod package;
pub mod statistics;
pub mod types;

pub use handle::{AssetHandle, AssetId, ResourceLifetime};
pub use metadata::{AssetMetadata, AssetSource, AssetTypeId};
pub use statistics::{AssetStatistics, TypeStats};
pub use types::{AudioAsset, FontAsset, LuaSourceAsset, PackageAsset, RawAsset, TextureAsset};

use std::sync::Arc;

/// The central asset registry.
///
/// Owns all typed asset caches and provides the public API for loading,
/// retrieving, and releasing assets.
pub struct AssetManager {
    /// Cache for GPU textures.
    texture_cache: cache::AssetCache<TextureAsset>,
    /// Cache for audio samples.
    audio_cache: cache::AssetCache<AudioAsset>,
    /// Cache for Lua source files.
    lua_cache: cache::AssetCache<LuaSourceAsset>,
    /// Cache for raw binary data.
    raw_cache: cache::AssetCache<RawAsset>,
    /// Cache for mounted packages.
    package_cache: cache::AssetCache<PackageAsset>,
    /// Registered texture loader callback (provided by the renderer).
    texture_loader: std::sync::RwLock<Option<loader::TextureLoaderFn>>,
}

impl AssetManager {
    /// Create a new empty asset manager.
    pub fn new() -> Self {
        Self {
            texture_cache: cache::AssetCache::new(Box::new(|t: &TextureAsset| {
                (t.width as u64) * (t.height as u64) * 4
            })),
            audio_cache: cache::AssetCache::new(Box::new(|a: &AudioAsset| {
                a.samples.len() as u64 * 2
            })),
            lua_cache: cache::AssetCache::new(Box::new(|l: &LuaSourceAsset| l.source.len() as u64)),
            raw_cache: cache::AssetCache::new(Box::new(|r: &RawAsset| r.data.len() as u64)),
            package_cache: cache::AssetCache::new(Box::new(|p: &PackageAsset| {
                p.entries().iter().map(|(_, _, s)| s).sum::<u64>()
            })),
            texture_loader: std::sync::RwLock::new(None),
        }
    }

    /// Register a texture loader callback (called by the renderer on init).
    pub fn set_texture_loader(&self, loader: loader::TextureLoaderFn) {
        let mut guard = self.texture_loader.write().unwrap_or_else(|e| {
            tracing::warn!("Texture loader mutex poisoned — recovering");
            e.into_inner()
        });
        *guard = Some(loader);
    }

    // ── Texture Assets ──────────────────────────────────────────────

    /// Load a texture from raw bytes.
    pub fn load_texture(
        &self,
        key: &str,
        data: &[u8],
        source: AssetSource,
    ) -> Result<AssetHandle<TextureAsset>, loader::LoaderError> {
        if let Some(handle) = self.texture_cache.get(key) {
            return Ok(handle);
        }

        loader::TextureLoader::validate(data)?;

        let loader_guard = self.texture_loader.read().unwrap_or_else(|e| {
            tracing::warn!("Texture loader mutex poisoned — recovering");
            e.into_inner()
        });
        let loader_fn = loader_guard.as_ref().ok_or_else(|| {
            self.texture_cache.record_failure();
            loader::LoaderError::InvalidData(
                "No texture loader registered (renderer not ready)".into(),
            )
        })?;

        let texture = loader_fn(data, source.clone()).inspect_err(|_| {
            self.texture_cache.record_failure();
        })?;
        let id = self.texture_cache.next_id();
        let size = (texture.width as u64) * (texture.height as u64) * 4;

        let meta = AssetMetadata::new(
            id,
            key.to_string(),
            AssetTypeId::Texture,
            source,
            size,
            texture.format.clone(),
        );

        let lifetime = ResourceLifetime::new();
        let handle = AssetHandle::new(id, key.to_string(), Arc::clone(&lifetime));
        self.texture_cache
            .insert(key.to_string(), texture, meta, lifetime, id);
        Ok(handle)
    }

    /// Get a handle to a cached texture. Returns `None` if not loaded.
    pub fn get_texture(&self, key: &str) -> Option<AssetHandle<TextureAsset>> {
        self.texture_cache.get(key)
    }

    /// Access the raw texture data from cache.
    pub fn get_texture_data(&self, key: &str) -> Option<TextureAsset> {
        self.texture_cache.get_data(key)
    }

    /// Check if a texture is cached.
    pub fn has_texture(&self, key: &str) -> bool {
        self.texture_cache.contains(key)
    }

    /// Remove a texture from the cache.
    pub fn release_texture(&self, key: &str) {
        self.texture_cache.remove(key);
    }

    // ── Audio Assets ────────────────────────────────────────────────

    /// Load an audio asset from raw PCM samples.
    pub fn load_audio(
        &self,
        key: &str,
        samples: Vec<i16>,
        source: AssetSource,
    ) -> AssetHandle<AudioAsset> {
        if let Some(handle) = self.audio_cache.get(key) {
            return handle;
        }

        let audio = loader::AudioLoader::load(samples);
        let id = self.audio_cache.next_id();
        let size = audio.memory_bytes() as u64;

        let meta = AssetMetadata::new(
            id,
            key.to_string(),
            AssetTypeId::Audio,
            source,
            size,
            "pcm_i16_44100".into(),
        );

        let lifetime = ResourceLifetime::new();
        let handle = AssetHandle::new(id, key.to_string(), Arc::clone(&lifetime));
        self.audio_cache
            .insert(key.to_string(), audio, meta, lifetime, id);
        handle
    }

    /// Get a handle to a cached audio asset.
    pub fn get_audio(&self, key: &str) -> Option<AssetHandle<AudioAsset>> {
        self.audio_cache.get(key)
    }

    /// Access raw audio data from cache.
    pub fn get_audio_data(&self, key: &str) -> Option<AudioAsset> {
        self.audio_cache.get_data(key)
    }

    /// Check if an audio asset is cached.
    pub fn has_audio(&self, key: &str) -> bool {
        self.audio_cache.contains(key)
    }

    /// Remove an audio asset from the cache.
    pub fn release_audio(&self, key: &str) {
        self.audio_cache.remove(key);
    }

    // ── Lua Source Assets ──────────────────────────────────────────

    /// Load a Lua source file from raw bytes.
    pub fn load_lua_source(
        &self,
        key: &str,
        data: &[u8],
        source: AssetSource,
    ) -> Result<AssetHandle<LuaSourceAsset>, loader::LoaderError> {
        if let Some(handle) = self.lua_cache.get(key) {
            return Ok(handle);
        }

        loader::LuaSourceLoader::validate(data).inspect_err(|_| {
            self.lua_cache.record_failure();
        })?;
        let lua_asset = loader::LuaSourceLoader::load(data).inspect_err(|_| {
            self.lua_cache.record_failure();
        })?;
        let id = self.lua_cache.next_id();
        let size = lua_asset.source.len() as u64;

        let meta = AssetMetadata::new(
            id,
            key.to_string(),
            AssetTypeId::LuaSource,
            source,
            size,
            "lua".into(),
        );

        let lifetime = ResourceLifetime::new();
        let handle = AssetHandle::new(id, key.to_string(), Arc::clone(&lifetime));
        self.lua_cache
            .insert(key.to_string(), lua_asset, meta, lifetime, id);
        Ok(handle)
    }

    /// Get a handle to a cached Lua source asset.
    pub fn get_lua_source(&self, key: &str) -> Option<AssetHandle<LuaSourceAsset>> {
        self.lua_cache.get(key)
    }

    /// Access raw Lua source from cache.
    pub fn get_lua_source_data(&self, key: &str) -> Option<String> {
        self.lua_cache.get_data(key).map(|a| a.source)
    }

    /// Check if a Lua source is cached.
    pub fn has_lua_source(&self, key: &str) -> bool {
        self.lua_cache.contains(key)
    }

    /// Remove a Lua source from the cache.
    pub fn release_lua_source(&self, key: &str) {
        self.lua_cache.remove(key);
    }

    // ── Raw Assets ─────────────────────────────────────────────────

    /// Load a raw binary asset.
    pub fn load_raw(
        &self,
        key: &str,
        data: Vec<u8>,
        source: AssetSource,
    ) -> Result<AssetHandle<RawAsset>, loader::LoaderError> {
        if let Some(handle) = self.raw_cache.get(key) {
            return Ok(handle);
        }

        loader::RawLoader::validate(&data).inspect_err(|_| {
            self.raw_cache.record_failure();
        })?;
        let raw = loader::RawLoader::load(data);
        let id = self.raw_cache.next_id();
        let size = raw.data.len() as u64;

        let meta = AssetMetadata::new(
            id,
            key.to_string(),
            AssetTypeId::Raw,
            source,
            size,
            raw.mime_type.clone(),
        );

        let lifetime = ResourceLifetime::new();
        let handle = AssetHandle::new(id, key.to_string(), Arc::clone(&lifetime));
        self.raw_cache
            .insert(key.to_string(), raw, meta, lifetime, id);
        Ok(handle)
    }

    /// Get a handle to a cached raw asset.
    pub fn get_raw(&self, key: &str) -> Option<AssetHandle<RawAsset>> {
        self.raw_cache.get(key)
    }

    /// Access raw data from cache.
    pub fn get_raw_data(&self, key: &str) -> Option<Vec<u8>> {
        self.raw_cache.get_data(key).map(|a| a.data)
    }

    // ── Package Assets ─────────────────────────────────────────────

    /// Mount a .vibepkg archive and cache it.
    pub fn mount_package(
        &self,
        key: &str,
        data: &[u8],
    ) -> Result<AssetHandle<PackageAsset>, loader::LoaderError> {
        if let Some(handle) = self.package_cache.get(key) {
            return Ok(handle);
        }

        let pkg = package::PackageMount::mount(data, key).inspect_err(|_| {
            self.package_cache.record_failure();
        })?;
        let id = self.package_cache.next_id();
        let size = pkg.entries().iter().map(|(_, _, s)| s).sum::<u64>();

        let meta = AssetMetadata::new(
            id,
            key.to_string(),
            AssetTypeId::Package,
            AssetSource::Memory,
            size,
            "vibepkg".into(),
        );

        let lifetime = ResourceLifetime::new();
        let handle = AssetHandle::new(id, key.to_string(), Arc::clone(&lifetime));
        self.package_cache
            .insert(key.to_string(), pkg, meta, lifetime, id);
        Ok(handle)
    }

    /// Get a handle to a cached package.
    pub fn get_package(&self, key: &str) -> Option<AssetHandle<PackageAsset>> {
        self.package_cache.get(key)
    }

    /// Access package data from cache.
    pub fn get_package_data(&self, key: &str) -> Option<PackageAsset> {
        self.package_cache.get_data(key)
    }

    /// Check if a package is mounted.
    pub fn has_package(&self, key: &str) -> bool {
        self.package_cache.contains(key)
    }

    // ── Asset Existence Check ──────────────────────────────────────

    /// Check whether an asset with the given key exists in any cache.
    pub fn exists(&self, key: &str) -> bool {
        self.texture_cache.contains(key)
            || self.audio_cache.contains(key)
            || self.lua_cache.contains(key)
            || self.raw_cache.contains(key)
            || self.package_cache.contains(key)
    }

    // ── Statistics ─────────────────────────────────────────────────

    /// Gather aggregate statistics across all caches.
    pub fn statistics(&self) -> AssetStatistics {
        let tex = self.texture_cache.stats("texture");
        let aud = self.audio_cache.stats("audio");
        let lua = self.lua_cache.stats("lua_source");
        let raw = self.raw_cache.stats("raw");
        let pkg = self.package_cache.stats("package");

        let total_assets = tex.count + aud.count + lua.count + raw.count + pkg.count;
        let total_memory = tex.memory_bytes
            + aud.memory_bytes
            + lua.memory_bytes
            + raw.memory_bytes
            + pkg.memory_bytes;
        let total_hits =
            tex.cache_hits + aud.cache_hits + lua.cache_hits + raw.cache_hits + pkg.cache_hits;
        let total_misses = tex.cache_misses
            + aud.cache_misses
            + lua.cache_misses
            + raw.cache_misses
            + pkg.cache_misses;
        let total_loads = self.texture_cache.loads()
            + self.audio_cache.loads()
            + self.lua_cache.loads()
            + self.raw_cache.loads()
            + self.package_cache.loads();
        let total_releases = self.texture_cache.releases()
            + self.audio_cache.releases()
            + self.lua_cache.releases()
            + self.raw_cache.releases()
            + self.package_cache.releases();

        let total_failed = tex.failed_loads
            + aud.failed_loads
            + lua.failed_loads
            + raw.failed_loads
            + pkg.failed_loads;

        let mut breakdown = std::collections::HashMap::new();
        breakdown.insert("texture", tex);
        breakdown.insert("audio", aud);
        breakdown.insert("lua_source", lua);
        breakdown.insert("raw", raw);
        breakdown.insert("package", pkg);

        AssetStatistics {
            total_assets,
            total_memory_bytes: total_memory,
            total_cache_hits: total_hits,
            total_cache_misses: total_misses,
            total_loads,
            total_releases,
            total_failed_loads: total_failed,
            asset_type_breakdown: breakdown,
        }
    }

    // ── Lifecycle ──────────────────────────────────────────────────

    /// Release all cached assets.
    pub fn clear(&self) {
        self.texture_cache.clear();
        self.audio_cache.clear();
        self.lua_cache.clear();
        self.raw_cache.clear();
        self.package_cache.clear();
    }

    /// Get metadata for all cached assets.
    pub fn all_metadata(&self) -> Vec<AssetMetadata> {
        let mut all = self.texture_cache.all_metadata();
        all.extend(self.audio_cache.all_metadata());
        all.extend(self.lua_cache.all_metadata());
        all.extend(self.raw_cache.all_metadata());
        all.extend(self.package_cache.all_metadata());
        all
    }
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_manager_new() {
        let am = AssetManager::new();
        assert_eq!(am.statistics().total_assets, 0);
    }

    #[test]
    fn test_asset_manager_load_and_get_texture() {
        // Create a tiny valid PNG
        let png_bytes = create_test_png(2, 2);
        let am = AssetManager::new();
        am.set_texture_loader(Box::new(|data, _source| {
            let (_rgba, w, h) = loader::TextureLoader::load(data)?;
            Ok(TextureAsset {
                bind_group_index: 0,
                width: w,
                height: h,
                format: "rgba8".into(),
            })
        }));

        let handle = am
            .load_texture("test_tex", &png_bytes, AssetSource::Memory)
            .unwrap();
        assert_eq!(handle.key(), "test_tex");
        assert_eq!(handle.ref_count(), 1);

        let cached = am.get_texture_data("test_tex");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().width, 2);

        assert!(am.has_texture("test_tex"));
        drop(handle);
    }

    #[test]
    fn test_asset_manager_texture_dedup() {
        let png_bytes = create_test_png(2, 2);
        let am = AssetManager::new();
        am.set_texture_loader(Box::new(|data, _source| {
            let (_rgba, w, h) = loader::TextureLoader::load(data)?;
            Ok(TextureAsset {
                bind_group_index: 0,
                width: w,
                height: h,
                format: "rgba8".into(),
            })
        }));

        let h1 = am
            .load_texture("dedup", &png_bytes, AssetSource::Memory)
            .unwrap();
        let h2 = am
            .load_texture("dedup", &png_bytes, AssetSource::Memory)
            .unwrap();

        assert_eq!(h1.id(), h2.id());
        assert_eq!(am.texture_cache.len(), 1);
        drop(h1);
        drop(h2);
    }

    #[test]
    fn test_asset_manager_audio() {
        let am = AssetManager::new();
        let handle = am.load_audio(
            "test_wav",
            vec![0i16; 44100],
            AssetSource::Procedural("sine".into()),
        );
        assert_eq!(handle.key(), "test_wav");
        assert!(am.has_audio("test_wav"));
        let data = am.get_audio_data("test_wav");
        assert!(data.is_some());
        assert_eq!(data.unwrap().samples.len(), 44100);
        drop(handle);
    }

    #[test]
    fn test_asset_manager_lua_source() {
        let am = AssetManager::new();
        let handle = am
            .load_lua_source(
                "main.lua",
                b"print('hello')",
                AssetSource::File("main.lua".into()),
            )
            .unwrap();
        assert_eq!(handle.key(), "main.lua");
        assert!(am.has_lua_source("main.lua"));
        assert_eq!(
            am.get_lua_source_data("main.lua"),
            Some("print('hello')".to_string())
        );
        drop(handle);
    }

    #[test]
    fn test_asset_manager_raw() {
        let am = AssetManager::new();
        let handle = am
            .load_raw("data.bin", vec![0, 1, 2, 3], AssetSource::Memory)
            .unwrap();
        assert_eq!(am.get_raw_data("data.bin"), Some(vec![0, 1, 2, 3]));
        drop(handle);
    }

    #[test]
    fn test_asset_manager_exists() {
        let am = AssetManager::new();
        assert!(!am.exists("nothing"));
        let _h = am.load_audio("beep", vec![0i16; 100], AssetSource::Memory);
        assert!(am.exists("beep"));
    }

    #[test]
    fn test_asset_manager_statistics() {
        let am = AssetManager::new();
        let _h1 = am.load_audio("a", vec![0i16; 100], AssetSource::Memory);
        let _h2 = am.load_audio("b", vec![0i16; 200], AssetSource::Memory);
        let stats = am.statistics();
        assert_eq!(stats.total_assets, 2);
        assert!(stats.total_memory_bytes > 0);
    }

    #[test]
    fn test_asset_manager_clear() {
        let am = AssetManager::new();
        let _h = am.load_audio("test", vec![0i16; 100], AssetSource::Memory);
        assert_eq!(am.statistics().total_assets, 1);
        am.clear();
        assert_eq!(am.statistics().total_assets, 0);
    }

    #[test]
    fn test_asset_manager_all_metadata() {
        let am = AssetManager::new();
        let _h = am.load_audio("test", vec![0i16; 100], AssetSource::Memory);
        let meta = am.all_metadata();
        assert_eq!(meta.len(), 1);
        assert_eq!(meta[0].key, "test");
        assert_eq!(meta[0].asset_type, AssetTypeId::Audio);
    }

    /// Helper: creates a minimal valid 2×2 red PNG.
    fn create_test_png(w: u32, h: u32) -> Vec<u8> {
        use image::RgbaImage;
        let mut img = RgbaImage::new(w, h);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([255, 0, 0, 255]);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)
            .expect("PNG write");
        buf.into_inner()
    }
}
