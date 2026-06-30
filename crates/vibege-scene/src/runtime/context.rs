use std::path::PathBuf;
use std::sync::Arc;

use vibege_asset::AssetManager;
use vibege_audio::AudioSystem;
use vibege_core::EventBus;
use vibege_renderer::Renderer;

use super::state::RuntimeState;

/// Typed metadata for a game package.
#[derive(Debug, Clone)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub entry_point: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub engine_version: Option<String>,
    pub sdk_version: Option<String>,
    pub permissions: Vec<String>,
    pub assets: Vec<String>,
}

impl PackageManifest {
    pub fn new(name: &str, version: &str, entry_point: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            entry_point: entry_point.to_string(),
            author: None,
            description: None,
            engine_version: None,
            sdk_version: None,
            permissions: Vec::new(),
            assets: Vec::new(),
        }
    }
}

/// Runtime context passed to the game session, providing access
/// to all engine services.
#[derive(Clone)]
pub struct RuntimeContext {
    pub game_name: String,
    pub manifest: PackageManifest,
    pub state: RuntimeState,
    pub source: String,
    pub base_path: Option<PathBuf>,
    pub renderer: Arc<Renderer>,
    pub input: Arc<std::sync::Mutex<vibege_input::InputManager>>,
    pub audio: Option<Arc<AudioSystem>>,
    pub assets: Arc<AssetManager>,
    pub event_bus: Option<Arc<EventBus>>,
}

impl RuntimeContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        game_name: String,
        manifest: PackageManifest,
        source: String,
        base_path: Option<PathBuf>,
        renderer: Arc<Renderer>,
        input: Arc<std::sync::Mutex<vibege_input::InputManager>>,
        audio: Option<Arc<AudioSystem>>,
        assets: Arc<AssetManager>,
        event_bus: Option<Arc<EventBus>>,
    ) -> Self {
        Self {
            game_name,
            manifest,
            state: RuntimeState::Discovered,
            source,
            base_path,
            renderer,
            input,
            audio,
            assets,
            event_bus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_manifest_new() {
        let m = PackageManifest::new("test", "1.0.0", "main.lua");
        assert_eq!(m.name, "test");
        assert_eq!(m.version, "1.0.0");
        assert_eq!(m.entry_point, "main.lua");
        assert!(m.author.is_none());
        assert!(m.permissions.is_empty());
    }

    #[test]
    fn test_package_manifest_full() {
        let mut m = PackageManifest::new("full", "2.0.0", "game.lua");
        m.author = Some("VibeGE".into());
        m.description = Some("Test".into());
        m.permissions = vec!["storage".into(), "network".into()];
        assert_eq!(m.author.as_deref(), Some("VibeGE"));
        assert_eq!(m.permissions.len(), 2);
    }
}
