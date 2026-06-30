use super::models::{GameListing, SectionType, StoreSection};

/// Generates discovery sections from available game listings.
pub struct DiscoveryEngine;

impl DiscoveryEngine {
    /// Build all discovery sections from the given listings.
    pub fn build_sections(listings: &[GameListing], installed_ids: &[String]) -> Vec<StoreSection> {
        let mut sections = Vec::new();

        if let Some(featured) = Self::featured(listings) {
            sections.push(featured);
        }
        if let Some(trending) = Self::trending(listings) {
            sections.push(trending);
        }
        if let Some(new_releases) = Self::new_releases(listings) {
            sections.push(new_releases);
        }
        if let Some(updated) = Self::recently_updated(listings) {
            sections.push(updated);
        }
        if let Some(top_rated) = Self::top_rated(listings) {
            sections.push(top_rated);
        }
        if let Some(most_dl) = Self::most_downloaded(listings) {
            sections.push(most_dl);
        }
        if let Some(rec) = Self::recommended(listings, installed_ids) {
            sections.push(rec);
        }

        sections
    }

    /// Featured games (first N, highest rated).
    pub fn featured(listings: &[GameListing]) -> Option<StoreSection> {
        let mut sorted: Vec<_> = listings.iter().collect();
        sorted.sort_by(|a, b| {
            b.rating
                .partial_cmp(&a.rating)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let games: Vec<GameListing> = sorted.into_iter().take(5).cloned().collect();
        if games.is_empty() {
            return None;
        }
        Some(StoreSection {
            title: "Featured".into(),
            games,
            section_type: SectionType::Featured,
        })
    }

    /// Trending games (most downloaded recently).
    pub fn trending(listings: &[GameListing]) -> Option<StoreSection> {
        let mut sorted: Vec<_> = listings.iter().collect();
        sorted.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        let games: Vec<GameListing> = sorted.into_iter().take(5).cloned().collect();
        if games.is_empty() {
            return None;
        }
        Some(StoreSection {
            title: "Trending".into(),
            games,
            section_type: SectionType::Trending,
        })
    }

    /// New releases (by creation date).
    pub fn new_releases(listings: &[GameListing]) -> Option<StoreSection> {
        let mut sorted: Vec<_> = listings.iter().collect();
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let games: Vec<GameListing> = sorted.into_iter().take(5).cloned().collect();
        if games.is_empty() {
            return None;
        }
        Some(StoreSection {
            title: "New Releases".into(),
            games,
            section_type: SectionType::NewReleases,
        })
    }

    /// Recently updated games.
    pub fn recently_updated(listings: &[GameListing]) -> Option<StoreSection> {
        let mut sorted: Vec<_> = listings.iter().collect();
        sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let games: Vec<GameListing> = sorted.into_iter().take(5).cloned().collect();
        if games.is_empty() {
            return None;
        }
        Some(StoreSection {
            title: "Recently Updated".into(),
            games,
            section_type: SectionType::RecentlyUpdated,
        })
    }

    /// Top rated games.
    pub fn top_rated(listings: &[GameListing]) -> Option<StoreSection> {
        let mut sorted: Vec<_> = listings.iter().collect();
        sorted.sort_by(|a, b| {
            b.rating
                .partial_cmp(&a.rating)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let games: Vec<GameListing> = sorted.into_iter().take(5).cloned().collect();
        if games.is_empty() {
            return None;
        }
        Some(StoreSection {
            title: "Top Rated".into(),
            games,
            section_type: SectionType::TopRated,
        })
    }

    /// Most downloaded games.
    pub fn most_downloaded(listings: &[GameListing]) -> Option<StoreSection> {
        let mut sorted: Vec<_> = listings.iter().collect();
        sorted.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        let games: Vec<GameListing> = sorted.into_iter().take(5).cloned().collect();
        if games.is_empty() {
            return None;
        }
        Some(StoreSection {
            title: "Most Downloaded".into(),
            games,
            section_type: SectionType::MostDownloaded,
        })
    }

    /// Recommended games based on what's installed.
    pub fn recommended(listings: &[GameListing], installed_ids: &[String]) -> Option<StoreSection> {
        if listings.is_empty() {
            return None;
        }
        // Find genres of installed games
        let installed_genres: Vec<&str> = listings
            .iter()
            .filter(|l| installed_ids.contains(&l.id))
            .flat_map(|l| l.genres.iter().map(|g| g.as_str()))
            .collect();

        // Score uninstalled games by genre overlap with installed
        let mut scored: Vec<(&GameListing, usize)> = listings
            .iter()
            .filter(|l| !installed_ids.contains(&l.id))
            .map(|l| {
                let score = l
                    .genres
                    .iter()
                    .filter(|g| installed_genres.contains(&g.as_str()))
                    .count();
                (l, score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        let games: Vec<GameListing> = scored.into_iter().take(5).map(|(l, _)| l.clone()).collect();

        if games.is_empty() {
            return None;
        }
        Some(StoreSection {
            title: "Recommended".into(),
            games,
            section_type: SectionType::Recommended,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listings() -> Vec<GameListing> {
        vec![
            GameListing {
                id: "1".into(),
                name: "Pong".into(),
                description: "".into(),
                author: "".into(),
                publisher: "".into(),
                version: "1.0".into(),
                category: "action".into(),
                genres: vec!["arcade".into()],
                tags: vec![],
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
                description: "".into(),
                author: "".into(),
                publisher: "".into(),
                version: "2.0".into(),
                category: "strategy".into(),
                genres: vec!["board".into(), "strategy".into()],
                tags: vec![],
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
                description: "".into(),
                author: "".into(),
                publisher: "".into(),
                version: "0.5".into(),
                category: "adventure".into(),
                genres: vec!["exploration".into(), "sci-fi".into()],
                tags: vec![],
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
    }

    #[test]
    fn test_featured_section() {
        let section = DiscoveryEngine::featured(&listings()).unwrap();
        assert_eq!(section.section_type, SectionType::Featured);
        assert_eq!(section.games.len(), 3); // All 3 games, limited to 5
        // Highest rated first
        assert_eq!(section.games[0].name, "Chess");
    }

    #[test]
    fn test_trending_section() {
        let section = DiscoveryEngine::trending(&listings()).unwrap();
        assert_eq!(section.section_type, SectionType::Trending);
        assert_eq!(section.games[0].name, "Void Drifter"); // 200 downloads
    }

    #[test]
    fn test_new_releases() {
        let section = DiscoveryEngine::new_releases(&listings()).unwrap();
        assert_eq!(section.games[0].name, "Void Drifter"); // newest
    }

    #[test]
    fn test_recently_updated() {
        let section = DiscoveryEngine::recently_updated(&listings()).unwrap();
        assert_eq!(section.games[0].name, "Void Drifter"); // latest update
    }

    #[test]
    fn test_top_rated() {
        let section = DiscoveryEngine::top_rated(&listings()).unwrap();
        assert_eq!(section.games[0].name, "Chess"); // 4.8 rating
        assert_eq!(section.games[2].name, "Void Drifter"); // 4.2 rating
    }

    #[test]
    fn test_recommended() {
        let installed = vec!["1".into()]; // Pong installed (arcade genre)
        let section = DiscoveryEngine::recommended(&listings(), &installed).unwrap();
        // Should recommend games not installed
        assert!(section.games.iter().all(|g| g.id != "1"));
        assert_eq!(section.section_type, SectionType::Recommended);
    }

    #[test]
    fn test_build_sections() {
        let sections = DiscoveryEngine::build_sections(&listings(), &[]);
        assert!(!sections.is_empty());
        // Should have featured, trending, new releases, updated, top rated, most downloaded
        assert!(
            sections
                .iter()
                .any(|s| s.section_type == SectionType::Featured)
        );
        assert!(
            sections
                .iter()
                .any(|s| s.section_type == SectionType::Trending)
        );
        assert!(
            sections
                .iter()
                .any(|s| s.section_type == SectionType::NewReleases)
        );
    }

    #[test]
    fn test_empty_listings_no_sections() {
        let sections = DiscoveryEngine::build_sections(&[], &[]);
        assert!(sections.is_empty());
    }

    #[test]
    fn test_recommended_empty_when_nothing_installed() {
        // Without installed games, recommendations fall back
        let section = DiscoveryEngine::recommended(&listings(), &[]);
        // Should still work, recommending all games
        assert!(section.is_some());
    }
}
