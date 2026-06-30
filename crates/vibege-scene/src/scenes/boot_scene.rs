use std::rc::Rc;
use tracing::info;
use crate::scene::{Scene, SceneId, SceneContext, SceneAction, SceneResult};

pub struct BootScene {
    initialized: bool,
}

impl BootScene {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl Scene for BootScene {
    fn id(&self) -> SceneId { SceneId::Boot }

    fn on_create(&mut self, ctx: &mut SceneContext) -> SceneResult {
        info!("BootScene: loading config");
        let cfg = ctx.config.get();
        info!(first_run = !cfg.general.first_run_complete, "BootScene: config loaded");
        Ok(SceneAction::Continue)
    }

    fn on_enter(&mut self, ctx: &mut SceneContext) -> SceneResult {
        if self.initialized {
            return Ok(SceneAction::Continue);
        }
        self.initialized = true;

        let lua = Rc::clone(&ctx.platform_lua);
        let cfg = ctx.config.get();
        if !cfg.general.first_run_complete {
            info!("BootScene: first run detected — launching wizard");
            Ok(SceneAction::Push(Box::new(super::first_run_scene::FirstRunScene::new(lua))))
        } else {
            info!("BootScene: returning player — launching home");
            Ok(SceneAction::Replace(Box::new(super::home_scene::HomeScene::new(lua))))
        }
    }
}
