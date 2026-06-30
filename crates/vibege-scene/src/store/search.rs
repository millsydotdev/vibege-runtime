use super::models::{GameListing, SearchQuery, SortField, SortOrder};

/// In-memory search engine with fuzzy matching and filtering.
pub struct SearchEngine;

impl SearchEngine {
    /// Search listings matching the given query.
    pub fn search<'a>(listings: &'a [GameListing], query: &SearchQuery) -> Vec<&'a GameListing> {
        let mut results: Vec<(&GameListing, f64)> = listings
            .iter()
            .filter(|l| Self::matches_filter(l, query))
            .map(|l| {
                let score = if query.text.is_empty() {
                    1.0
                } else {
                    l.fuzzy_score(&query.text)
                };
                (l, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort by score or selected field
        Self::sort_results(&mut results, query);

        results.into_iter().map(|(l, _)| l).collect()
    }

    fn matches_filter(listing: &GameListing, query: &SearchQuery) -> bool {
        if let Some(ref category) = query.category {
            if !listing.category.eq_ignore_ascii_case(category) {
                return false;
            }
        }
        if let Some(ref genre) = query.genre {
            if !listing.genres.iter().any(|g| g.eq_ignore_ascii_case(genre)) {
                return false;
            }
        }
        if let Some(ref tag) = query.tag {
            if !listing.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                return false;
            }
        }
        if let Some(ref author) = query.author {
            if !listing.author.eq_ignore_ascii_case(author) {
                return false;
            }
        }
        if let Some(min_rating) = query.min_rating {
            if listing.rating < min_rating {
                return false;
            }
        }
        true
    }

    fn sort_results(results: &mut Vec<(&GameListing, f64)>, query: &SearchQuery) {
        match query.sort_by {
            SortField::Relevance => {
                results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
            SortField::Name => {
                results.sort_by(|a, b| a.0.name.cmp(&b.0.name));
            }
            SortField::Downloads => {
                results.sort_by(|a, b| b.0.downloads.cmp(&a.0.downloads));
            }
            SortField::Rating => {
                results.sort_by(|a, b| {
                    b.0.rating
                        .partial_cmp(&a.0.rating)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortField::Updated => {
                results.sort_by(|a, b| b.0.updated_at.cmp(&a.0.updated_at));
            }
            SortField::Created => {
                results.sort_by(|a, b| b.0.created_at.cmp(&a.0.created_at));
            }
        }

        if query.sort_order == SortOrder::Ascending {
            results.reverse();
        }
    }

    /// Extract all unique categories from listings.
    pub fn extract_categories(listings: &[GameListing]) -> Vec<String> {
        let mut cats: Vec<String> = listings.iter().map(|l| l.category.clone()).collect();
        cats.sort();
        cats.dedup();
        cats
    }

    /// Extract all unique genres from listings.
    pub fn extract_genres(listings: &[GameListing]) -> Vec<String> {
        let mut genres: Vec<String> = Vec::new();
        for listing in listings {
            for genre in &listing.genres {
                if !genres.contains(genre) {
                    genres.push(genre.clone());
                }
            }
        }
        genres.sort();
        genres
    }

    /// Extract all unique tags from listings.
    pub fn extract_tags(listings: &[GameListing]) -> Vec<String> {
        let mut tags: Vec<String> = Vec::new();
        for listing in listings {
            for tag in &listing.tags {
                if !tags.contains(tag) {
                    tags.push(tag.clone());
                }
            }
        }
        tags.sort();
        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn listings() -> &'static Vec<GameListing> {
        static LISTINGS: OnceLock<Vec<GameListing>> = OnceLock::new();
        LISTINGS.get_or_init(|| {
            vec![
                GameListing {
                    id: "1".into(),
                    name: "Pong".into(),
                    description: "Classic paddle game".into(),
                    author: "VibeGE".into(),
                    publisher: "".into(),
                    version: "1.0".into(),
                    category: "action".into(),
                    genres: vec!["arcade".into(), "sports".into()],
                    tags: vec!["multiplayer".into()],
                    status: "approved".into(),
                    downloads: 100,
                    file_size: 0,
                    icon_url: None,
                    hero_url: None,
                    screenshots: vec![],
                    created_at: "2026-01-01".into(),
                    updated_at: "2026-06-01".into(),
                    engine_version: None,
                    rating: 4.5,
                },
                GameListing {
                    id: "2".into(),
                    name: "Chess".into(),
                    description: "Strategy board game".into(),
                    author: "VibeGE".into(),
                    publisher: "".into(),
                    version: "2.0".into(),
                    category: "strategy".into(),
                    genres: vec!["board".into(), "strategy".into()],
                    tags: vec!["multiplayer".into(), "turn-based".into()],
                    status: "approved".into(),
                    downloads: 50,
                    file_size: 0,
                    icon_url: None,
                    hero_url: None,
                    screenshots: vec![],
                    created_at: "2026-02-01".into(),
                    updated_at: "2026-05-01".into(),
                    engine_version: None,
                    rating: 4.8,
                },
                GameListing {
                    id: "3".into(),
                    name: "Void Drifter".into(),
                    description: "Space exploration game".into(),
                    author: "VibeGE Labs".into(),
                    publisher: "".into(),
                    version: "0.5".into(),
                    category: "adventure".into(),
                    genres: vec!["exploration".into(), "sci-fi".into()],
                    tags: vec!["singleplayer".into()],
                    status: "approved".into(),
                    downloads: 200,
                    file_size: 0,
                    icon_url: None,
                    hero_url: None,
                    screenshots: vec![],
                    created_at: "2026-03-01".into(),
                    updated_at: "2026-06-15".into(),
                    engine_version: None,
                    rating: 4.2,
                },
            ]
        })
    }

    #[test]
    fn test_search_empty_query() {
        let list = listings();
        let results = SearchEngine::search(list, &SearchQuery::default());
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_by_name() {
        let q = SearchQuery {
            text: "Pong".into(),
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Pong");
    }

    #[test]
    fn test_search_by_genre() {
        let q = SearchQuery {
            text: "arcade".into(),
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Pong");
    }

    #[test]
    fn test_search_by_tag() {
        let q = SearchQuery {
            text: "turn-based".into(),
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Chess");
    }

    #[test]
    fn test_search_empty_results() {
        let q = SearchQuery {
            text: "zzzzz".into(),
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert!(results.is_empty());
    }

    #[test]
    fn test_filter_by_category() {
        let q = SearchQuery {
            category: Some("action".into()),
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_filter_by_genre() {
        let q = SearchQuery {
            genre: Some("board".into()),
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Chess");
    }

    #[test]
    fn test_filter_by_author() {
        let q = SearchQuery {
            author: Some("VibeGE Labs".into()),
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_sort_by_downloads() {
        let q = SearchQuery {
            sort_by: SortField::Downloads,
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert_eq!(results[0].name, "Void Drifter");
        assert_eq!(results[2].name, "Chess");
    }

    #[test]
    fn test_sort_by_rating() {
        let q = SearchQuery {
            sort_by: SortField::Rating,
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert_eq!(results[0].name, "Chess");
        assert_eq!(results[2].name, "Void Drifter");
    }

    #[test]
    fn test_extract_categories() {
        let list = listings();
        let cats = SearchEngine::extract_categories(list);
        assert_eq!(cats, vec!["action", "adventure", "strategy"]);
    }

    #[test]
    fn test_extract_genres() {
        let list = listings();
        let genres = SearchEngine::extract_genres(list);
        assert!(genres.contains(&"arcade".to_string()));
        assert!(genres.contains(&"strategy".to_string()));
    }

    #[test]
    fn test_min_rating_filter() {
        let q = SearchQuery {
            min_rating: Some(4.5),
            ..Default::default()
        };
        let list = listings();
        let results = SearchEngine::search(list, &q);
        assert_eq!(results.len(), 2); // Pong (4.5) and Chess (4.8)
        assert!(!results.iter().any(|l| l.name == "Void Drifter"));
    }
}
