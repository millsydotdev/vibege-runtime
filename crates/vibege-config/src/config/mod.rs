use serde::{Deserialize, Serialize};

use crate::migration;
use crate::profile::ProfileMap;
use crate::validation::Validate;

pub mod audio;
pub mod developer;
pub mod graphics;
pub mod input;

pub use audio::AudioConfig;
pub use developer::DeveloperConfig;
pub use graphics::GraphicsConfig;
pub use input::InputConfig;

/// Current configuration schema version.
/// Increment when making breaking changes to the config format.
pub const CONFIG_VERSION: u32 = 2;

/// Minimum supported version — configs older than this are reset to defaults.
pub const MIN_SUPPORTED_VERSION: u32 = 1;

/// Top-level configuration.
///
/// # Backward Compatibility
///
/// The `#[serde(default)]` on each field means old config files missing new
/// sections will silently get defaults. Fields added to existing sections
/// also get the default value when the key is absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VibegeConfig {
    /// Schema version. Missing (v1 files) default to 1.
    #[serde(default = "version_v1")]
    pub version: u32,

    /// Overlay configuration (originated in v1).
    pub overlay: OverlayConfig,

    /// Audio configuration.
    pub audio: AudioConfig,

    /// General / runtime configuration.
    pub general: GeneralConfig,

    /// Graphics / display configuration (added in v2).
    #[serde(default)]
    pub graphics: GraphicsConfig,

    /// Input / mouse configuration (added in v2).
    #[serde(default)]
    pub input: InputConfig,

    /// Developer options (added in v2).
    #[serde(default)]
    pub developer: DeveloperConfig,

    /// Active profile name (added in v2).
    #[serde(default = "default_profile_name")]
    pub active_profile: String,

    /// Named profiles (added in v2).
    #[serde(default)]
    pub profiles: ProfileMap,
}

fn version_v1() -> u32 {
    1
}

fn default_profile_name() -> String {
    "Default".to_string()
}

impl Default for VibegeConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            overlay: OverlayConfig::default(),
            audio: AudioConfig::default(),
            general: GeneralConfig::default(),
            graphics: GraphicsConfig::default(),
            input: InputConfig::default(),
            developer: DeveloperConfig::default(),
            active_profile: default_profile_name(),
            profiles: ProfileMap::new(),
        }
    }
}

impl Validate for VibegeConfig {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        errors.extend(self.overlay.validate().err().unwrap_or_default());
        errors.extend(self.audio.validate().err().unwrap_or_default());
        errors.extend(self.graphics.validate().err().unwrap_or_default());
        errors.extend(self.input.validate().err().unwrap_or_default());
        errors.extend(self.developer.validate().err().unwrap_or_default());
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn sanitize(&mut self) {
        self.overlay.sanitize();
        self.audio.sanitize();
        self.graphics.sanitize();
        self.input.sanitize();
        self.developer.sanitize();
    }
}

impl VibegeConfig {
    /// Run automatic migration from any earlier version to the current version.
    /// Returns true if a migration was applied.
    pub fn migrate(&mut self) -> bool {
        migration::run(self)
    }

    /// Validate and auto-fix common issues. Returns Ok if valid after sanitize,
    /// or Err with remaining issues that could not be auto-fixed.
    pub fn validate_and_fix(&mut self) -> Result<(), Vec<String>> {
        self.sanitize();
        self.validate()
    }
}

// ─── Overlay Config (v1) ─────────────────────────────────────────

/// Configuration for the overlay window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayConfig {
    #[serde(default = "default_hotkey_mod")]
    pub hotkey_modifiers: String,
    #[serde(default = "default_hotkey_key")]
    pub hotkey_key: String,
    #[serde(default = "default_position")]
    pub position: String,
    #[serde(default = "default_overlay_width")]
    pub width: u32,
    #[serde(default = "default_overlay_height")]
    pub height: u32,
    /// Whether the overlay should start hidden.
    #[serde(default)]
    pub start_hidden: bool,
    /// Last known overlay X position (for session persistence).
    #[serde(default)]
    pub last_x: i32,
    /// Last known overlay Y position (for session persistence).
    #[serde(default)]
    pub last_y: i32,
    /// Last known monitor name (for multi-monitor restoration).
    #[serde(default)]
    pub last_monitor: String,
    /// Whether the overlay was visible when last saved.
    #[serde(default)]
    pub last_visible: bool,
    /// HWND of the target window to overlay on top of (empty = free float).
    #[serde(default)]
    pub target_hwnd: String,
}

fn default_hotkey_mod() -> String {
    "ctrl+shift".to_string()
}
fn default_hotkey_key() -> String {
    "v".to_string()
}
fn default_position() -> String {
    "center".to_string()
}
fn default_overlay_width() -> u32 {
    800
}
fn default_overlay_height() -> u32 {
    600
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            hotkey_modifiers: default_hotkey_mod(),
            hotkey_key: default_hotkey_key(),
            position: default_position(),
            width: default_overlay_width(),
            height: default_overlay_height(),
            start_hidden: false,
            last_x: 0,
            last_y: 0,
            last_monitor: String::new(),
            last_visible: false,
            target_hwnd: String::new(),
        }
    }
}

impl Validate for OverlayConfig {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let valid_mods = [
            "ctrl+shift",
            "ctrl+alt",
            "alt+shift",
            "ctrl",
            "alt",
            "shift",
        ];
        if !valid_mods.contains(&self.hotkey_modifiers.as_str()) {
            errors.push(format!(
                "overlay.hotkey_modifiers '{}' not in {:?}",
                self.hotkey_modifiers, valid_mods
            ));
        }
        let valid_keys = ["v", "g", "b", "h", "space", "tab", "escape"];
        if !valid_keys.contains(&self.hotkey_key.as_str()) {
            errors.push(format!(
                "overlay.hotkey_key '{}' not in {:?}",
                self.hotkey_key, valid_keys
            ));
        }
        let valid_pos = [
            "center",
            "top-left",
            "top-right",
            "bottom-left",
            "bottom-right",
        ];
        if !valid_pos.contains(&self.position.as_str()) {
            errors.push(format!(
                "overlay.position '{}' not in {:?}",
                self.position, valid_pos
            ));
        }
        if self.width < 200 || self.width > 7680 {
            errors.push(format!(
                "overlay.width ({}) out of range 200–7680",
                self.width
            ));
        }
        if self.height < 150 || self.height > 4320 {
            errors.push(format!(
                "overlay.height ({}) out of range 150–4320",
                self.height
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn sanitize(&mut self) {
        let valid_mods = [
            "ctrl+shift",
            "ctrl+alt",
            "alt+shift",
            "ctrl",
            "alt",
            "shift",
        ];
        if !valid_mods.contains(&self.hotkey_modifiers.as_str()) {
            self.hotkey_modifiers = default_hotkey_mod();
        }
        let valid_keys = ["v", "g", "b", "h", "space", "tab", "escape"];
        if !valid_keys.contains(&self.hotkey_key.as_str()) {
            self.hotkey_key = default_hotkey_key();
        }
        let valid_pos = [
            "center",
            "top-left",
            "top-right",
            "bottom-left",
            "bottom-right",
        ];
        if !valid_pos.contains(&self.position.as_str()) {
            self.position = default_position();
        }
        self.width = self.width.clamp(200, 7680);
        self.height = self.height.clamp(150, 4320);
    }
}

// ─── General Config (v1) ─────────────────────────────────────────

/// Runtime / platform configuration. Some fields are deprecated in v2
/// and kept only for backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_startup")]
    pub startup_behavior: String,
    #[serde(default = "default_perf")]
    pub performance_mode: String,
    #[serde(default)]
    pub first_run_complete: bool,
    #[serde(default = "default_backend_url")]
    pub backend_url: String,
    /// Theme preference (added in v2).
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_startup() -> String {
    "hidden".to_string()
}
fn default_perf() -> String {
    "balanced".to_string()
}
fn default_backend_url() -> String {
    "http://localhost:3000/api/v1".to_string()
}
fn default_theme() -> String {
    "dark".to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            startup_behavior: default_startup(),
            performance_mode: default_perf(),
            first_run_complete: false,
            backend_url: default_backend_url(),
            theme: default_theme(),
        }
    }
}

impl Validate for GeneralConfig {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let valid_startup = ["hidden", "shown", "minimised", "minimized"];
        if !valid_startup.contains(&self.startup_behavior.as_str()) {
            errors.push(format!(
                "general.startup_behavior '{}' not in {:?}",
                self.startup_behavior, valid_startup
            ));
        }
        let valid_perf = ["battery", "balanced", "performance"];
        if !valid_perf.contains(&self.performance_mode.as_str()) {
            errors.push(format!(
                "general.performance_mode '{}' not in {:?}",
                self.performance_mode, valid_perf
            ));
        }
        let valid_theme = ["dark", "light", "system"];
        if !valid_theme.contains(&self.theme.as_str()) {
            errors.push(format!(
                "general.theme '{}' not in {:?}",
                self.theme, valid_theme
            ));
        }
        if !self.backend_url.starts_with("http://") && !self.backend_url.starts_with("https://") {
            errors.push(format!(
                "general.backend_url '{}' does not start with http:// or https://",
                self.backend_url
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn sanitize(&mut self) {
        let valid_startup = ["hidden", "shown", "minimised", "minimized"];
        if !valid_startup.contains(&self.startup_behavior.as_str()) {
            self.startup_behavior = default_startup();
        } else if self.startup_behavior == "minimized" {
            self.startup_behavior = "minimised".to_string();
        }
        let valid_perf = ["battery", "balanced", "performance"];
        if !valid_perf.contains(&self.performance_mode.as_str()) {
            self.performance_mode = default_perf();
        }
        let valid_theme = ["dark", "light", "system"];
        if !valid_theme.contains(&self.theme.as_str()) {
            self.theme = default_theme();
        }
        if !self.backend_url.starts_with("http://") && !self.backend_url.starts_with("https://") {
            self.backend_url = default_backend_url();
        }
    }
}
