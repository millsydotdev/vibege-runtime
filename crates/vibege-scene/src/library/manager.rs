use super::collections::CollectionManager;
use super::history::PlayHistory;
use super::integrity::{IntegrityChecker, IntegrityReport};
use super::models::{InstalledGame, LibraryQuery};
use super::registry::InstalledGameRegistry;
use super::search::LibrarySearchEngine;
use super::updates::UpdateManager;

/// Top-level orchestrator for all library operations.
pub struct LibraryManager {
    pub registry: InstalledGameRegistry,
    pub collections: CollectionManager,
    pub history: PlayHistory,
    pub updates: UpdateManager,
    backend: String,
}

impl LibraryManager {
    pub fn new(backend: String) -> Self {
        let registry = InstalledGameRegistry::new();
        let collections = CollectionManager::new();
        let history = PlayHistory::new(1000);
        let updates = UpdateManager::new(backend.clone());

        Self {
            registry,
            collections,
            history,
            updates,
            backend,
        }
    }

    /// Initialize the library: scan games, rebuild collections, check updates.
    pub fn initialize(&self) {
        let games = self.registry.scan();
        self.collections.rebuild(&games);
        self.updates.scan(&games);
    }

    /// Refresh the library from disk.
    pub fn refresh(&self) {
        let games = self.registry.scan();
        self.collections.rebuild(&games);
    }

    /// Get all installed games.
    pub fn games(&self) -> Vec<InstalledGame> {
        self.registry.all()
    }

    /// Search and filter games.
    pub fn search(&self, query: &LibraryQuery) -> Vec<InstalledGame> {
        let games = self.registry.all();
        LibrarySearchEngine::search(&games, query)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Launch a game by name.
    pub fn launch(&self, game_name: &str) -> Option<InstalledGame> {
        let game = self.registry.get(game_name)?;

        // Update play count and last played
        if let Err(e) = self.registry.update_metadata(
            game_name,
            "last_played",
            &serde_json::json!(timestamp_now()),
        ) {
            tracing::warn!("Failed to update last_played: {e}");
        }

        let new_count = game.play_count + 1;
        if let Err(e) =
            self.registry
                .update_metadata(game_name, "play_count", &serde_json::json!(new_count))
        {
            tracing::warn!("Failed to update play_count: {e}");
        }

        // Record in play history
        self.history.record_play(game_name, 0);

        Some(game)
    }

    /// Toggle favorite status.
    pub fn toggle_favorite(&self, game_name: &str) -> bool {
        let is_fav = self.collections.is_favorite(game_name);
        if is_fav {
            self.collections
                .remove_from_collection("Favorites", game_name);
        } else {
            self.collections.add_to_collection("Favorites", game_name);
        }
        !is_fav
    }

    /// Uninstall a game.
    pub fn uninstall(&self, game_name: &str) -> Result<(), String> {
        self.registry.uninstall(game_name)?;
        self.refresh();
        Ok(())
    }

    /// Check integrity of a game.
    pub fn check_integrity(&self, game_name: &str) -> Option<IntegrityReport> {
        let game = self.registry.get(game_name)?;
        Some(IntegrityChecker::check(&game))
    }

    /// Get update info.
    pub fn available_updates(&self) -> std::collections::HashMap<String, String> {
        self.updates.available()
    }

    pub fn has_update(&self, game_name: &str) -> bool {
        self.updates.has_update(game_name)
    }

    pub fn refresh_updates(&self) {
        let games = self.registry.all();
        self.updates.scan(&games);
    }

    pub fn backend_url(&self) -> &str {
        &self.backend
    }
}

fn timestamp_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_new() {
        let mgr = LibraryManager::new("http://localhost:3000/api/v1".into());
        assert_eq!(mgr.games().len(), 0);
        assert!(!mgr.has_update("nonexistent"));
    }
}
