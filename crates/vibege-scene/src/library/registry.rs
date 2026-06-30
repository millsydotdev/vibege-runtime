use std::collections::HashMap;
use std::sync::Mutex;

use super::models::InstalledGame;

/// Manages the database of installed games by scanning disk metadata.
pub struct InstalledGameRegistry {
    games: Mutex<Vec<InstalledGame>>,
    by_name: Mutex<HashMap<String, usize>>,
    last_scan: Mutex<u64>,
}

impl InstalledGameRegistry {
    pub fn new() -> Self {
        Self {
            games: Mutex::new(Vec::new()),
            by_name: Mutex::new(HashMap::new()),
            last_scan: Mutex::new(0),
        }
    }

    /// Scan the installed games directory and rebuild the registry.
    pub fn scan(&self) -> Vec<InstalledGame> {
        let dir = vibege_config::installed_games_dir();
        let mut games = Vec::new();
        let mut by_name = HashMap::new();

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let meta_path = path.join(".vibege-install.json");
                if !meta_path.exists() {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&meta_path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        let name = json["name"].as_str().unwrap_or("").to_string();
                        if name.is_empty() {
                            continue;
                        }

                        let size: u64 = path
                            .read_dir()
                            .ok()
                            .map(|e| {
                                e.flatten()
                                    .filter_map(|f| f.metadata().ok())
                                    .map(|m| m.len())
                                    .sum()
                            })
                            .unwrap_or(0);

                        let game = InstalledGame {
                            name: name.clone(),
                            path: path.clone(),
                            entry_point: json["entry"]
                                .as_str()
                                .unwrap_or("src/main.lua")
                                .to_string(),
                            version: json["version"].as_str().unwrap_or("0.1.0").to_string(),
                            author: json["author"].as_str().unwrap_or("").to_string(),
                            description: json["description"].as_str().unwrap_or("").to_string(),
                            installed_at: json["installed_at"].as_u64().unwrap_or(0),
                            last_played: json["last_played"].as_u64().unwrap_or(0),
                            play_count: json["play_count"].as_u64().unwrap_or(0),
                            total_play_time_secs: json["total_play_time_secs"]
                                .as_u64()
                                .unwrap_or(0),
                            size_bytes: size,
                            engine_version: json["engine_version"]
                                .as_str()
                                .unwrap_or("0.2.0-alpha.1")
                                .to_string(),
                            category: json["category"].as_str().unwrap_or("").to_string(),
                            genres: json["genres"]
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            tags: json["tags"]
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            hidden: json["hidden"].as_bool().unwrap_or(false),
                            pinned: json["pinned"].as_bool().unwrap_or(false),
                        };

                        by_name.insert(name.clone(), games.len());
                        games.push(game);
                    }
                }
            }
        }

        games.sort_by(|a, b| a.name.cmp(&b.name));

        *self.games.lock().expect("registry lock") = games.clone();
        *self.by_name.lock().expect("registry lock") = by_name;
        *self.last_scan.lock().expect("registry lock") = timestamp_now();

        games
    }

    pub fn all(&self) -> Vec<InstalledGame> {
        self.games.lock().expect("registry lock").clone()
    }

    pub fn get(&self, name: &str) -> Option<InstalledGame> {
        let games = self.games.lock().expect("registry lock");
        let by_name = self.by_name.lock().expect("registry lock");
        by_name.get(name).and_then(|&idx| games.get(idx).cloned())
    }

    pub fn count(&self) -> usize {
        self.games.lock().expect("registry lock").len()
    }

    pub fn update_metadata(
        &self,
        name: &str,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<(), String> {
        let game = self
            .get(name)
            .ok_or_else(|| format!("Game not found: {name}"))?;
        let meta_path = game.path.join(".vibege-install.json");

        let content = std::fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        let mut json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;

        if let Some(obj) = json.as_object_mut() {
            obj.insert(key.to_string(), value.clone());
        }
        std::fs::write(
            &meta_path,
            serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        // Refresh registry
        self.scan();
        Ok(())
    }

    pub fn uninstall(&self, name: &str) -> Result<(), String> {
        let game = self
            .get(name)
            .ok_or_else(|| format!("Game not found: {name}"))?;
        std::fs::remove_dir_all(&game.path).map_err(|e| format!("Uninstall failed: {e}"))?;
        self.scan();
        Ok(())
    }

    pub fn last_scan_time(&self) -> u64 {
        *self.last_scan.lock().expect("registry lock")
    }
}

impl Default for InstalledGameRegistry {
    fn default() -> Self {
        Self::new()
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
    #[allow(unused_imports)]
    use tempfile::tempdir;

    #[test]
    fn test_registry_new() {
        let reg = InstalledGameRegistry::new();
        assert_eq!(reg.count(), 0);
        assert!(reg.all().is_empty());
    }

    #[test]
    fn test_registry_scan_does_not_panic() {
        let reg = InstalledGameRegistry::new();
        let _games = reg.scan(); // Just verify no crash
    }

    #[test]
    fn test_registry_update_metadata_nonexistent() {
        let reg = InstalledGameRegistry::new();
        let result = reg.update_metadata("nonexistent", "key", &serde_json::Value::Null);
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_uninstall_nonexistent() {
        let reg = InstalledGameRegistry::new();
        let result = reg.uninstall("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp() {
        let ts = timestamp_now();
        assert!(ts > 1000000000); // Should be a valid Unix timestamp
    }
}
