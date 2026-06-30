/// Typed metadata for a single game listing from the store.
#[derive(Debug, Clone)]
pub struct GameListing {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub publisher: String,
    pub version: String,
    pub category: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub status: String,
    pub downloads: u64,
    pub file_size: u64,
    pub icon_url: Option<String>,
    pub hero_url: Option<String>,
    pub screenshots: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub engine_version: Option<String>,
    pub rating: f64,
}

impl GameListing {
    pub fn from_json(json: &serde_json::Value) -> Option<Self> {
        let id = json["id"].as_str()?.to_string();
        let name = json["name"].as_str().unwrap_or("").to_string();
        if name.is_empty() {
            return None;
        }
        Some(Self {
            id,
            name,
            description: json["description"].as_str().unwrap_or("").to_string(),
            author: json["author"].as_str().unwrap_or("").to_string(),
            publisher: json["publisher"].as_str().unwrap_or("").to_string(),
            version: json["version"].as_str().unwrap_or("0.1.0").to_string(),
            category: json["category"]
                .as_str()
                .unwrap_or("uncategorized")
                .to_string(),
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
            status: json["status"].as_str().unwrap_or("draft").to_string(),
            downloads: json["downloads"].as_u64().unwrap_or(0),
            file_size: json["file_size"].as_u64().unwrap_or(0),
            icon_url: json["icon_url"].as_str().map(String::from),
            hero_url: json["hero_url"].as_str().map(String::from),
            screenshots: json["screenshots"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            created_at: json["created_at"].as_str().unwrap_or("").to_string(),
            updated_at: json["updated_at"].as_str().unwrap_or("").to_string(),
            engine_version: json["engine_version"].as_str().map(String::from),
            rating: json["rating"].as_f64().unwrap_or(0.0),
        })
    }

    pub fn matches_query(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.name.to_lowercase().contains(&q)
            || self.description.to_lowercase().contains(&q)
            || self.author.to_lowercase().contains(&q)
            || self.genres.iter().any(|g| g.to_lowercase().contains(&q))
            || self.tags.iter().any(|t| t.to_lowercase().contains(&q))
    }

    pub fn fuzzy_score(&self, query: &str) -> f64 {
        let q = query.to_lowercase();
        let name_lower = self.name.to_lowercase();

        // Exact prefix match: highest score
        if name_lower.starts_with(&q) {
            return 1.0;
        }
        // Contains match: high score
        if name_lower.contains(&q) {
            return 0.9;
        }
        // Word boundary match: medium-high
        if name_lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w.starts_with(&q))
        {
            return 0.7;
        }
        // Partial word match: medium
        if name_lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w.contains(&q))
        {
            return 0.5;
        }
        // Genre/tag match: lower
        for genre in &self.genres {
            if genre.to_lowercase().contains(&q) {
                return 0.4;
            }
        }
        for tag in &self.tags {
            if tag.to_lowercase().contains(&q) {
                return 0.3;
            }
        }
        // Description match: lowest
        if self.description.to_lowercase().contains(&q) {
            return 0.2;
        }
        0.0
    }
}

/// Query parameters for searching games.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    pub category: Option<String>,
    pub genre: Option<String>,
    pub tag: Option<String>,
    pub author: Option<String>,
    pub min_rating: Option<f64>,
    pub sort_by: SortField,
    pub sort_order: SortOrder,
    pub installed_filter: Option<bool>,
    pub update_filter: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SortField {
    #[default]
    Relevance,
    Name,
    Downloads,
    Rating,
    Updated,
    Created,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SortOrder {
    #[default]
    Descending,
    Ascending,
}

/// A section on the store front page.
#[derive(Debug, Clone)]
pub struct StoreSection {
    pub title: String,
    pub games: Vec<GameListing>,
    pub section_type: SectionType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SectionType {
    Featured,
    Trending,
    NewReleases,
    RecentlyUpdated,
    TopRated,
    MostDownloaded,
    Recommended,
    Similar,
    Category,
}

/// A download task in the queue.
#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub game_id: String,
    pub game_name: String,
    pub status: DownloadStatus,
    pub progress: f32,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub error: Option<String>,
    pub retry_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Verifying,
    Installing,
    Completed,
    Failed,
    Cancelled,
    Paused,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_listing() -> GameListing {
        GameListing {
            id: "abc123".into(),
            name: "Pong".into(),
            description: "Classic paddle game".into(),
            author: "VibeGE".into(),
            publisher: "".into(),
            version: "1.0.0".into(),
            category: "action".into(),
            genres: vec!["arcade".into(), "sports".into()],
            tags: vec!["multiplayer".into(), "retro".into()],
            status: "approved".into(),
            downloads: 42,
            file_size: 1024,
            icon_url: None,
            hero_url: None,
            screenshots: vec![],
            created_at: "2026-01-01".into(),
            updated_at: "2026-06-01".into(),
            engine_version: None,
            rating: 4.5,
        }
    }

    #[test]
    fn test_listing_from_json_valid() {
        let json = serde_json::json!({
            "id": "game1", "name": "Test Game", "description": "A test",
            "author": "Dev", "version": "0.1.0", "category": "puzzle",
            "status": "approved", "downloads": 100, "file_size": 2048,
            "genres": ["puzzle", "strategy"],
            "rating": 4.2,
        });
        let listing = GameListing::from_json(&json);
        assert!(listing.is_some());
        let l = listing.unwrap();
        assert_eq!(l.name, "Test Game");
        assert_eq!(l.downloads, 100);
        assert_eq!(l.genres, vec!["puzzle", "strategy"]);
        assert!((l.rating - 4.2).abs() < 1e-6);
    }

    #[test]
    fn test_listing_from_json_empty_name() {
        let json = serde_json::json!({"id": "g1", "name": ""});
        assert!(GameListing::from_json(&json).is_none());
    }

    #[test]
    fn test_listing_from_json_missing_fields() {
        let json = serde_json::json!({"id": "g1", "name": "Game"});
        let l = GameListing::from_json(&json).unwrap();
        assert_eq!(l.description, "");
        assert_eq!(l.downloads, 0);
        assert!(l.icon_url.is_none());
    }

    #[test]
    fn test_matches_query_by_name() {
        let listing = sample_listing();
        assert!(listing.matches_query("Pong"));
        assert!(!listing.matches_query("Chess"));
    }

    #[test]
    fn test_matches_query_by_description() {
        let listing = sample_listing();
        assert!(listing.matches_query("paddle"));
        assert!(!listing.matches_query("shooter"));
    }

    #[test]
    fn test_matches_query_by_genre() {
        let listing = sample_listing();
        assert!(listing.matches_query("arcade"));
        assert!(listing.matches_query("sports"));
    }

    #[test]
    fn test_fuzzy_score_exact_prefix() {
        let listing = sample_listing();
        let score = listing.fuzzy_score("Pong");
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_fuzzy_score_contains() {
        let listing = sample_listing();
        let score = listing.fuzzy_score("ong");
        assert!((score - 0.9).abs() < 1e-6);
    }

    #[test]
    fn test_fuzzy_score_genre_match() {
        let listing = sample_listing();
        let score = listing.fuzzy_score("arcade");
        assert!((score - 0.4).abs() < 1e-6);
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        let listing = sample_listing();
        let score = listing.fuzzy_score("zzzzz");
        assert!((score - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_search_query_default() {
        let q = SearchQuery::default();
        assert_eq!(q.text, "");
        assert_eq!(q.sort_by, SortField::Relevance);
        assert_eq!(q.sort_order, SortOrder::Descending);
    }

    #[test]
    fn test_download_task_defaults() {
        let task = DownloadTask {
            game_id: "g1".into(),
            game_name: "Game".into(),
            status: DownloadStatus::Queued,
            progress: 0.0,
            total_bytes: 0,
            downloaded_bytes: 0,
            error: None,
            retry_count: 0,
        };
        assert_eq!(task.status, DownloadStatus::Queued);
        assert_eq!(task.retry_count, 0);
    }
}
