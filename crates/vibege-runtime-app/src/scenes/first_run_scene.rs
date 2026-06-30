//! FirstRunScene — temporary wrapper around the existing first-run.lua.
//!
//! During migration, this scene runs the first-run wizard in the platform Lua VM.
//! Once native scenes are built, this will be replaced by a Rust SettingsScene.

use mlua::{Function, Lua};
use tracing::info;
use crate::scene::{Scene, SceneId, SceneContext, SceneAction, SceneResult, PLATFORM_LUA, LuaPtr};

const FIRST_RUN_SOURCE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/first-run.lua"));

fn with_lua<F, T>(f: F) -> Result<T, String>
where F: FnOnce(&Lua) -> Result<T, String>
{
    let LuaPtr(ptr) = PLATFORM_LUA.get().ok_or("Platform Lua not initialized")?;
    let lua: &Lua = unsafe { &**ptr };
    f(lua)
}

pub struct FirstRunScene {
    initialized: bool,
}

impl FirstRunScene {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl Scene for FirstRunScene {
    fn id(&self) -> SceneId { SceneId::FirstRun }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        with_lua(|lua| {
            lua.load(FIRST_RUN_SOURCE).exec().map_err(|e| format!("Lua error: {e}"))
        })?;
        with_lua(|lua| {
            if let Ok(init_fn) = lua.globals().get::<Function>("init") {
                init_fn.call::<()>(()).ok();
            }
            Ok(())
        })?;
        self.initialized = true;
        info!("FirstRunScene: wizard loaded");
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, _ctx: &mut SceneContext, dt: f64) -> SceneResult {
        with_lua(|lua| {
            if let Ok(update_fn) = lua.globals().get::<Function>("update") {
                update_fn.call::<()>(dt).map_err(|e| format!("Lua: {e}"))?;
            }
            Ok(())
        })?;
        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        with_lua(|lua| {
            if let Ok(render_fn) = lua.globals().get::<Function>("render") {
                render_fn.call::<()>(()).map_err(|e| format!("Lua: {e}"))?;
            }
            Ok(())
        })?;
        Ok(SceneAction::Continue)
    }
}
