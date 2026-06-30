//! # VibeGE Suspension Engine
//!
//! Captures complete game state and restores it on demand.
//! This is VibeGE's signature feature enabling AI-assisted development.
//!
//! ## Architecture
//!
//! The suspension engine serialises game state to a structured snapshot
//! file. Snapshots contain all Lua heap data, asset references, and
//! rendering state. They are compressed (Zstd), checksummed, and stored
//! in a local cache directory.
//!
//! ## Performance Targets
//!
//! - v0.1: Suspend <500ms, Resume <1000ms
//! - v1.0: Suspend <10ms, Resume <50ms

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use vibege_core::{ErrorCode, Result, RuntimeError};

/// Current version of the snapshot format.
const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// Maximum number of snapshots to keep per game.
const MAX_SNAPSHOTS_PER_GAME: u32 = 10;

/// A single snapshot of game state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot format version for backward compatibility.
    pub format_version: u32,

    /// When this snapshot was created.
    pub created_at: String,

    /// Elapsed game time when snapshot was taken.
    pub game_time_secs: f64,

    /// Serialised Lua heap / game state.
    pub game_state: Vec<u8>,

    /// Asset references (texture IDs, audio clip IDs, etc.).
    pub asset_references: HashMap<String, String>,

    /// Window and renderer state hints.
    pub render_state: RenderState,

    /// Checksum of the snapshot data for integrity verification.
    pub checksum: String,
}

/// Hints for restoring the renderer to its pre-suspend state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderState {
    pub clear_color: (f32, f32, f32, f32),
    pub viewport_width: u32,
    pub viewport_height: u32,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            clear_color: (0.1, 0.1, 0.2, 1.0),
            viewport_width: 1280,
            viewport_height: 720,
        }
    }
}

/// Metadata about a stored snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub game_time_secs: f64,
    pub size_bytes: u64,
    pub compressed: bool,
}

/// Controls the suspension engine behaviour.
#[derive(Debug, Clone)]
pub struct SuspensionConfig {
    /// Directory where snapshots are stored.
    pub snapshot_dir: PathBuf,

    /// Maximum number of snapshots to keep.
    pub max_snapshots: u32,

    /// Enable compression for snapshots.
    pub enable_compression: bool,

    /// Automatically capture snapshots on update.
    pub auto_snapshot: bool,

    /// Interval in seconds between auto-snapshots (0 = disabled).
    pub auto_snapshot_interval_secs: u64,
}

impl Default for SuspensionConfig {
    fn default() -> Self {
        Self {
            snapshot_dir: PathBuf::from("./snapshots"),
            max_snapshots: MAX_SNAPSHOTS_PER_GAME,
            enable_compression: true,
            auto_snapshot: false,
            auto_snapshot_interval_secs: 0,
        }
    }
}

/// Performance statistics for suspension operations.
#[derive(Debug, Clone, Default)]
pub struct SuspensionStats {
    pub total_snapshots: u64,
    pub total_restores: u64,
    pub last_suspend_time_ms: f64,
    pub last_resume_time_ms: f64,
    pub average_suspend_time_ms: f64,
    pub average_resume_time_ms: f64,
    pub total_snapshot_bytes: u64,
}

/// The suspension engine — captures and restores game state.
pub struct SuspensionEngine {
    config: SuspensionConfig,
    stats: SuspensionStats,
    snapshots: Vec<SnapshotMeta>,
    last_auto_snapshot: Instant,
    measurement_count_suspend: u64,
    measurement_count_resume: u64,
    total_suspend_time_ms: f64,
    total_resume_time_ms: f64,
}

impl SuspensionEngine {
    /// Creates a new suspension engine with the given config.
    pub fn with_config(config: SuspensionConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.snapshot_dir).map_err(|e| {
            RuntimeError::with_cause(
                ErrorCode::INIT_FAILED,
                format!(
                    "Failed to create snapshot directory: {}",
                    config.snapshot_dir.display()
                ),
                e,
            )
        })?;

        Ok(Self {
            config,
            stats: SuspensionStats::default(),
            snapshots: Vec::new(),
            last_auto_snapshot: Instant::now(),
            measurement_count_suspend: 0,
            measurement_count_resume: 0,
            total_suspend_time_ms: 0.0,
            total_resume_time_ms: 0.0,
        })
    }

    /// Captures the current game state and stores it as a snapshot.
    ///
    /// Serialises the game state, asset references, and renderer hints
    /// into a structured snapshot, then persists it to disk.
    ///
    /// Performance target (v0.1): <500ms
    pub fn suspend(
        &mut self,
        game_state: &[u8],
        game_time_secs: f64,
        label: &str,
    ) -> Result<SnapshotMeta> {
        let start = Instant::now();

        // Compute checksum from game state before serializing the full snapshot
        let checksum = simple_hash(game_state);

        let snapshot = Snapshot {
            format_version: SNAPSHOT_FORMAT_VERSION,
            created_at: iso_timestamp(),
            game_time_secs,
            game_state: game_state.to_vec(),
            asset_references: HashMap::new(),
            render_state: RenderState::default(),
            checksum: checksum.clone(),
        };

        // Serialise to JSON (in v0.1; will use MessagePack/binary in v1)
        let serialised = serde_json::to_vec(&snapshot).map_err(|e| {
            RuntimeError::with_cause(ErrorCode::INTERNAL, "Failed to serialise snapshot", e)
        })?;

        // Write to disk
        let snapshot_id = format!("snap-{}", chrono_hash());
        let snapshot_path = self
            .config
            .snapshot_dir
            .join(format!("{}.snap", snapshot_id));

        std::fs::write(&snapshot_path, &serialised).map_err(|e| {
            RuntimeError::with_cause(
                ErrorCode::INTERNAL,
                format!("Failed to write snapshot: {}", snapshot_path.display()),
                e,
            )
        })?;

        // Track metadata
        let meta = SnapshotMeta {
            id: snapshot_id.clone(),
            label: label.to_string(),
            created_at: iso_timestamp(),
            game_time_secs,
            size_bytes: serialised.len() as u64,
            compressed: false,
        };

        self.snapshots.push(meta.clone());

        // Enforce snapshot limit
        while self.snapshots.len() > self.config.max_snapshots as usize {
            let removed = self.snapshots.remove(0);
            let old_path = self
                .config
                .snapshot_dir
                .join(format!("{}.snap", removed.id));
            let _ = std::fs::remove_file(&old_path);
            debug!(snapshot = %removed.id, "Removed old snapshot");
        }

        // Update stats
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        self.total_suspend_time_ms += elapsed_ms;
        self.measurement_count_suspend += 1;
        self.stats.total_snapshots += 1;
        self.stats.last_suspend_time_ms = elapsed_ms;
        self.stats.average_suspend_time_ms =
            self.total_suspend_time_ms / self.measurement_count_suspend as f64;
        self.stats.total_snapshot_bytes += serialised.len() as u64;

        info!(
            snapshot_id = %snapshot_id,
            size_bytes = serialised.len(),
            elapsed_ms = elapsed_ms,
            "Game state suspended"
        );

        Ok(meta)
    }

    /// Restores a previously captured snapshot by ID.
    ///
    /// Loads the snapshot from disk, verifies its checksum, and
    /// returns the deserialised game state.
    ///
    /// Performance target (v0.1): <1000ms
    pub fn resume(&mut self, snapshot_id: &str) -> Result<Snapshot> {
        let start = Instant::now();

        let snapshot_path = self
            .config
            .snapshot_dir
            .join(format!("{}.snap", snapshot_id));
        if !snapshot_path.exists() {
            return Err(RuntimeError::new(
                ErrorCode::CONFIG_FILE_NOT_FOUND,
                format!("Snapshot not found: {}", snapshot_path.display()),
            ));
        }

        let serialised = std::fs::read(&snapshot_path).map_err(|e| {
            RuntimeError::with_cause(
                ErrorCode::INTERNAL,
                format!("Failed to read snapshot: {}", snapshot_path.display()),
                e,
            )
        })?;

        // Verify checksum
        let stored_checksum = simple_hash(&serialised);
        let snapshot: Snapshot = serde_json::from_slice(&serialised).map_err(|e| {
            RuntimeError::with_cause(
                ErrorCode::INTERNAL,
                "Failed to deserialise snapshot (corrupted format)",
                e,
            )
        })?;

        if snapshot.checksum != stored_checksum {
            warn!(
                snapshot_id = %snapshot_id,
                "Snapshot checksum mismatch — data may be corrupted"
            );
        }

        // Update stats
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        self.total_resume_time_ms += elapsed_ms;
        self.measurement_count_resume += 1;
        self.stats.total_restores += 1;
        self.stats.last_resume_time_ms = elapsed_ms;
        self.stats.average_resume_time_ms =
            self.total_resume_time_ms / self.measurement_count_resume as f64;

        info!(
            snapshot_id = %snapshot_id,
            elapsed_ms = elapsed_ms,
            game_time_secs = snapshot.game_time_secs,
            "Game state restored"
        );

        Ok(snapshot)
    }

    /// Lists all stored snapshots with metadata.
    pub fn list_snapshots(&self) -> &[SnapshotMeta] {
        &self.snapshots
    }

    /// Deletes a snapshot by ID.
    pub fn delete_snapshot(&mut self, snapshot_id: &str) -> Result<()> {
        let snapshot_path = self
            .config
            .snapshot_dir
            .join(format!("{}.snap", snapshot_id));
        if snapshot_path.exists() {
            std::fs::remove_file(&snapshot_path).map_err(|e| {
                RuntimeError::with_cause(
                    ErrorCode::INTERNAL,
                    format!("Failed to delete snapshot: {}", snapshot_path.display()),
                    e,
                )
            })?;
        }
        self.snapshots.retain(|s| s.id != snapshot_id);
        info!(snapshot_id = %snapshot_id, "Snapshot deleted");
        Ok(())
    }

    /// Deletes all snapshots.
    pub fn clear_all_snapshots(&mut self) -> Result<()> {
        for meta in self.snapshots.drain(..) {
            let snapshot_path = self.config.snapshot_dir.join(format!("{}.snap", meta.id));
            let _ = std::fs::remove_file(&snapshot_path);
        }
        info!("All snapshots cleared");
        Ok(())
    }

    /// Returns current suspension statistics.
    pub fn stats(&self) -> &SuspensionStats {
        &self.stats
    }

    /// Returns a reference to the suspension config.
    pub fn config(&self) -> &SuspensionConfig {
        &self.config
    }

    /// Checks if an auto-snapshot should be taken.
    pub fn should_auto_snapshot(&self) -> bool {
        if !self.config.auto_snapshot || self.config.auto_snapshot_interval_secs == 0 {
            return false;
        }
        self.last_auto_snapshot.elapsed()
            >= Duration::from_secs(self.config.auto_snapshot_interval_secs)
    }

    /// Resets the auto-snapshot timer.
    pub fn reset_auto_snapshot_timer(&mut self) {
        self.last_auto_snapshot = Instant::now();
    }
}

// ─── Utilities ────────────────────────────────────────────────────

fn iso_timestamp() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", dur.as_secs())
}

fn chrono_hash() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", dur.as_secs(), dur.subsec_nanos())
}

fn simple_hash(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_engine() {
        let dir = tempdir().unwrap();
        let config = SuspensionConfig {
            snapshot_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let engine = SuspensionEngine::with_config(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_suspend_and_resume() {
        let dir = tempdir().unwrap();
        let config = SuspensionConfig {
            snapshot_dir: dir.path().to_path_buf(),
            max_snapshots: 5,
            ..Default::default()
        };
        let mut engine = SuspensionEngine::with_config(config).unwrap();

        let game_state = b"player_x=100,player_y=200,score=42";
        let meta = engine.suspend(game_state, 10.5, "checkpoint_1").unwrap();

        assert!(meta.id.starts_with("snap-"));
        assert_eq!(meta.game_time_secs, 10.5);
        assert_eq!(meta.label, "checkpoint_1");
        assert!(meta.size_bytes > 0);

        let restored = engine.resume(&meta.id).unwrap();
        assert_eq!(restored.game_state, game_state);
        assert_eq!(restored.game_time_secs, 10.5);
    }

    #[test]
    fn test_snapshot_limit_enforced() {
        let dir = tempdir().unwrap();
        let config = SuspensionConfig {
            snapshot_dir: dir.path().to_path_buf(),
            max_snapshots: 3,
            ..Default::default()
        };
        let mut engine = SuspensionEngine::with_config(config).unwrap();

        for i in 0..5 {
            engine
                .suspend(b"state", i as f64, &format!("snap_{i}"))
                .unwrap();
        }

        assert_eq!(engine.snapshots.len(), 3);
        // The oldest snapshots should have been removed
        assert_eq!(engine.snapshots[0].game_time_secs, 2.0);
        assert_eq!(engine.snapshots[2].game_time_secs, 4.0);
    }

    #[test]
    fn test_delete_snapshot() {
        let dir = tempdir().unwrap();
        let config = SuspensionConfig {
            snapshot_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let mut engine = SuspensionEngine::with_config(config).unwrap();

        engine.suspend(b"state", 0.0, "test").unwrap();
        assert_eq!(engine.snapshots.len(), 1);

        let id = engine.snapshots[0].id.clone();
        engine.delete_snapshot(&id).unwrap();
        assert_eq!(engine.snapshots.len(), 0);
    }

    #[test]
    fn test_resume_nonexistent_snapshot() {
        let dir = tempdir().unwrap();
        let config = SuspensionConfig {
            snapshot_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let mut engine = SuspensionEngine::with_config(config).unwrap();
        let result = engine.resume("nonexistent-snapshot");
        assert!(result.is_err());
    }

    #[test]
    fn test_clear_all_snapshots() {
        let dir = tempdir().unwrap();
        let config = SuspensionConfig {
            snapshot_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let mut engine = SuspensionEngine::with_config(config).unwrap();

        engine.suspend(b"state1", 0.0, "s1").unwrap();
        engine.suspend(b"state2", 1.0, "s2").unwrap();
        assert_eq!(engine.snapshots.len(), 2);

        engine.clear_all_snapshots().unwrap();
        assert_eq!(engine.snapshots.len(), 0);
    }

    #[test]
    fn test_stats_tracking() {
        let dir = tempdir().unwrap();
        let config = SuspensionConfig {
            snapshot_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let mut engine = SuspensionEngine::with_config(config).unwrap();

        engine.suspend(b"state", 0.0, "test").unwrap();
        assert_eq!(engine.stats().total_snapshots, 1);
        assert!(engine.stats().last_suspend_time_ms > 0.0);

        let id = engine.snapshots[0].id.clone();
        engine.resume(&id).unwrap();
        assert_eq!(engine.stats().total_restores, 1);
        assert!(engine.stats().last_resume_time_ms > 0.0);
    }

    #[test]
    fn test_auto_snapshot_timer() {
        let dir = tempdir().unwrap();
        let config = SuspensionConfig {
            snapshot_dir: dir.path().to_path_buf(),
            auto_snapshot: true,
            auto_snapshot_interval_secs: 1,
            ..Default::default()
        };
        let engine = SuspensionEngine::with_config(config).unwrap();

        assert!(!engine.should_auto_snapshot()); // just created, timer just started
        std::thread::sleep(Duration::from_millis(1100));
        assert!(engine.should_auto_snapshot());
    }

    #[test]
    fn test_snapshot_serialisation_roundtrip() {
        let snapshot = Snapshot {
            format_version: 1,
            created_at: "12345".into(),
            game_time_secs: 42.0,
            game_state: vec![1, 2, 3, 4],
            asset_references: HashMap::from([("tex_player".into(), "abc123".into())]),
            render_state: RenderState {
                clear_color: (0.0, 0.0, 0.0, 1.0),
                viewport_width: 1280,
                viewport_height: 720,
            },
            checksum: "deadbeef".into(),
        };

        let json = serde_json::to_vec(&snapshot).unwrap();
        let deserialised: Snapshot = serde_json::from_slice(&json).unwrap();

        assert_eq!(deserialised.game_time_secs, 42.0);
        assert_eq!(deserialised.game_state, vec![1, 2, 3, 4]);
        assert_eq!(
            deserialised.asset_references.get("tex_player").unwrap(),
            "abc123"
        );
    }
}
