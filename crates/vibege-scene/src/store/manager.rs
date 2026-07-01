use std::io::Read;
use std::sync::Arc;
use std::sync::Mutex;

use tracing::info;

use super::cache::StoreCache;
use super::discovery::DiscoveryEngine;
use super::download::DownloadQueue;
use super::metadata::MetadataProvider;
use super::models::{GameListing, SearchQuery, StoreSection};
use super::search::SearchEngine;

/// Top-level store manager that coordinates fetching, caching,
/// searching, discovery, and downloads.
pub struct StoreManager {
    /// Backend API URL.
    backend: String,
    /// Metadata and search cache.
    cache: Arc<StoreCache>,
    /// Download queue.
    downloads: Arc<DownloadQueue>,
    /// Cached listings (parsed from API).
    listings: Mutex<Vec<GameListing>>,
    /// Currently active sections.
    sections: Mutex<Vec<StoreSection>>,
    /// IDs of installed games.
    installed_ids: Mutex<Vec<String>>,
    /// Error state.
    error: Mutex<Option<String>>,
    /// Loading state.
    loading: Mutex<bool>,
}

impl StoreManager {
    pub fn new(backend: String) -> Self {
        Self {
            backend,
            cache: Arc::new(StoreCache::new()),
            downloads: Arc::new(DownloadQueue::new(3, 3)),
            listings: Mutex::new(Vec::new()),
            sections: Mutex::new(Vec::new()),
            installed_ids: Mutex::new(Vec::new()),
            error: Mutex::new(None),
            loading: Mutex::new(false),
        }
    }

    pub fn cache(&self) -> &Arc<StoreCache> {
        &self.cache
    }

    pub fn downloads(&self) -> &Arc<DownloadQueue> {
        &self.downloads
    }

    pub fn listings(&self) -> Vec<GameListing> {
        self.listings.lock().expect("listings lock").clone()
    }

    pub fn sections(&self) -> Vec<StoreSection> {
        self.sections.lock().expect("sections lock").clone()
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().expect("error lock").clone()
    }

    pub fn loading(&self) -> bool {
        *self.loading.lock().expect("loading lock")
    }

    /// Fetch listings from the backend API.
    pub fn fetch(&self, page: u32) {
        if self.loading() {
            return;
        }
        *self.loading.lock().expect("loading lock") = true;
        *self.error.lock().expect("error lock") = None;

        let url = format!("{}/registry?limit=50&offset={}", self.backend, page * 50);

        match ureq::get(&url).call() {
            Ok(resp) => {
                let mut reader = resp.into_body().into_reader();
                let mut body = String::new();
                if reader.read_to_string(&mut body).is_ok() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        let listings = MetadataProvider::parse_listings(&json);
                        self.cache.cache_listings(listings.clone(), 300);
                        *self.listings.lock().expect("listings lock") = listings.clone();

                        // Build discovery sections
                        let installed = self.installed_ids.lock().expect("installed lock").clone();
                        let sections = DiscoveryEngine::build_sections(&listings, &installed);
                        *self.sections.lock().expect("sections lock") = sections;

                        info!(
                            "Store: fetched {} games",
                            self.listings.lock().expect("listings lock").len()
                        );
                    }
                }
            }
            Err(e) => {
                // Try cache
                let cached = self.cache.get_all_listings();
                if !cached.is_empty() {
                    *self.listings.lock().expect("listings lock") = cached.clone();
                    let installed = self.installed_ids.lock().expect("installed lock").clone();
                    let sections = DiscoveryEngine::build_sections(&cached, &installed);
                    *self.sections.lock().expect("sections lock") = sections;
                    self.cache.set_offline(true);
                    info!("Store: serving {} cached games offline", cached.len());
                } else {
                    *self.error.lock().expect("error lock") = Some(format!("HTTP: {e}"));
                }
            }
        }

        *self.loading.lock().expect("loading lock") = false;
    }

    /// Search cached listings.
    pub fn search(&self, query: &SearchQuery) -> Vec<GameListing> {
        let listings = self.listings.lock().expect("listings lock");
        SearchEngine::search(&listings, query)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get a single listing by ID.
    pub fn get_listing(&self, id: &str) -> Option<GameListing> {
        self.cache.get_listing(id).or_else(|| {
            let listings = self.listings.lock().expect("listings lock");
            listings.iter().find(|l| l.id == id).cloned()
        })
    }

    /// Set installed game IDs for recommendations.
    pub fn set_installed_ids(&self, ids: Vec<String>) {
        *self.installed_ids.lock().expect("installed lock") = ids;
    }

    /// Download a package by game ID.
    pub fn download_package(&self, id: &str) -> Result<Vec<u8>, String> {
        let mut data: Vec<u8> = Vec::new();
        ureq::get(&format!("{}/registry/{}/download", self.backend, id))
            .call()
            .map_err(|e| format!("Download HTTP: {e}"))?
            .into_body()
            .into_reader()
            .read_to_end(&mut data)
            .map_err(|e| format!("Download read: {e}"))?;
        Ok(data)
    }

    pub fn install_package(&self, data: &[u8], name: &str) -> Result<(), String> {
        install_package_impl(data, name)
    }

    /// Clear all cached data.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Refresh listings from the backend.
    pub fn refresh(&self) {
        self.cache.invalidate_listings();
        self.fetch(0);
    }

    pub fn backend_url(&self) -> &str {
        &self.backend
    }
}

/// Install a .vibepkg buffer to the game library.
fn install_package_impl(data: &[u8], name: &str) -> Result<(), String> {
    use crate::runtime::validator::PackageValidator;
    use std::io::Write;

    if data.len() < 4 || data[0] != 0x50 || data[1] != 0x4B || data[2] != 0x03 || data[3] != 0x04 {
        return Err("Invalid .vibepkg: not a ZIP archive".into());
    }
    let install_dir = vibege_config::installed_games_dir().join(sanitize_name(name));
    std::fs::create_dir_all(&install_dir).map_err(|e| format!("Create dir: {e}"))?;

    let mut entry_point = String::from("src/main.lua");
    let mut version = String::from("0.1.0");
    let mut author = String::new();

    let cursor = std::io::Cursor::new(data);
    match zip::ZipArchive::new(cursor) {
        Ok(mut archive) => {
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).map_err(|e| format!("ZIP entry: {e}"))?;
                if entry.is_dir() {
                    continue;
                }
                let safe_path = PackageValidator::sanitize_path(&install_dir, entry.name())
                    .map_err(|e| format!("Invalid entry path: {e}"))?;
                if let Some(parent) = safe_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("Dir: {e}"))?;
                }
                let mut content = Vec::new();
                entry
                    .read_to_end(&mut content)
                    .map_err(|e| format!("Read: {e}"))?;
                if entry.name() == "vibege.json" || entry.name() == "manifest.json" {
                    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&content) {
                        if let Some(ep) = json["entry"].as_str() {
                            entry_point = ep.to_string();
                        }
                        if let Some(v) = json["version"].as_str() {
                            version = v.to_string();
                        }
                        if let Some(a) = json["author"].as_str() {
                            author = a.to_string();
                        }
                    }
                }
                let mut f =
                    std::fs::File::create(&safe_path).map_err(|e| format!("Create: {e}"))?;
                f.write_all(&content).map_err(|e| format!("Write: {e}"))?;
            }
        }
        Err(e) => return Err(format!("Invalid ZIP: {e}")),
    }

    let meta = serde_json::json!({
        "name": name,
        "entry": entry_point,
        "version": version,
        "author": author,
        "installed_at": format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()),
    });
    let meta_path = install_dir.join(".vibege-install.json");
    std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Meta: {e}"))?;
    Ok(())
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_initial_state() {
        let mgr = StoreManager::new("http://localhost:3000/api/v1".into());
        assert_eq!(mgr.backend_url(), "http://localhost:3000/api/v1");
        assert!(!mgr.loading());
        assert!(mgr.error().is_none());
        assert!(mgr.listings().is_empty());
        assert!(mgr.sections().is_empty());
    }
}
