use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneKind, SceneResult};
use tracing::info;

/// Fallback scene displayed when another scene encounters a lifecycle error.
///
/// The ErrorScene provides a safe fallback that never fails — so the runtime
/// always has something to show even when a normal scene crashes.
pub struct ErrorScene {
    message: String,
}

impl ErrorScene {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl Scene for ErrorScene {
    fn id(&self) -> SceneId {
        SceneId::Error
    }

    fn kind(&self) -> SceneKind {
        SceneKind::Modal
    }

    fn on_create(&mut self, ctx: &mut SceneContext) -> SceneResult {
        info!(msg = %self.message, "ErrorScene: displayed");
        ctx.renderer.set_clear(0.15, 0.05, 0.05, 1.0);
        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        ctx.renderer
            .draw_rect(200.0, 180.0, 400.0, 240.0, 0.2, 0.05, 0.05, 1.0);
        ctx.renderer
            .draw_rect(200.0, 180.0, 400.0, 40.0, 0.5, 0.1, 0.1, 1.0);
        ctx.renderer
            .draw_text(220.0, 190.0, "Scene Error", 14.0, 1.0, 1.0, 1.0);
        ctx.renderer.draw_text(
            220.0,
            240.0,
            "Something went wrong in this scene.",
            9.0,
            0.8,
            0.8,
            0.8,
        );
        ctx.renderer
            .draw_text(220.0, 265.0, &self.message, 8.0, 0.6, 0.6, 0.6);
        ctx.renderer
            .draw_text(220.0, 380.0, "Press Enter to return", 9.0, 0.8, 0.8, 0.8);
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        let inp = crate::input_helper::InputState::new(&ctx.input, &["enter"]);
        if inp.pressed(0) {
            return Ok(SceneAction::PopModal);
        }
        Ok(SceneAction::Continue)
    }
}
