use super::models::{InstalledGame, LibraryQuery, LibrarySortField, LibrarySortOrder};

/// Search and sort engine for the library's installed game list.
pub struct LibrarySearchEngine;

impl LibrarySearchEngine {
    /// Search installed games matching the given query.
    pub fn search<'a>(games: &'a [InstalledGame], query: &LibraryQuery) -> Vec<&'a InstalledGame> {
        let mut results: Vec<&InstalledGame> = games
            .iter()
            .filter(|g| Self::matches_filter(g, query))
            .collect();

        Self::sort_results(&mut results, query);
        results
    }

    fn matches_filter(game: &InstalledGame, query: &LibraryQuery) -> bool {
        if !query.text.is_empty() && !game.matches_query(&query.text) {
            return false;
        }
        if let Some(ref author) = query.author {
            if !game.author.eq_ignore_ascii_case(author) {
                return false;
            }
        }
        if let Some(ref genre) = query.genre {
            if !game.genres.iter().any(|g| g.eq_ignore_ascii_case(genre)) {
                return false;
            }
        }
        if let Some(hidden) = query.hidden {
            if game.hidden != hidden {
                return false;
            }
        }
        if let Some(pinned) = query.pinned {
            if game.pinned != pinned {
                return false;
            }
        }
        true
    }

    fn sort_results(results: &mut Vec<&InstalledGame>, query: &LibraryQuery) {
        match query.sort_by {
            LibrarySortField::Name => {
                results.sort_by(|a, b| a.name.cmp(&b.name));
            }
            LibrarySortField::InstallDate => {
                results.sort_by(|a, b| a.installed_at.cmp(&b.installed_at));
            }
            LibrarySortField::LastPlayed => {
                results.sort_by(|a, b| b.last_played.cmp(&a.last_played));
            }
            LibrarySortField::PlayTime => {
                results.sort_by(|a, b| b.total_play_time_secs.cmp(&a.total_play_time_secs));
            }
            LibrarySortField::PlayCount => {
                results.sort_by(|a, b| b.play_count.cmp(&a.play_count));
            }
            LibrarySortField::Size => {
                results.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
            }
            LibrarySortField::Author => {
                results.sort_by(|a, b| a.author.cmp(&b.author));
            }
        }

        if query.sort_order == LibrarySortOrder::Descending {
            results.reverse();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    fn games() -> &'static Vec<InstalledGame> {
        static GAMES: OnceLock<Vec<InstalledGame>> = OnceLock::new();
        GAMES.get_or_init(|| {
            vec![
                InstalledGame {
                    name: "Pong".into(),
                    author: "VibeGE".into(),
                    genres: vec!["arcade".into()],
                    installed_at: 100,
                    last_played: 300,
                    play_count: 10,
                    total_play_time_secs: 3600,
                    size_bytes: 1024,
                    ..sample_base("1.0")
                },
                InstalledGame {
                    name: "Chess".into(),
                    author: "VibeGE".into(),
                    genres: vec!["board".into(), "strategy".into()],
                    installed_at: 200,
                    last_played: 200,
                    play_count: 5,
                    total_play_time_secs: 1800,
                    size_bytes: 512,
                    ..sample_base("2.0")
                },
                InstalledGame {
                    name: "Void Drifter".into(),
                    author: "VibeGE Labs".into(),
                    genres: vec!["exploration".into()],
                    installed_at: 300,
                    last_played: 100,
                    play_count: 20,
                    total_play_time_secs: 7200,
                    size_bytes: 2048,
                    hidden: true,
                    ..sample_base("0.5")
                },
            ]
        })
    }

    fn sample_base(version: &str) -> InstalledGame {
        InstalledGame {
            name: String::new(),
            path: PathBuf::new(),
            entry_point: "main.lua".into(),
            version: version.into(),
            author: String::new(),
            description: String::new(),
            installed_at: 0,
            last_played: 0,
            play_count: 0,
            total_play_time_secs: 0,
            size_bytes: 0,
            engine_version: "0.2.0".into(),
            category: String::new(),
            genres: vec![],
            tags: vec![],
            hidden: false,
            pinned: false,
        }
    }

    #[test]
    fn test_search_empty_query() {
        let list = games();
        let results = LibrarySearchEngine::search(list, &LibraryQuery::default());
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_by_name() {
        let q = LibraryQuery {
            text: "Pong".into(),
            ..Default::default()
        };
        let list = games();
        let results = LibrarySearchEngine::search(list, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Pong");
    }

    #[test]
    fn test_filter_hidden() {
        let q = LibraryQuery {
            hidden: Some(false),
            ..Default::default()
        };
        let list = games();
        let results = LibrarySearchEngine::search(list, &q);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_filter_by_author() {
        let q = LibraryQuery {
            author: Some("VibeGE Labs".into()),
            ..Default::default()
        };
        let list = games();
        let results = LibrarySearchEngine::search(list, &q);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_filter_by_genre() {
        let q = LibraryQuery {
            genre: Some("arcade".into()),
            ..Default::default()
        };
        let list = games();
        let results = LibrarySearchEngine::search(list, &q);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_sort_by_play_count() {
        let q = LibraryQuery {
            sort_by: LibrarySortField::PlayCount,
            ..Default::default()
        };
        let list = games();
        let results = LibrarySearchEngine::search(list, &q);
        assert_eq!(results[0].name, "Void Drifter"); // 20 plays
        assert_eq!(results[2].name, "Chess"); // 5 plays
    }

    #[test]
    fn test_sort_by_last_played() {
        let q = LibraryQuery {
            sort_by: LibrarySortField::LastPlayed,
            ..Default::default()
        };
        let list = games();
        let results = LibrarySearchEngine::search(list, &q);
        assert_eq!(results[0].name, "Pong"); // last_played = 300
        assert_eq!(results[2].name, "Void Drifter"); // last_played = 100
    }
}
