use super::models::GameListing;

/// Parses and enriches game metadata from raw API responses.
pub struct MetadataProvider;

impl MetadataProvider {
    /// Parse a JSON response body into `GameListing` objects.
    ///
    /// Supports both single-object `{...}` and array `[...]` and
    /// paginated `{"packages": [...]}` responses.
    pub fn parse_listings(json: &serde_json::Value) -> Vec<GameListing> {
        let mut listings = Vec::new();

        // Try array first
        if let Some(arr) = json.as_array() {
            for item in arr {
                if let Some(listing) = GameListing::from_json(item) {
                    listings.push(listing);
                }
            }
            return listings;
        }

        // Try packages key (paginated API response)
        if let Some(packages) = json["packages"].as_array() {
            for item in packages {
                if let Some(listing) = GameListing::from_json(item) {
                    listings.push(listing);
                }
            }
            return listings;
        }

        // Try single object
        if let Some(listing) = GameListing::from_json(json) {
            listings.push(listing);
        }

        listings
    }

    /// Extract the total count from a paginated response.
    pub fn parse_total_count(json: &serde_json::Value) -> u32 {
        json["total"].as_u64().unwrap_or(0) as u32
    }

    /// Check if a game has an update available.
    pub fn has_update(listing: &GameListing, installed_version: &str) -> bool {
        listing.version != installed_version
    }

    /// Format file size to human-readable string.
    pub fn format_file_size(bytes: u64) -> String {
        if bytes < 1024 {
            format!("{bytes} B")
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_listings_array() {
        let json = serde_json::json!([
            {"id": "g1", "name": "Game 1"},
            {"id": "g2", "name": "Game 2"},
        ]);
        let listings = MetadataProvider::parse_listings(&json);
        assert_eq!(listings.len(), 2);
        assert_eq!(listings[0].name, "Game 1");
    }

    #[test]
    fn test_parse_listings_paginated() {
        let json = serde_json::json!({
            "packages": [
                {"id": "g1", "name": "Game 1"},
                {"id": "g2", "name": "Game 2"},
            ],
            "total": 2,
        });
        let listings = MetadataProvider::parse_listings(&json);
        assert_eq!(listings.len(), 2);
    }

    #[test]
    fn test_parse_listings_single() {
        let json = serde_json::json!({"id": "g1", "name": "Single Game"});
        let listings = MetadataProvider::parse_listings(&json);
        assert_eq!(listings.len(), 1);
    }

    #[test]
    fn test_parse_listings_empty() {
        let json = serde_json::json!({"packages": []});
        let listings = MetadataProvider::parse_listings(&json);
        assert!(listings.is_empty());
    }

    #[test]
    fn test_parse_total_count() {
        let json = serde_json::json!({"total": 42});
        assert_eq!(MetadataProvider::parse_total_count(&json), 42);
    }

    #[test]
    fn test_parse_total_count_missing() {
        let json = serde_json::json!({});
        assert_eq!(MetadataProvider::parse_total_count(&json), 0);
    }

    #[test]
    fn test_has_update() {
        let listing = GameListing {
            version: "2.0.0".into(),
            id: "g1".into(),
            name: "Test".into(),
            description: "".into(),
            author: "".into(),
            publisher: "".into(),
            category: "".into(),
            genres: vec![],
            tags: vec![],
            status: "".into(),
            downloads: 0,
            file_size: 0,
            icon_url: None,
            hero_url: None,
            screenshots: vec![],
            created_at: "".into(),
            updated_at: "".into(),
            engine_version: None,
            rating: 0.0,
        };
        assert!(MetadataProvider::has_update(&listing, "1.0.0"));
        assert!(!MetadataProvider::has_update(&listing, "2.0.0"));
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(MetadataProvider::format_file_size(500), "500 B");
        assert_eq!(MetadataProvider::format_file_size(2048), "2.0 KB");
        assert_eq!(
            MetadataProvider::format_file_size(2 * 1024 * 1024),
            "2.0 MB"
        );
        assert_eq!(
            MetadataProvider::format_file_size(2 * 1024 * 1024 * 1024),
            "2.0 GB"
        );
    }
}
