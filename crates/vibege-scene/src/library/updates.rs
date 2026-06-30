use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;

use super::models::InstalledGame;

/// Manages checking and tracking available game updates.
pub struct UpdateManager {
    available: Mutex<HashMap<String, String>>,
    skipped: Mutex<HashMap<String, String>>,
    backend: String,
}

impl UpdateManager {
    pub fn new(backend: String) -> Self {
        Self {
            available: Mutex::new(HashMap::new()),
            skipped: Mutex::new(HashMap::new()),
            backend,
        }
    }

    /// Check for updates for all installed games against the backend.
    pub fn scan(&self, games: &[InstalledGame]) -> HashMap<String, String> {
        let mut available = HashMap::new();

        for game in games {
            let url = format!("{}/registry/{}", self.backend, urlencoding(&game.name));
            if let Ok(resp) = ureq::get(&url).call() {
                let mut body = String::new();
                if resp
                    .into_body()
                    .into_reader()
                    .read_to_string(&mut body)
                    .is_ok()
                {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        let latest = json["package"]["updatedAt"].as_str().unwrap_or("");
                        if !latest.is_empty() && latest != game.version {
                            // Check if this version was skipped
                            let is_skipped = self
                                .skipped
                                .lock()
                                .expect("skipped lock")
                                .get(&game.name)
                                .map(|v| v.as_str() == latest)
                                .unwrap_or(false);
                            if !is_skipped {
                                available.insert(game.name.clone(), latest.to_string());
                            }
                        }
                    }
                }
            }
        }

        *self.available.lock().expect("updates lock") = available.clone();
        available
    }

    /// Get the currently available updates.
    pub fn available(&self) -> HashMap<String, String> {
        self.available.lock().expect("updates lock").clone()
    }

    /// Check if a specific game has an update.
    pub fn has_update(&self, game_name: &str) -> bool {
        self.available
            .lock()
            .expect("updates lock")
            .contains_key(game_name)
    }

    /// Skip a specific version of a game (don't show update).
    pub fn skip_version(&self, game_name: &str, version: &str) {
        self.skipped
            .lock()
            .expect("skipped lock")
            .insert(game_name.to_string(), version.to_string());
        self.available
            .lock()
            .expect("updates lock")
            .remove(game_name);
    }

    /// Clear all skipped versions.
    pub fn clear_skipped(&self) {
        self.skipped.lock().expect("skipped lock").clear();
    }

    /// Number of available updates.
    pub fn count(&self) -> usize {
        self.available.lock().expect("updates lock").len()
    }
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c.to_string(),
            ' ' => "+".into(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_manager_new() {
        let um = UpdateManager::new("http://localhost:3000/api/v1".into());
        assert_eq!(um.count(), 0);
    }

    #[test]
    fn test_skip_version() {
        let um = UpdateManager::new("http://localhost:3000/api/v1".into());
        // Simulate an available update
        um.available
            .lock()
            .expect("lock")
            .insert("Pong".into(), "2.0.0".into());
        assert!(um.has_update("Pong"));
        um.skip_version("Pong", "2.0.0");
        assert!(!um.has_update("Pong"));
    }

    #[test]
    fn test_clear_skipped() {
        let um = UpdateManager::new("http://localhost:3000/api/v1".into());
        um.skipped
            .lock()
            .expect("lock")
            .insert("Pong".into(), "1.0.0".into());
        um.clear_skipped();
        assert!(um.skipped.lock().expect("lock").is_empty());
    }
}
