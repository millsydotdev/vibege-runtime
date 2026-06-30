use tracing::info;

use crate::config::{CONFIG_VERSION, MIN_SUPPORTED_VERSION, VibegeConfig};

/// Run automatic migration from any earlier version to the current version.
/// Returns `true` if any migration was applied.
pub fn run(config: &mut VibegeConfig) -> bool {
    let mut migrated = false;

    while config.version < CONFIG_VERSION {
        match config.version {
            1 => {
                migrate_v1_to_v2(config);
                config.version = 2;
                migrated = true;
            }
            v => {
                info!(version = v, "Unknown config version, resetting to defaults");
                *config = VibegeConfig::default();
                return true;
            }
        }
    }

    if config.version < MIN_SUPPORTED_VERSION {
        info!(
            version = config.version,
            min = MIN_SUPPORTED_VERSION,
            "Config version too old, resetting to defaults"
        );
        *config = VibegeConfig::default();
        return true;
    }

    migrated
}

/// Migrate from v1 to v2.
///
/// Changes:
///   - Add `graphics` section (defaults)
///   - Add `input` section (defaults)
///   - Add `developer` section (defaults)
///   - Add `active_profile = "Default"`
///   - Add `profiles = {}`
///   - Add `overlay.start_hidden = false`
///   - Add `general.theme = "dark"`
///   - Preserve all v1 fields as-is
fn migrate_v1_to_v2(config: &mut VibegeConfig) {
    info!("Migrating config from v1 to v2");

    // v2 adds these with defaults — serde default handles them.
    // We just ensure version is updated.
    config.overlay.start_hidden = false;
    if config.general.theme.is_empty() {
        config.general.theme = "dark".to_string();
    }

    info!(version = 2, "Config migrated to v2");
}
