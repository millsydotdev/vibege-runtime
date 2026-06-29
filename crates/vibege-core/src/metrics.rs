use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// A snapshot of all metrics at a point in time.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub uptime_secs: f64,
    pub frame_count: u64,
    pub fps: f64,
    pub last_frame_ms: f64,
    pub custom_counters: HashMap<String, u64>,
    pub custom_gauges: HashMap<String, f64>,
    pub peak_memory_kb: u64,
    pub started_at: String,
}

/// A simple metrics registry for runtime instrumentation.
///
/// Metrics are thread-safe and can be written from any subsystem.
/// Snapshots are taken atomically for diagnostic display.
#[derive(Debug)]
pub struct MetricsRegistry {
    started_at: Instant,
    frame_count: AtomicU64,
    last_frame_time_ns: AtomicU64,
    counters: std::sync::RwLock<HashMap<String, u64>>,
    gauges: std::sync::RwLock<HashMap<String, f64>>,
    peak_memory_kb: AtomicU64,
    running: AtomicBool,
}

impl MetricsRegistry {
    /// Creates a new metrics registry.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started_at: Instant::now(),
            frame_count: AtomicU64::new(0),
            last_frame_time_ns: AtomicU64::new(0),
            counters: std::sync::RwLock::new(HashMap::new()),
            gauges: std::sync::RwLock::new(HashMap::new()),
            peak_memory_kb: AtomicU64::new(0),
            running: AtomicBool::new(true),
        })
    }

    /// Records a frame with its duration in seconds.
    /// Thread-safe. FPS is calculated from last frame time to avoid race conditions.
    pub fn record_frame(&self, delta_seconds: f64) {
        self.frame_count.fetch_add(1, Ordering::Relaxed);
        let ns = (delta_seconds * 1_000_000_000.0) as u64;
        self.last_frame_time_ns.store(ns, Ordering::Relaxed);
    }

    /// Increments a named counter by 1.
    pub fn increment_counter(&self, name: &str) {
        let mut counters = self.counters.write().unwrap();
        *counters.entry(name.to_string()).or_insert(0) += 1;
    }

    /// Sets a named gauge to a value.
    pub fn set_gauge(&self, name: &str, value: f64) {
        let mut gauges = self.gauges.write().unwrap();
        gauges.insert(name.to_string(), value);
    }

    /// Updates the peak memory tracking.
    pub fn record_memory(&self, current_kb: u64) {
        let mut peak = self.peak_memory_kb.load(Ordering::Relaxed);
        while current_kb > peak {
            match self.peak_memory_kb.compare_exchange_weak(peak, current_kb, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(p) => peak = p,
            }
        }
    }

    /// Takes an atomic snapshot of all metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let uptime = self.started_at.elapsed();
        let counters = self.counters.read().unwrap().clone();
        let gauges = self.gauges.read().unwrap().clone();
        let frame_count = self.frame_count.load(Ordering::Relaxed);
        let last_frame_ms = self.last_frame_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;

        // Compute FPS from last frame time (avoids race conditions with accumulators)
        let fps = if last_frame_ms > 0.0 { 1000.0 / last_frame_ms } else { 0.0 };

        MetricsSnapshot {
            uptime_secs: uptime.as_secs_f64(),
            frame_count,
            fps,
            last_frame_ms,
            custom_counters: counters,
            custom_gauges: gauges,
            peak_memory_kb: self.peak_memory_kb.load(Ordering::Relaxed),
            started_at: format!("{}.{:09}s", uptime.as_secs(), uptime.subsec_nanos()),
        }
    }

    /// Stops the FPS ticker. Called on shutdown.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new();
        let snapshot = registry.snapshot();
        assert!(snapshot.uptime_secs >= 0.0);
        assert_eq!(snapshot.frame_count, 0);
    }

    #[test]
    fn test_record_frame() {
        let registry = MetricsRegistry::new();
        registry.record_frame(0.016);
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.frame_count, 1);
        assert!(snapshot.last_frame_ms > 0.0);
    }

    #[test]
    fn test_counter() {
        let registry = MetricsRegistry::new();
        registry.increment_counter("test_events");
        registry.increment_counter("test_events");
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.custom_counters.get("test_events"), Some(&2));
    }

    #[test]
    fn test_gauge() {
        let registry = MetricsRegistry::new();
        registry.set_gauge("temperature", 36.5);
        let snapshot = registry.snapshot();
        assert!((snapshot.custom_gauges.get("temperature").unwrap() - 36.5).abs() < 0.001);
    }

    #[test]
    fn test_peak_memory() {
        let registry = MetricsRegistry::new();
        registry.record_memory(1024);
        registry.record_memory(2048);
        registry.record_memory(1500);
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.peak_memory_kb, 2048);
    }

    #[test]
    fn test_multiple_frames() {
        let registry = MetricsRegistry::new();
        for _ in 0..60 {
            registry.record_frame(0.016);
        }
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.frame_count, 60);
    }
}
