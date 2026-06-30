use super::error::RuntimeError;

/// A trait defining the complete game lifecycle.
///
/// Every implementor provides hooks for each lifecycle stage.
/// The runtime calls these in the deterministic order defined
/// by `RuntimeState::valid_transitions()`.
pub trait GameLifecycle {
    /// Called when the package is discovered (found on disk or in registry).
    fn on_discover(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called to mount the package contents (load ZIP, enumerate entries).
    fn on_mount(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called to validate the package (manifest, integrity, compatibility).
    fn on_validate(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called to initialize the runtime (create VM, register SDK).
    fn on_initialize(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called when the game starts running.
    fn on_start(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called per frame while the game is running.
    fn on_update(&mut self, _dt: f64) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called per frame to render.
    fn on_render(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called to suspend the game (save state, pause audio).
    fn on_suspend(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called to resume the game after suspension.
    fn on_resume(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called to pause the game (overlay shown on top).
    fn on_pause(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called to stop the game permanently.
    fn on_stop(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called to unload assets and free resources.
    fn on_unload(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Called for final cleanup. Must not fail.
    fn on_cleanup(&mut self) {}
}
