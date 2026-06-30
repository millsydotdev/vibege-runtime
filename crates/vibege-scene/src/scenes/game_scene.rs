use super::game_manager::GameSession;
use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use std::sync::Arc;
use std::sync::Mutex;
use tracing::info;
use vibege_audio::AudioSystem;
use vibege_input::InputManager;
use vibege_renderer::Renderer;

pub struct GameScene {
    session: Option<GameSession>,
    game_source: String,
    renderer: Arc<Renderer>,
    input: Arc<Mutex<InputManager>>,
    audio: Option<Arc<AudioSystem>>,
}

impl GameScene {
    pub fn new(
        source: String,
        renderer: Arc<Renderer>,
        input: Arc<Mutex<InputManager>>,
        audio: Option<Arc<AudioSystem>>,
    ) -> Self {
        Self {
            session: None,
            game_source: source,
            renderer,
            input,
            audio,
        }
    }
}

impl Scene for GameScene {
    fn id(&self) -> SceneId {
        SceneId::Game
    }

    fn on_create(&mut self, ctx: &mut SceneContext) -> SceneResult {
        info!("GameScene: creating game session");
        let event_bus = ctx.event_bus.clone();
        match GameSession::load(
            "game",
            &self.game_source,
            &self.renderer,
            &self.input,
            &self.audio,
            event_bus,
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

    fn on_enter(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        if let Some(ref session) = self.session {
            session.resume();
        }
        Ok(SceneAction::Continue)
    }

    fn on_suspend(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        if let Some(ref session) = self.session {
            session.suspend();
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
                info!("Game exited: {e}");
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
                info!("Game render exited: {e}");
                Ok(SceneAction::Pop)
            }
        }
    }
}
