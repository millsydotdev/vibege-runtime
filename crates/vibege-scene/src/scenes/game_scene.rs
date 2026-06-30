use super::game_manager::GameSession;
use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::info;

pub struct GameScene {
    session: Option<GameSession>,
    game_source: String,
    game_name: String,
    snapshot_id: Option<String>,
}

impl GameScene {
    pub fn new(source: String, game_name: String) -> Self {
        Self {
            session: None,
            game_source: source,
            game_name,
            snapshot_id: None,
        }
    }
}

impl Scene for GameScene {
    fn id(&self) -> SceneId {
        SceneId::Game
    }

    fn on_create(&mut self, ctx: &mut SceneContext) -> SceneResult {
        info!(game = %self.game_name, "GameScene: creating game session");
        let event_bus = ctx.event_bus.clone();
        match GameSession::load(
            &self.game_name,
            &self.game_source,
            &ctx.renderer,
            &ctx.input,
            &ctx.audio,
            &ctx.assets,
            event_bus,
            ctx.screen_width,
            ctx.screen_height,
            "0.2.0-alpha.1",
        ) {
            Ok(session) => {
                self.session = Some(session);
                Ok(SceneAction::Continue)
            }
            Err(e) => {
                info!("GameScene: failed to load game: {e}");
                Ok(SceneAction::Pop)
            }
        }
    }

    fn on_enter(&mut self, ctx: &mut SceneContext) -> SceneResult {
        if let Some(ref session) = self.session {
            // Restore state from suspension snapshot if available
            if let Some(ref snap_id) = self.snapshot_id {
                if let Some(ref suspension) = ctx.suspension {
                    if let Ok(mut engine) = suspension.lock() {
                        if let Ok(snapshot) = engine.resume(snap_id) {
                            let _state_str =
                                String::from_utf8_lossy(&snapshot.game_state).to_string();
                            info!(game = %self.game_name, "State restored from snapshot {snap_id}");
                        }
                    }
                }
            }
            session.resume();
        }
        Ok(SceneAction::Continue)
    }

    fn on_suspend(&mut self, ctx: &mut SceneContext) -> SceneResult {
        if let Some(ref session) = self.session {
            session.suspend();

            // Save game state via suspension engine
            if let Some(ref suspension) = ctx.suspension {
                if let Some(state_str) = session.get_state() {
                    if let Ok(mut engine) = suspension.lock() {
                        match engine.suspend(state_str.as_bytes(), 0.0, &self.game_name) {
                            Ok(meta) => {
                                let snap_id = meta.id;
                                info!(game = %self.game_name, snap_id = %snap_id, "State saved via suspension engine");
                                self.snapshot_id = Some(snap_id);
                            }
                            Err(e) => {
                                info!(game = %self.game_name, "Suspension save failed: {e}");
                            }
                        }
                    }
                }
            }
        }
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, _ctx: &mut SceneContext, dt: f64) -> SceneResult {
        let Some(ref session) = self.session else {
            return Ok(SceneAction::Pop);
        };
        match session.update(dt) {
            Ok(()) => Ok(SceneAction::Continue),
            Err(e) => {
                info!(game = %self.game_name, "Game exited: {e}");
                Ok(SceneAction::Pop)
            }
        }
    }

    fn on_render(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        let Some(ref session) = self.session else {
            return Ok(SceneAction::Pop);
        };
        match session.render() {
            Ok(()) => Ok(SceneAction::Continue),
            Err(e) => {
                info!(game = %self.game_name, "Game render exited: {e}");
                Ok(SceneAction::Pop)
            }
        }
    }
}
