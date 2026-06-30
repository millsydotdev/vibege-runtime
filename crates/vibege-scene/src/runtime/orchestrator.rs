use std::sync::Arc;
use std::sync::Mutex;

use tracing::info;
use vibege_asset::AssetManager;
use vibege_audio::AudioSystem;
use vibege_core::EventBus;
use vibege_input::InputManager;
use vibege_renderer::Renderer;

use super::context::{PackageManifest, RuntimeContext};
use super::error::RuntimeError;
use super::session::SessionController;
use super::state::RuntimeState;
use super::validator::{PackageValidator, ValidationReport};

/// Top-level orchestrator for game execution.
///
/// Manages the complete lifecycle of a game from discovery through
/// to cleanup. Supports mounting, validation, initialization,
/// running, suspension, and teardown.
pub struct GameRuntime {
    /// Active session controller (None if no game is loaded).
    active: Option<SessionController>,
    /// Engine version for compatibility checks.
    engine_version: String,
    /// Shared engine services.
    renderer: Arc<Renderer>,
    input: Arc<Mutex<InputManager>>,
    audio: Option<Arc<AudioSystem>>,
    assets: Arc<AssetManager>,
    event_bus: Option<Arc<EventBus>>,
}

impl GameRuntime {
    pub fn new(
        renderer: Arc<Renderer>,
        input: Arc<Mutex<InputManager>>,
        audio: Option<Arc<AudioSystem>>,
        assets: Arc<AssetManager>,
        event_bus: Option<Arc<EventBus>>,
        engine_version: &str,
    ) -> Self {
        Self {
            active: None,
            engine_version: engine_version.to_string(),
            renderer,
            input,
            audio,
            assets,
            event_bus,
        }
    }

    /// Check if a game is currently running.
    pub fn is_running(&self) -> bool {
        self.active
            .as_ref()
            .map(|c| c.state() == RuntimeState::Running)
            .unwrap_or(false)
    }

    /// Get the active session controller, if any.
    pub fn active(&self) -> Option<&SessionController> {
        self.active.as_ref()
    }

    /// Get the active session controller (mutable).
    pub fn active_mut(&mut self) -> Option<&mut SessionController> {
        self.active.as_mut()
    }

    /// Current state of the active session.
    pub fn state(&self) -> Option<RuntimeState> {
        self.active.as_ref().map(|c| c.state())
    }

    /// Load a game from a source string.
    pub fn load_from_source(
        &mut self,
        game_name: &str,
        manifest: PackageManifest,
        source: String,
    ) -> Result<&mut SessionController, RuntimeError> {
        // Clean up any existing session
        if let Some(ctrl) = self.active.take() {
            let name = ctrl.context().game_name.clone();
            drop(ctrl);
            info!(game = %name, "Previous session dropped");
        }

        let ctx = RuntimeContext::new(
            game_name.to_string(),
            manifest,
            source,
            None,
            Arc::clone(&self.renderer),
            Arc::clone(&self.input),
            self.audio.clone(),
            Arc::clone(&self.assets),
            self.event_bus.clone(),
        );

        let mut controller = SessionController::new(ctx);
        controller.mount()?;

        let version = self.engine_version.clone();
        let report = controller.validate(&version);
        if !report.passed {
            return Err(RuntimeError::InvalidPackage(format!(
                "Validation failed: {}",
                report.summary()
            )));
        }

        controller.initialize()?;
        controller.start()?;

        self.active = Some(controller);
        Ok(self.active.as_mut().unwrap())
    }

    /// Load a game from a package (.vibepkg) buffer.
    pub fn load_from_package(
        &mut self,
        data: &[u8],
        game_name: &str,
    ) -> Result<&mut SessionController, RuntimeError> {
        // Mount the package
        let _pkg_handle = self
            .assets
            .mount_package(game_name, data)
            .map_err(|e| RuntimeError::InvalidPackage(e.to_string()))?;

        // Read entry point source
        let pkg_asset = self
            .assets
            .get_package_data(game_name)
            .ok_or_else(|| RuntimeError::PackageNotFound(game_name.to_string()))?;

        // Look for manifest
        let mut manifest = PackageManifest::new(game_name, "0.1.0", "src/main.lua");
        if let Some(manifest_data) = pkg_asset.read_entry("vibege.json") {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(manifest_data) {
                if let Some(ep) = json["entry"].as_str() {
                    manifest.entry_point = ep.to_string();
                }
                if let Some(v) = json["version"].as_str() {
                    manifest.version = v.to_string();
                }
                if let Some(name) = json["name"].as_str() {
                    manifest.name = name.to_string();
                }
                if let Some(author) = json["author"].as_str() {
                    manifest.author = Some(author.to_string());
                }
                if let Some(desc) = json["description"].as_str() {
                    manifest.description = Some(desc.to_string());
                }
                if let Some(perms) = json["permissions"].as_array() {
                    manifest.permissions = perms
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                }
            }
        }

        // Read entry point source
        let entry_source = pkg_asset
            .read_entry(&manifest.entry_point)
            .and_then(|d| std::str::from_utf8(d).ok())
            .ok_or_else(|| RuntimeError::EntryPointNotFound(manifest.entry_point.clone()))?
            .to_string();

        self.load_from_source(game_name, manifest, entry_source)
    }

    /// Update the active game.
    pub fn update(&mut self, dt: f64) -> Result<(), RuntimeError> {
        match self.active.as_mut() {
            Some(ctrl) if ctrl.state() == RuntimeState::Running => ctrl.update(dt),
            _ => Ok(()),
        }
    }

    /// Render the active game.
    pub fn render(&self) -> Result<(), RuntimeError> {
        match self.active.as_ref() {
            Some(ctrl) if ctrl.state() == RuntimeState::Running => ctrl.render(),
            _ => Ok(()),
        }
    }

    /// Suspend the active game.
    pub fn suspend(&mut self) -> Result<(), RuntimeError> {
        match self.active.as_mut() {
            Some(ctrl) if ctrl.state() == RuntimeState::Running => ctrl.suspend(),
            _ => Err(RuntimeError::SessionNotActive),
        }
    }

    /// Resume the active game.
    pub fn resume(&mut self) -> Result<(), RuntimeError> {
        match self.active.as_mut() {
            Some(ctrl) if ctrl.state() == RuntimeState::Suspended => ctrl.resume(),
            _ => Err(RuntimeError::SessionNotActive),
        }
    }

    /// Stop the active game.
    pub fn stop(&mut self) -> Result<(), RuntimeError> {
        if let Some(mut ctrl) = self.active.take() {
            ctrl.stop()?;
            ctrl.unload()?;
            drop(ctrl);
            info!("Game runtime: session stopped and unloaded");
        }
        Ok(())
    }

    /// Shut down the runtime, cleaning up all resources.
    pub fn shutdown(&mut self) {
        if let Some(mut ctrl) = self.active.take() {
            let name = ctrl.context().game_name.clone();
            ctrl.cleanup();
            info!(game = %name, "Game runtime shutdown complete");
        }
    }

    /// Validate a package without loading it.
    pub fn validate_package(
        &self,
        data: &[u8],
        game_name: &str,
    ) -> Result<ValidationReport, RuntimeError> {
        // Quick ZIP header validation
        PackageValidator::validate(
            &PackageManifest::new(game_name, "", ""),
            None,
            &[],
            &self.engine_version,
        );
        // More thorough validation requires mounting the package
        let pkg = vibege_asset::package::PackageMount::mount(data, game_name)
            .map_err(|e| RuntimeError::InvalidPackage(e.to_string()))?;

        let mut manifest = PackageManifest::new(game_name, "0.1.0", "src/main.lua");
        if let Some(manifest_data) = pkg.read_entry("vibege.json") {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(manifest_data) {
                if let Some(ep) = json["entry"].as_str() {
                    manifest.entry_point = ep.to_string();
                }
                if let Some(v) = json["version"].as_str() {
                    manifest.version = v.to_string();
                }
            }
        }

        let entry_data = pkg.read_entry(&manifest.entry_point);
        let asset_names: Vec<String> = pkg.entry_names().iter().map(|s| (*s).to_string()).collect();

        Ok(PackageValidator::validate(
            &manifest,
            entry_data,
            &asset_names,
            &self.engine_version,
        ))
    }
}

impl Drop for GameRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
