use std::path::PathBuf;

/// Typed representation of an installed game.
#[derive(Debug, Clone)]
pub struct InstalledGame {
    pub name: String,
    pub path: PathBuf,
    pub entry_point: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub installed_at: u64,
    pub last_played: u64,
    pub play_count: u64,
    pub total_play_time_secs: u64,
    pub size_bytes: u64,
    pub engine_version: String,
    pub category: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub hidden: bool,
    pub pinned: bool,
}

impl InstalledGame {
    pub fn new(name: String, path: PathBuf) -> Self {
        Self {
            name,
            path,
            entry_point: String::new(),
            version: String::new(),
            author: String::new(),
            description: String::new(),
            installed_at: 0,
            last_played: 0,
            play_count: 0,
            total_play_time_secs: 0,
            size_bytes: 0,
            engine_version: "0.2.0-alpha.1".into(),
            category: String::new(),
            genres: Vec::new(),
            tags: Vec::new(),
            hidden: false,
            pinned: false,
        }
    }

    pub fn matches_query(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.name.to_lowercase().contains(&q)
            || self.author.to_lowercase().contains(&q)
            || self.description.to_lowercase().contains(&q)
            || self.genres.iter().any(|g| g.to_lowercase().contains(&q))
            || self.tags.iter().any(|t| t.to_lowercase().contains(&q))
    }
}

/// A user-defined or auto-generated collection of games.
#[derive(Debug, Clone)]
pub struct Collection {
    pub name: String,
    pub kind: CollectionKind,
    pub game_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CollectionKind {
    Favorites,
    RecentlyPlayed,
    RecentlyInstalled,
    MostPlayed,
    Pinned,
    Hidden,
    Custom,
}

impl Collection {
    pub fn new(name: &str, kind: CollectionKind) -> Self {
        Self {
            name: name.to_string(),
            kind,
            game_names: Vec::new(),
        }
    }
}

/// A record of a play session.
#[derive(Debug, Clone)]
pub struct PlayRecord {
    pub game_name: String,
    pub timestamp: u64,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LibrarySortField {
    #[default]
    Name,
    InstallDate,
    LastPlayed,
    PlayTime,
    PlayCount,
    Size,
    Author,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LibrarySortOrder {
    #[default]
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryQuery {
    pub text: String,
    pub author: Option<String>,
    pub genre: Option<String>,
    pub has_updates: Option<bool>,
    pub is_favorite: Option<bool>,
    pub collection: Option<String>,
    pub hidden: Option<bool>,
    pub pinned: Option<bool>,
    pub sort_by: LibrarySortField,
    pub sort_order: LibrarySortOrder,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_game() -> InstalledGame {
        InstalledGame {
            name: "Pong".into(),
            path: PathBuf::from("/games/pong"),
            entry_point: "src/main.lua".into(),
            version: "1.0.0".into(),
            author: "VibeGE".into(),
            description: "Classic paddle game".into(),
            installed_at: 1000,
            last_played: 2000,
            play_count: 10,
            total_play_time_secs: 3600,
            size_bytes: 1024,
            engine_version: "0.2.0".into(),
            category: "action".into(),
            genres: vec!["arcade".into()],
            tags: vec!["multiplayer".into()],
            hidden: false,
            pinned: true,
        }
    }

    #[test]
    fn test_game_matches_query() {
        let g = sample_game();
        assert!(g.matches_query("Pong"));
        assert!(g.matches_query("paddle"));
        assert!(g.matches_query("arcade"));
        assert!(g.matches_query("multiplayer"));
        assert!(!g.matches_query("Chess"));
    }

    #[test]
    fn test_collection_new() {
        let c = Collection::new("Favorites", CollectionKind::Favorites);
        assert_eq!(c.name, "Favorites");
        assert_eq!(c.kind, CollectionKind::Favorites);
        assert!(c.game_names.is_empty());
    }

    #[test]
    fn test_play_record() {
        let r = PlayRecord {
            game_name: "Pong".into(),
            timestamp: 1000,
            duration_secs: 120,
        };
        assert_eq!(r.duration_secs, 120);
    }

    #[test]
    fn test_library_query_default() {
        let q = LibraryQuery::default();
        assert_eq!(q.sort_by, LibrarySortField::Name);
    }
}
