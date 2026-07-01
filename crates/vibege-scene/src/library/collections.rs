use std::sync::Mutex;

use super::models::{Collection, CollectionKind, InstalledGame};

/// Manages auto-generated and user-defined game collections.
pub struct CollectionManager {
    collections: Mutex<Vec<Collection>>,
}

impl CollectionManager {
    pub fn new() -> Self {
        let collections = vec![
            Collection::new("Favorites", CollectionKind::Favorites),
            Collection::new("Recently Played", CollectionKind::RecentlyPlayed),
            Collection::new("Recently Installed", CollectionKind::RecentlyInstalled),
            Collection::new("Most Played", CollectionKind::MostPlayed),
            Collection::new("Pinned", CollectionKind::Pinned),
            Collection::new("Hidden", CollectionKind::Hidden),
        ];

        Self {
            collections: Mutex::new(collections),
        }
    }

    /// Rebuild all auto-collections from the current game list.
    pub fn rebuild(&self, games: &[InstalledGame]) {
        let mut collections = self.collections.lock().expect("collections lock");

        for collection in collections.iter_mut() {
            match collection.kind {
                CollectionKind::Favorites => {
                    collection.game_names = games
                        .iter()
                        .filter(|g| !g.hidden)
                        .map(|g| g.name.clone())
                        .collect();
                    // In a real implementation, persist favorites separately
                }
                CollectionKind::RecentlyPlayed => {
                    let mut sorted: Vec<_> = games.iter().filter(|g| g.last_played > 0).collect();
                    sorted.sort_by_key(|k| std::cmp::Reverse(k.last_played));
                    collection.game_names =
                        sorted.iter().take(20).map(|g| g.name.clone()).collect();
                }
                CollectionKind::RecentlyInstalled => {
                    let mut sorted: Vec<_> = games.iter().collect();
                    sorted.sort_by_key(|k| std::cmp::Reverse(k.installed_at));
                    collection.game_names =
                        sorted.iter().take(20).map(|g| g.name.clone()).collect();
                }
                CollectionKind::MostPlayed => {
                    let mut sorted: Vec<_> = games.iter().collect();
                    sorted.sort_by_key(|k| std::cmp::Reverse(k.play_count));
                    collection.game_names =
                        sorted.iter().take(20).map(|g| g.name.clone()).collect();
                }
                CollectionKind::Pinned => {
                    collection.game_names = games
                        .iter()
                        .filter(|g| g.pinned)
                        .map(|g| g.name.clone())
                        .collect();
                }
                CollectionKind::Hidden => {
                    collection.game_names = games
                        .iter()
                        .filter(|g| g.hidden)
                        .map(|g| g.name.clone())
                        .collect();
                }
                CollectionKind::Custom => {
                    // Custom collections are user-defined, not auto-generated
                }
            }
        }
    }

    pub fn all(&self) -> Vec<Collection> {
        self.collections.lock().expect("collections lock").clone()
    }

    /// Get game names for a specific collection kind.
    pub fn by_kind(&self, kind: CollectionKind) -> Vec<String> {
        self.collections
            .lock()
            .expect("collections lock")
            .iter()
            .find(|c| c.kind == kind)
            .map(|c| c.game_names.clone())
            .unwrap_or_default()
    }

    pub fn get(&self, name: &str) -> Option<Collection> {
        let collections = self.collections.lock().expect("collections lock");
        collections.iter().find(|c| c.name == name).cloned()
    }

    pub fn add_custom(&self, name: &str) -> Result<(), String> {
        let mut collections = self.collections.lock().expect("collections lock");
        if collections.iter().any(|c| c.name == name) {
            return Err(format!("Collection '{name}' already exists"));
        }
        collections.push(Collection::new(name, CollectionKind::Custom));
        Ok(())
    }

    pub fn remove_custom(&self, name: &str) -> Result<(), String> {
        let mut collections = self.collections.lock().expect("collections lock");
        let pos = collections
            .iter()
            .position(|c| c.name == name && c.kind == CollectionKind::Custom)
            .ok_or_else(|| format!("Custom collection '{name}' not found"))?;
        collections.remove(pos);
        Ok(())
    }

    pub fn add_to_collection(&self, collection_name: &str, game_name: &str) {
        let mut collections = self.collections.lock().expect("collections lock");
        if let Some(collection) = collections.iter_mut().find(|c| c.name == collection_name) {
            if !collection.game_names.contains(&game_name.to_string()) {
                collection.game_names.push(game_name.to_string());
            }
        }
    }

    pub fn remove_from_collection(&self, collection_name: &str, game_name: &str) {
        let mut collections = self.collections.lock().expect("collections lock");
        if let Some(collection) = collections.iter_mut().find(|c| c.name == collection_name) {
            collection.game_names.retain(|g| g != game_name);
        }
    }

    pub fn is_favorite(&self, game_name: &str) -> bool {
        let collections = self.collections.lock().expect("collections lock");
        collections
            .iter()
            .find(|c| c.kind == CollectionKind::Favorites)
            .map(|c| c.game_names.contains(&game_name.to_string()))
            .unwrap_or(false)
    }

    pub fn pinned_games(&self) -> Vec<String> {
        let collections = self.collections.lock().expect("collections lock");
        collections
            .iter()
            .find(|c| c.kind == CollectionKind::Pinned)
            .map(|c| c.game_names.clone())
            .unwrap_or_default()
    }
}

impl Default for CollectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_game(name: &str, played: u64, installed: u64, count: u64) -> InstalledGame {
        InstalledGame {
            name: name.to_string(),
            path: PathBuf::new(),
            entry_point: "main.lua".into(),
            version: "1.0".into(),
            author: "".into(),
            description: "".into(),
            installed_at: installed,
            last_played: played,
            play_count: count,
            total_play_time_secs: 0,
            size_bytes: 0,
            engine_version: "0.2.0".into(),
            category: "".into(),
            genres: vec![],
            tags: vec![],
            hidden: false,
            pinned: false,
        }
    }

    #[test]
    fn test_collection_manager_new() {
        let cm = CollectionManager::new();
        assert_eq!(cm.all().len(), 6);
    }

    #[test]
    fn test_rebuild_collections() {
        let cm = CollectionManager::new();
        let games = vec![
            sample_game("Pong", 100, 50, 10),
            sample_game("Chess", 200, 30, 5),
        ];
        cm.rebuild(&games);

        let recent = cm.get("Recently Played").unwrap();
        assert_eq!(recent.game_names[0], "Chess"); // most recently played
    }

    #[test]
    fn test_add_custom_collection() {
        let cm = CollectionManager::new();
        assert!(cm.add_custom("My Collection").is_ok());
        assert!(cm.add_custom("My Collection").is_err()); // duplicate
        assert_eq!(cm.all().len(), 7);
    }

    #[test]
    fn test_remove_custom_collection() {
        let cm = CollectionManager::new();
        cm.add_custom("Test").unwrap();
        assert!(cm.remove_custom("Test").is_ok());
        assert!(cm.remove_custom("Test").is_err()); // already removed
    }

    #[test]
    fn test_add_to_collection() {
        let cm = CollectionManager::new();
        cm.add_to_collection("Favorites", "Pong");
        assert!(cm.is_favorite("Pong"));
    }

    #[test]
    fn test_remove_from_collection() {
        let cm = CollectionManager::new();
        cm.add_to_collection("Favorites", "Pong");
        assert!(cm.is_favorite("Pong"));
        cm.remove_from_collection("Favorites", "Pong");
        assert!(!cm.is_favorite("Pong"));
    }

    #[test]
    fn test_pinned_games() {
        let mut game = sample_game("Pong", 0, 0, 0);
        game.pinned = true;
        let cm = CollectionManager::new();
        cm.rebuild(&[game]);
        assert_eq!(cm.pinned_games(), vec!["Pong"]);
    }
}
