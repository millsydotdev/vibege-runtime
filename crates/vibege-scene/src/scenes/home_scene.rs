use mlua::{Function, Lua};
use std::rc::Rc;
use tracing::info;
use crate::scene::{Scene, SceneId, SceneContext, SceneAction, SceneResult};

const LAUNCHER_SOURCE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/launcher.lua"));

pub struct HomeScene {
    platform_lua: Rc<Lua>,
}

impl HomeScene {
    pub fn new(platform_lua: Rc<Lua>) -> Self {
        Self { platform_lua }
    }
}

impl Scene for HomeScene {
    fn id(&self) -> SceneId { SceneId::Home }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        self.platform_lua.load(LAUNCHER_SOURCE).exec().map_err(|e| format!("Lua error: {e}"))?;
        if let Ok(init_fn) = self.platform_lua.globals().get::<Function>("init") {
            init_fn.call::<()>(()).ok();
        }
        info!("HomeScene: launcher loaded");
        Ok(SceneAction::Continue)
    }

    fn on_enter(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        if let Ok(init_fn) = self.platform_lua.globals().get::<Function>("init") {
            let _ = init_fn.call::<()>(());
        }
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, _ctx: &mut SceneContext, dt: f64) -> SceneResult {
        if let Ok(update_fn) = self.platform_lua.globals().get::<Function>("update") {
            update_fn.call::<()>(dt).map_err(|e| format!("Lua: {e}"))?;
        }
        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        if let Ok(render_fn) = self.platform_lua.globals().get::<Function>("render") {
            render_fn.call::<()>(()).map_err(|e| format!("Lua: {e}"))?;
        }
        Ok(SceneAction::Continue)
    }
}
