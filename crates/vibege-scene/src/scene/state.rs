//! Scene state persistence.
//!
//! Scenes can optionally save and restore their internal state as JSON strings.
//! The SceneManager calls `save_state()` on suspend and `restore_state()` on resume,
//! allowing scenes to survive interruption (e.g., overlay toggle, window focus loss).

/// Persisted snapshot of a scene's state at a point in time.
#[derive(Debug, Clone)]
pub struct SceneSnapshot {
    /// The scene's type identifier.
    pub scene_id: super::SceneId,

    /// Serialized state payload (typically JSON).
    pub data: String,

    /// Frame number when this snapshot was taken.
    pub frame: u64,

    /// Free-form metadata (e.g., game name, settings version).
    pub metadata: std::collections::HashMap<String, String>,
}

impl SceneSnapshot {
    /// Create a new scene snapshot.
    pub fn new(scene_id: super::SceneId, data: String, frame: u64) -> Self {
        Self {
            scene_id,
            data,
            frame,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Add a metadata key-value pair.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// A store for scene state snapshots, keyed by SceneId.
///
/// The SceneManager maintains one `SceneStateStore` instance that accumulates
/// state over the session. Snapshots are created on suspend and consumed on resume.
#[derive(Debug, Clone)]
pub struct SceneStateStore {
    snapshots: std::collections::HashMap<super::SceneId, SceneSnapshot>,
    max_snapshots: usize,
}

impl SceneStateStore {
    /// Create a new state store.
    pub fn new() -> Self {
        Self {
            snapshots: std::collections::HashMap::new(),
            max_snapshots: 64,
        }
    }

    /// Set the maximum number of snapshots (older ones are evicted).
    pub fn with_max_snapshots(mut self, max: usize) -> Self {
        self.max_snapshots = max;
        self
    }

    /// Store a snapshot, replacing any existing snapshot for the same SceneId.
    pub fn store(&mut self, snapshot: SceneSnapshot) {
        if self.snapshots.len() >= self.max_snapshots {
            // Evict oldest entry
            if let Some(oldest_key) = self.snapshots.keys().next().cloned() {
                self.snapshots.remove(&oldest_key);
            }
        }
        let id = snapshot.scene_id.clone();
        self.snapshots.insert(id, snapshot);
    }

    /// Retrieve and remove a snapshot for the given SceneId.
    pub fn take(&mut self, scene_id: &super::SceneId) -> Option<SceneSnapshot> {
        self.snapshots.remove(scene_id)
    }

    /// Retrieve a snapshot without removing it.
    pub fn peek(&self, scene_id: &super::SceneId) -> Option<&SceneSnapshot> {
        self.snapshots.get(scene_id)
    }

    /// Returns `true` if a snapshot exists for the given SceneId.
    pub fn has(&self, scene_id: &super::SceneId) -> bool {
        self.snapshots.contains_key(scene_id)
    }

    /// Number of stored snapshots.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns `true` if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Clear all snapshots.
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }
}

impl Default for SceneStateStore {
    fn default() -> Self {
        Self::new()
    }
}
