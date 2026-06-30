use std::collections::HashMap;

/// Per-asset-type statistics.
#[derive(Debug, Clone, Default)]
pub struct TypeStats {
    pub count: usize,
    pub memory_bytes: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub loads: u64,
    pub releases: u64,
    pub failed_loads: u64,
}

/// Aggregate asset system statistics.
#[derive(Debug, Clone, Default)]
pub struct AssetStatistics {
    pub total_assets: usize,
    pub total_memory_bytes: u64,
    pub total_cache_hits: u64,
    pub total_cache_misses: u64,
    pub total_loads: u64,
    pub total_releases: u64,
    pub total_failed_loads: u64,
    pub asset_type_breakdown: HashMap<&'static str, TypeStats>,
}

impl AssetStatistics {
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_cache_hits + self.total_cache_misses;
        if total == 0 {
            0.0
        } else {
            self.total_cache_hits as f64 / total as f64
        }
    }

    pub fn merge(&mut self, other: &AssetStatistics) {
        self.total_assets += other.total_assets;
        self.total_memory_bytes += other.total_memory_bytes;
        self.total_cache_hits += other.total_cache_hits;
        self.total_cache_misses += other.total_cache_misses;
        self.total_loads += other.total_loads;
        self.total_releases += other.total_releases;
        self.total_failed_loads += other.total_failed_loads;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statistics_hit_rate() {
        let stats = AssetStatistics {
            total_cache_hits: 80,
            total_cache_misses: 20,
            ..Default::default()
        };
        assert!((stats.hit_rate() - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_statistics_hit_rate_zero() {
        let stats = AssetStatistics::default();
        assert!((stats.hit_rate() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_statistics_merge() {
        let mut a = AssetStatistics {
            total_assets: 10,
            total_memory_bytes: 1000,
            total_cache_hits: 50,
            total_cache_misses: 5,
            total_loads: 10,
            total_releases: 2,
            total_failed_loads: 0,
            ..Default::default()
        };
        let b = AssetStatistics {
            total_assets: 5,
            total_memory_bytes: 500,
            total_cache_hits: 30,
            total_cache_misses: 2,
            total_loads: 5,
            total_releases: 1,
            total_failed_loads: 1,
            ..Default::default()
        };
        a.merge(&b);
        assert_eq!(a.total_assets, 15);
        assert_eq!(a.total_memory_bytes, 1500);
        assert_eq!(a.total_cache_hits, 80);
        assert_eq!(a.total_cache_misses, 7);
        assert_eq!(a.total_loads, 15);
        assert_eq!(a.total_releases, 3);
        assert_eq!(a.total_failed_loads, 1);
    }
}
