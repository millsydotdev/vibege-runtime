use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{info, warn};
use vibege_core::RuntimeEvent;

use super::context::RuntimeContext;
use super::error::RuntimeError;
use super::lifecycle::GameLifecycle;
use super::state::RuntimeState;
use super::validator::{PackageValidator, ValidationReport};

use crate::scenes::game_manager::GameSession;
use vibege_sdk::SdkState;

/// Controls the lifecycle of a single game session.
///
/// Wraps `GameSession` with a deterministic state machine and
/// provides all lifecycle transitions with proper error handling.
pub struct SessionController {
    /// Current runtime state.
    state: RuntimeState,
    /// The underlying game session (Lua VM + SDK).
    session: Option<GameSession>,
    /// Context with engine services.
    ctx: RuntimeContext,
    /// Performance tracking.
    started_at: Option<Instant>,
    total_runtime: Duration,
    update_count: u64,
    suspend_count: u64,
}

impl SessionController {
    pub fn new(ctx: RuntimeContext) -> Self {
        Self {
            state: RuntimeState::Discovered,
            session: None,
            ctx,
            started_at: None,
            total_runtime: Duration::default(),
            update_count: 0,
            suspend_count: 0,
        }
    }

    pub fn state(&self) -> RuntimeState {
        self.state
    }

    pub fn context(&self) -> &RuntimeContext {
        &self.ctx
    }

    pub fn context_mut(&mut self) -> &mut RuntimeContext {
        &mut self.ctx
    }

    pub fn update_count(&self) -> u64 {
        self.update_count
    }

    pub fn suspend_count(&self) -> u64 {
        self.suspend_count
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.map(|t| t.elapsed()).unwrap_or_default()
    }

    pub fn session(&self) -> Option<&GameSession> {
        self.session.as_ref()
    }

    fn transition(&mut self, next: RuntimeState) -> Result<(), RuntimeError> {
        if !self.state.can_transition_to(&next) {
            return Err(RuntimeError::LuaRuntimeError(format!(
                "Invalid state transition: {} -> {}",
                self.state, next
            )));
        }
        info!(from = %self.state, to = %next, game = %self.ctx.game_name, "Session state transition");
        self.state = next;
        Ok(())
    }

    /// Mount the runtime context.
    pub fn mount(&mut self) -> Result<(), RuntimeError> {
        self.transition(RuntimeState::Mounted)
    }

    /// Validate the package.
    pub fn validate(&self, engine_version: &str) -> ValidationReport {
        let entry_data = Some(self.ctx.source.as_bytes());
        PackageValidator::validate(&self.ctx.manifest, entry_data, &[], engine_version)
    }

    /// Initialize the game session (create Lua VM, register SDK, load source).
    pub fn initialize(&mut self) -> Result<(), RuntimeError> {
        self.transition(RuntimeState::Initialized)?;

        let renderer = Arc::clone(&self.ctx.renderer);
        let input = Arc::clone(&self.ctx.input);
        let audio = self.ctx.audio.clone();
        let assets = Arc::clone(&self.ctx.assets);

        let sdk_state = SdkState::new("0.2.0-alpha.1", 800, 600);
        let session = GameSession::load(
            &self.ctx.game_name,
            &self.ctx.source,
            &renderer,
            &input,
            &audio,
            &assets,
            self.ctx.event_bus.clone(),
            800,
            600,
            "0.2.0-alpha.1",
            &sdk_state,
        )
        .map_err(RuntimeError::SdkRegistrationFailed)?;

        self.session = Some(session);
        info!(game = %self.ctx.game_name, "Session initialized");
        Ok(())
    }

    /// Start the game (transition to Running).
    pub fn start(&mut self) -> Result<(), RuntimeError> {
        self.transition(RuntimeState::Running)?;
        self.started_at = Some(Instant::now());

        if let Some(ref bus) = self.ctx.event_bus {
            bus.publish(&RuntimeEvent::GameStarted {
                name: self.ctx.game_name.clone(),
            });
        }

        info!(game = %self.ctx.game_name, "Game started");
        Ok(())
    }

    /// Update the game logic.
    pub fn update(&mut self, dt: f64) -> Result<(), RuntimeError> {
        if self.state != RuntimeState::Running {
            return Err(RuntimeError::SessionNotActive);
        }

        if let Some(ref session) = self.session {
            session.update(dt).map_err(RuntimeError::LuaRuntimeError)?;
        }

        self.update_count += 1;
        self.total_runtime += Duration::from_secs_f64(dt);
        Ok(())
    }

    /// Render the game.
    pub fn render(&self) -> Result<(), RuntimeError> {
        if self.state != RuntimeState::Running {
            return Err(RuntimeError::SessionNotActive);
        }

        if let Some(ref session) = self.session {
            session.render().map_err(RuntimeError::LuaRuntimeError)?;
        }

        Ok(())
    }

    /// Suspend the game.
    pub fn suspend(&mut self) -> Result<(), RuntimeError> {
        if self.state != RuntimeState::Running && self.state != RuntimeState::Paused {
            return Err(RuntimeError::SessionNotActive);
        }

        if let Some(ref session) = self.session {
            session.suspend();
        }

        self.transition(RuntimeState::Suspended)?;
        self.suspend_count += 1;

        if let Some(ref bus) = self.ctx.event_bus {
            bus.publish(&RuntimeEvent::GameSuspended {
                name: self.ctx.game_name.clone(),
            });
        }

        info!(game = %self.ctx.game_name, "Game suspended");
        Ok(())
    }

    /// Resume the game.
    pub fn resume(&mut self) -> Result<(), RuntimeError> {
        if self.state != RuntimeState::Suspended && self.state != RuntimeState::Paused {
            return Err(RuntimeError::SessionNotActive);
        }

        if let Some(ref session) = self.session {
            session.resume();
        }

        self.transition(RuntimeState::Running)?;

        if let Some(ref bus) = self.ctx.event_bus {
            bus.publish(&RuntimeEvent::GameResumed {
                name: self.ctx.game_name.clone(),
            });
        }

        info!(game = %self.ctx.game_name, "Game resumed");
        Ok(())
    }

    /// Pause the game (overlay shown).
    pub fn pause(&mut self) -> Result<(), RuntimeError> {
        if self.state != RuntimeState::Running {
            return Err(RuntimeError::SessionNotActive);
        }

        self.transition(RuntimeState::Paused)?;
        info!(game = %self.ctx.game_name, "Game paused");
        Ok(())
    }

    /// Stop the game permanently.
    pub fn stop(&mut self) -> Result<(), RuntimeError> {
        if self.state == RuntimeState::Stopped {
            return Ok(());
        }

        if let Some(ref bus) = self.ctx.event_bus {
            bus.publish(&RuntimeEvent::GameExited {
                name: self.ctx.game_name.clone(),
            });
        }

        self.session = None;
        self.transition(RuntimeState::Stopped)?;
        info!(game = %self.ctx.game_name, total_runtime_ms = self.total_runtime.as_millis(), "Game stopped");
        Ok(())
    }

    /// Unload game assets.
    pub fn unload(&mut self) -> Result<(), RuntimeError> {
        if self.state != RuntimeState::Stopped {
            self.stop()?;
        }

        self.ctx.assets.clear();
        self.transition(RuntimeState::Unloaded)?;
        Ok(())
    }

    /// Final cleanup.
    pub fn cleanup(&mut self) {
        if let Err(e) = self.unload() {
            warn!(game = %self.ctx.game_name, error = %e, "Cleanup unload failed");
        }
        self.state = RuntimeState::CleanedUp;
        info!(game = %self.ctx.game_name, "Session cleaned up");
    }
}

impl GameLifecycle for SessionController {
    fn on_mount(&mut self) -> Result<(), RuntimeError> {
        self.mount()
    }

    fn on_initialize(&mut self) -> Result<(), RuntimeError> {
        self.initialize()
    }

    fn on_start(&mut self) -> Result<(), RuntimeError> {
        self.start()
    }

    fn on_update(&mut self, dt: f64) -> Result<(), RuntimeError> {
        self.update(dt)
    }

    fn on_render(&mut self) -> Result<(), RuntimeError> {
        self.render()
    }

    fn on_suspend(&mut self) -> Result<(), RuntimeError> {
        self.suspend()
    }

    fn on_resume(&mut self) -> Result<(), RuntimeError> {
        self.resume()
    }

    fn on_pause(&mut self) -> Result<(), RuntimeError> {
        self.pause()
    }

    fn on_stop(&mut self) -> Result<(), RuntimeError> {
        self.stop()
    }

    fn on_unload(&mut self) -> Result<(), RuntimeError> {
        self.unload()
    }

    fn on_cleanup(&mut self) {
        self.cleanup();
    }
}

impl Drop for SessionController {
    fn drop(&mut self) {
        if self.state != RuntimeState::CleanedUp {
            self.cleanup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::RuntimeState;

    /// Session controller tests focus on state transitions.
    /// Full integration tests require GPU and audio hardware.
    #[test]
    fn test_mount_from_discovered_is_valid() {
        assert!(RuntimeState::Discovered.can_transition_to(&RuntimeState::Mounted));
    }

    #[test]
    fn test_start_from_discovered_is_invalid() {
        assert!(!RuntimeState::Discovered.can_transition_to(&RuntimeState::Running));
    }

    #[test]
    fn test_run_can_suspend() {
        assert!(RuntimeState::Running.can_transition_to(&RuntimeState::Suspended));
    }

    #[test]
    fn test_suspend_can_resume() {
        assert!(RuntimeState::Suspended.can_transition_to(&RuntimeState::Running));
    }

    #[test]
    fn test_cleaned_up_has_no_transitions() {
        assert!(RuntimeState::CleanedUp.valid_transitions().is_empty());
    }
}
