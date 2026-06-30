use std::sync::Mutex;

use super::models::PlayRecord;

/// Tracks play sessions and maintains play history.
pub struct PlayHistory {
    records: Mutex<Vec<PlayRecord>>,
    max_records: usize,
}

impl PlayHistory {
    pub fn new(max_records: usize) -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            max_records,
        }
    }

    /// Record a play session for a game.
    pub fn record_play(&self, game_name: &str, duration_secs: u64) {
        let mut records = self.records.lock().expect("history lock");
        let now = timestamp_now();

        records.push(PlayRecord {
            game_name: game_name.to_string(),
            timestamp: now,
            duration_secs,
        });

        // Trim to max_records
        while records.len() > self.max_records {
            records.remove(0);
        }
    }

    /// Get all play records.
    pub fn all(&self) -> Vec<PlayRecord> {
        self.records.lock().expect("history lock").clone()
    }

    /// Get recently played game names (most recent first).
    pub fn recently_played(&self, limit: usize) -> Vec<String> {
        let mut records = self.records.lock().expect("history lock").clone();
        records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        records
            .iter()
            .take(limit)
            .map(|r| r.game_name.clone())
            .collect()
    }

    /// Get total play time for a specific game.
    pub fn total_play_time(&self, game_name: &str) -> u64 {
        self.records
            .lock()
            .expect("history lock")
            .iter()
            .filter(|r| r.game_name == game_name)
            .map(|r| r.duration_secs)
            .sum()
    }

    /// Get total play sessions for a specific game.
    pub fn play_count(&self, game_name: &str) -> usize {
        self.records
            .lock()
            .expect("history lock")
            .iter()
            .filter(|r| r.game_name == game_name)
            .count()
    }

    /// Get the last played timestamp for a game.
    pub fn last_played(&self, game_name: &str) -> Option<u64> {
        self.records
            .lock()
            .expect("history lock")
            .iter()
            .filter(|r| r.game_name == game_name)
            .map(|r| r.timestamp)
            .max()
    }

    /// Clear all history.
    pub fn clear(&self) {
        self.records.lock().expect("history lock").clear();
    }
}

impl Default for PlayHistory {
    fn default() -> Self {
        Self::new(1000)
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
    fn test_history_empty() {
        let history = PlayHistory::new(100);
        assert!(history.all().is_empty());
        assert!(history.recently_played(10).is_empty());
    }

    #[test]
    fn test_record_play() {
        let history = PlayHistory::new(100);
        history.record_play("Pong", 120);
        assert_eq!(history.all().len(), 1);
        assert_eq!(history.recently_played(10), vec!["Pong"]);
    }

    #[test]
    fn test_play_time_accumulation() {
        let history = PlayHistory::new(100);
        history.record_play("Pong", 120);
        history.record_play("Pong", 60);
        assert_eq!(history.total_play_time("Pong"), 180);
    }

    #[test]
    fn test_play_count() {
        let history = PlayHistory::new(100);
        history.record_play("Pong", 10);
        history.record_play("Pong", 20);
        history.record_play("Chess", 30);
        assert_eq!(history.play_count("Pong"), 2);
        assert_eq!(history.play_count("Chess"), 1);
    }

    #[test]
    fn test_last_played() {
        let history = PlayHistory::new(100);
        history.record_play("Pong", 10);
        assert!(history.last_played("Pong").is_some());
        assert!(history.last_played("Nonexistent").is_none());
    }

    #[test]
    fn test_max_records() {
        let history = PlayHistory::new(3);
        history.record_play("A", 10);
        history.record_play("B", 10);
        history.record_play("C", 10);
        history.record_play("D", 10);
        assert_eq!(history.all().len(), 3);
        // The oldest entry (A) should have been removed
        assert!(history.recently_played(10).contains(&"D".to_string()));
    }

    #[test]
    fn test_clear() {
        let history = PlayHistory::new(100);
        history.record_play("Pong", 10);
        history.clear();
        assert!(history.all().is_empty());
    }
}
