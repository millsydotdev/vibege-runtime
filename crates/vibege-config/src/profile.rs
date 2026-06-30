use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::VibegeConfig;

/// A map of named profiles.
pub type ProfileMap = HashMap<String, ProfileConfig>;

/// A named profile stores a complete configuration snapshot.
///
/// When a profile is active, its values override the base config.
/// Profiles are fully independent — switching profiles restores all
/// settings that were saved when the profile was created or last updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Display name for the profile.
    pub label: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
    /// The full config snapshot for this profile.
    #[serde(flatten)]
    pub config: VibegeConfig,
}

impl ProfileConfig {
    pub fn new(name: &str, config: VibegeConfig) -> Self {
        Self {
            label: name.to_string(),
            description: String::new(),
            config,
        }
    }
}

/// Built-in profile names.
pub const PROFILE_DEFAULT: &str = "Default";
pub const PROFILE_GAMING: &str = "Gaming";
pub const PROFILE_PRODUCTIVITY: &str = "Productivity";
pub const PROFILE_STREAMING: &str = "Streaming";
pub const PROFILE_LOW_POWER: &str = "Low Power";

/// Return true if the name is a built-in profile.
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        PROFILE_DEFAULT
            | PROFILE_GAMING
            | PROFILE_PRODUCTIVITY
            | PROFILE_STREAMING
            | PROFILE_LOW_POWER
    )
}

/// Create default profiles populated with sensible overrides.
pub fn default_profiles() -> ProfileMap {
    let mut map = ProfileMap::new();

    // Default profile — no overrides
    map.insert(
        PROFILE_DEFAULT.to_string(),
        ProfileConfig::new(PROFILE_DEFAULT, VibegeConfig::default()),
    );

    // Gaming — higher volume, fullscreen, high performance
    {
        let mut cfg = VibegeConfig::default();
        cfg.audio.volume = 1.0;
        cfg.audio.sfx_volume = 1.0;
        cfg.general.performance_mode = "performance".to_string();
        cfg.graphics.fullscreen = true;
        cfg.graphics.vsync = false;
        cfg.graphics.fps_limit = 0;
        map.insert(
            PROFILE_GAMING.to_string(),
            ProfileConfig::new(PROFILE_GAMING, cfg),
        );
    }

    // Productivity — windowed, balanced
    {
        let mut cfg = VibegeConfig::default();
        cfg.audio.volume = 0.3;
        cfg.general.performance_mode = "balanced".to_string();
        cfg.graphics.fullscreen = false;
        cfg.graphics.vsync = true;
        map.insert(
            PROFILE_PRODUCTIVITY.to_string(),
            ProfileConfig::new(PROFILE_PRODUCTIVITY, cfg),
        );
    }

    // Streaming — windowed, muted, performance
    {
        let mut cfg = VibegeConfig::default();
        cfg.audio.muted = true;
        cfg.audio.volume = 0.0;
        cfg.general.performance_mode = "performance".to_string();
        cfg.graphics.fullscreen = false;
        cfg.graphics.vsync = true;
        map.insert(
            PROFILE_STREAMING.to_string(),
            ProfileConfig::new(PROFILE_STREAMING, cfg),
        );
    }

    // Low Power — battery saving, low res, low fps
    {
        let mut cfg = VibegeConfig::default();
        cfg.audio.volume = 0.5;
        cfg.general.performance_mode = "battery".to_string();
        cfg.graphics.width = 960;
        cfg.graphics.height = 540;
        cfg.graphics.vsync = true;
        cfg.graphics.fps_limit = 30;
        cfg.input.mouse_sensitivity = 0.8;
        map.insert(
            PROFILE_LOW_POWER.to_string(),
            ProfileConfig::new(PROFILE_LOW_POWER, cfg),
        );
    }

    map
}
