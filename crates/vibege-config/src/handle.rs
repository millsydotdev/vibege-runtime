use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};

use tracing::warn;

use crate::config::{GeneralConfig, OverlayConfig, VibegeConfig};
use crate::profile::{PROFILE_DEFAULT, ProfileConfig};
use crate::validation::Validate;

type WatcherList = Vec<Weak<dyn Fn(&VibegeConfig) + Send + Sync>>;

/// Handle to the shared, thread-safe configuration.
///
/// All access goes through `get()` / `set()` which acquire a mutex lock.
/// For repeated reads, cache the result with `get()` and avoid calling it
/// in hot loops.
pub struct ConfigHandle {
    inner: Mutex<ConfigInner>,
    watchers: Mutex<WatcherList>,
}

struct ConfigInner {
    config: VibegeConfig,
    path: PathBuf,
    dirty: bool,
}

impl Default for ConfigHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigHandle {
    /// Create a new handle by loading the config file.
    /// If loading fails, defaults are used.
    pub fn new() -> Self {
        let path = config_path();
        let mut config = load_config_file(&path);

        // Run automatic migration
        if config.migrate() {
            warn!("Config was migrated — saving migrated version");
            let _ = save_config_file(&path, &config);
        }

        // Validate and sanitize
        config.sanitize();
        let dirty = false;

        Self {
            inner: Mutex::new(ConfigInner {
                config,
                path,
                dirty,
            }),
            watchers: Mutex::new(Vec::new()),
        }
    }

    /// Read the full configuration (cloned).
    pub fn get(&self) -> VibegeConfig {
        self.inner.lock().expect("config lock").config.clone()
    }

    /// Replace the full configuration and persist to disk.
    pub fn set(&self, config: VibegeConfig) {
        let mut guard = self.inner.lock().expect("config lock");
        guard.config = config;
        guard.dirty = false;
        let path = guard.path.clone();
        let cfg = guard.config.clone();
        drop(guard);

        if let Err(e) = save_config_file(&path, &cfg) {
            warn!(error = %e, "Failed to save config");
        }
        self.notify(&cfg);
    }

    /// Returns true if the config has been modified since last save.
    pub fn is_dirty(&self) -> bool {
        self.inner.lock().expect("config lock").dirty
    }

    /// Mark the config as dirty (unsaved changes exist).
    pub fn mark_dirty(&self) {
        self.inner.lock().expect("config lock").dirty = true;
    }

    /// Reload config from disk, discarding in-memory changes.
    /// Migrates and sanitizes on load.
    pub fn reload(&self) {
        let mut guard = self.inner.lock().expect("config lock");
        let mut config = load_config_file(&guard.path);
        config.migrate();
        config.sanitize();
        guard.config = config;
        guard.dirty = false;
        let cfg = guard.config.clone();
        drop(guard);
        self.notify(&cfg);
    }

    /// Reset all settings to factory defaults.
    pub fn reset_to_defaults(&self) {
        let mut config = VibegeConfig::default();
        config.migrate();
        config.sanitize();
        let path = self.inner.lock().expect("config lock").path.clone();
        if let Err(e) = save_config_file(&path, &config) {
            warn!(error = %e, "Failed to save default config");
        }
        let mut guard = self.inner.lock().expect("config lock");
        guard.config = config;
        guard.dirty = false;
        let cfg = guard.config.clone();
        drop(guard);
        self.notify(&cfg);
    }

    /// Check if this is the first ever run.
    pub fn is_first_run(&self) -> bool {
        !self
            .inner
            .lock()
            .expect("config lock")
            .config
            .general
            .first_run_complete
    }

    /// Mark first run as completed and persist.
    pub fn complete_first_run(&self) {
        let mut guard = self.inner.lock().expect("config lock");
        guard.config.general.first_run_complete = true;
        guard.dirty = false;
        let path = guard.path.clone();
        let cfg = guard.config.clone();
        drop(guard);
        if let Err(e) = save_config_file(&path, &cfg) {
            warn!(error = %e, "Failed to save first-run config");
        }
        self.notify(&cfg);
    }

    /// Convenience: read the overlay config (avoids cloning the full struct).
    pub fn overlay(&self) -> OverlayConfig {
        self.inner
            .lock()
            .expect("config lock")
            .config
            .overlay
            .clone()
    }

    /// Convenience: read the general config.
    pub fn general(&self) -> GeneralConfig {
        self.inner
            .lock()
            .expect("config lock")
            .config
            .general
            .clone()
    }

    /// Returns the path to the config file.
    pub fn path(&self) -> PathBuf {
        self.inner.lock().expect("config lock").path.clone()
    }

    /// Export current config as a TOML string.
    pub fn export_toml(&self) -> Result<String, String> {
        let config = self.get();
        toml::to_string_pretty(&config).map_err(|e| format!("Serialization error: {e}"))
    }

    /// Export current config as a JSON string.
    pub fn export_json(&self) -> Result<String, String> {
        let config = self.get();
        serde_json::to_string_pretty(&config).map_err(|e| format!("Serialization error: {e}"))
    }

    /// Import configuration from a TOML string.
    /// Validates the imported config before applying.
    pub fn import_toml(&self, data: &str) -> Result<(), Vec<String>> {
        let mut config: VibegeConfig =
            toml::from_str(data).map_err(|e| vec![format!("Parse error: {e}")])?;
        config.migrate();
        config.validate_and_fix()?;
        self.set(config);
        Ok(())
    }

    /// Import configuration from a JSON string.
    pub fn import_json(&self, data: &str) -> Result<(), Vec<String>> {
        let mut config: VibegeConfig =
            serde_json::from_str(data).map_err(|e| vec![format!("Parse error: {e}")])?;
        config.migrate();
        config.validate_and_fix()?;
        self.set(config);
        Ok(())
    }

    /// Backup current config to a separate path.
    pub fn backup(&self, path: &std::path::Path) -> Result<(), String> {
        let config = self.get();
        let toml_str =
            toml::to_string_pretty(&config).map_err(|e| format!("Serialization error: {e}"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create backup dir: {e}"))?;
        }
        std::fs::write(path, &toml_str).map_err(|e| format!("Backup write error: {e}"))?;
        Ok(())
    }

    /// Restore config from a backup file.
    pub fn restore(&self, path: &std::path::Path) -> Result<(), Vec<String>> {
        let content =
            std::fs::read_to_string(path).map_err(|e| vec![format!("Cannot read backup: {e}")])?;
        self.import_toml(&content)
    }

    // ─── Profile API ────────────────────────────────────────────────

    /// Get the name of the active profile.
    pub fn active_profile(&self) -> String {
        self.inner
            .lock()
            .expect("config lock")
            .config
            .active_profile
            .clone()
    }

    /// Switch to a named profile.
    pub fn switch_profile(&self, name: &str) -> Result<(), String> {
        let mut guard = self.inner.lock().expect("config lock");

        // If the profile doesn't exist, create it from current config
        if !guard.config.profiles.contains_key(name) {
            let snapshot = guard.config.clone();
            guard
                .config
                .profiles
                .insert(name.to_string(), ProfileConfig::new(name, snapshot));
        }

        // Restore the profile's config snapshot
        if let Some(profile) = guard.config.profiles.get(name) {
            guard.config = profile.config.clone();
            guard.config.active_profile = name.to_string();
            guard.dirty = true;
            let path = guard.path.clone();
            let cfg = guard.config.clone();
            drop(guard);

            let _ = save_config_file(&path, &cfg);
            self.notify(&cfg);
            Ok(())
        } else {
            Err(format!("Profile '{name}' not found"))
        }
    }

    /// Save the current config state to the active profile.
    pub fn save_profile(&self, name: &str) {
        let mut guard = self.inner.lock().expect("config lock");
        let snapshot = guard.config.clone();
        guard
            .config
            .profiles
            .insert(name.to_string(), ProfileConfig::new(name, snapshot));
        guard.dirty = true;
        let path = guard.path.clone();
        let cfg = guard.config.clone();
        drop(guard);
        let _ = save_config_file(&path, &cfg);
        self.notify(&cfg);
    }

    /// List all known profile names.
    pub fn list_profiles(&self) -> Vec<String> {
        self.inner
            .lock()
            .expect("config lock")
            .config
            .profiles
            .keys()
            .cloned()
            .collect()
    }

    /// Create a new empty profile from current config.
    pub fn create_profile(&self, name: &str) {
        let mut guard = self.inner.lock().expect("config lock");
        let snapshot = guard.config.clone();
        guard
            .config
            .profiles
            .entry(name.to_string())
            .or_insert_with(|| ProfileConfig::new(name, snapshot));
        guard.dirty = true;
        let path = guard.path.clone();
        drop(guard);
        let _ = save_config_file(&path, &self.inner.lock().expect("config lock").config);
    }

    /// Delete a profile (cannot delete Default).
    pub fn delete_profile(&self, name: &str) -> Result<(), String> {
        if name == PROFILE_DEFAULT {
            return Err("Cannot delete Default profile".to_string());
        }
        let mut guard = self.inner.lock().expect("config lock");
        guard.config.profiles.remove(name);
        if guard.config.active_profile == name {
            guard.config.active_profile = PROFILE_DEFAULT.to_string();
        }
        guard.dirty = true;
        let path = guard.path.clone();
        let cfg = guard.config.clone();
        drop(guard);
        let _ = save_config_file(&path, &cfg);
        self.notify(&cfg);
        Ok(())
    }

    // ─── Change Notification ────────────────────────────────────────

    /// Register a callback invoked whenever the config changes.
    /// Returns a handle that can be dropped to unregister.
    pub fn on_change<F>(&self, f: F) -> ChangeHandle
    where
        F: Fn(&VibegeConfig) + Send + Sync + 'static,
    {
        let cb: Arc<dyn Fn(&VibegeConfig) + Send + Sync> = Arc::new(f);
        let weak = Arc::downgrade(&cb);
        self.watchers.lock().expect("watchers lock").push(weak);
        ChangeHandle { _inner: cb }
    }

    fn notify(&self, config: &VibegeConfig) {
        let mut watchers = self.watchers.lock().expect("watchers lock");
        watchers.retain(|w| {
            if let Some(f) = w.upgrade() {
                f(config);
                true
            } else {
                false
            }
        });
    }
}

/// A handle that keeps a change callback alive.
/// When dropped, the callback is automatically unregistered.
pub struct ChangeHandle {
    _inner: Arc<dyn Fn(&VibegeConfig) + Send + Sync>,
}

// ─── File I/O ─────────────────────────────────────────────────────

fn config_path() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("vibege").join("config.toml")
    } else {
        PathBuf::from(".vibege/config.toml")
    }
}

fn load_config_file(path: &std::path::Path) -> VibegeConfig {
    if !path.exists() {
        let mut config = VibegeConfig::default();
        config.migrate();
        config.sanitize();
        // Save defaults so the file exists next time
        let _ = save_config_file(path, &config);
        return config;
    }

    match std::fs::read_to_string(path) {
        Ok(content) => match toml::from_str(&content) {
            Ok(config) => config,
            Err(e) => {
                warn!(error = %e, path = %path.display(), "Failed to parse config, using defaults");
                VibegeConfig::default()
            }
        },
        Err(e) => {
            warn!(error = %e, path = %path.display(), "Failed to read config, using defaults");
            VibegeConfig::default()
        }
    }
}

fn save_config_file(path: &std::path::Path, config: &VibegeConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create config dir: {e}"))?;
    }
    let content =
        toml::to_string_pretty(config).map_err(|e| format!("Config serialization error: {e}"))?;
    std::fs::write(path, &content).map_err(|e| format!("Cannot write config: {e}"))?;
    Ok(())
}

/// Returns the path to the installed games directory.
pub fn installed_games_dir() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("vibege").join("games")
    } else {
        PathBuf::from(".vibege/installed-games")
    }
}
