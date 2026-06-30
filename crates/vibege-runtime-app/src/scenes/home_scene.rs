//! HomeScene — temporary wrapper around the existing launcher.lua.
//!
//! During migration, this scene runs the Lua launcher in the platform VM.
//! Once native scenes replace Lua UI, this scene will be removed.

use mlua::{Function, Lua};
use tracing::info;
use crate::scene::{Scene, SceneId, SceneContext, SceneAction, SceneResult, PLATFORM_LUA, LuaPtr};

const LAUNCHER_SOURCE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/launcher.lua"));

/// Access the platform Lua VM (main thread only).
fn with_lua<F, T>(f: F) -> Result<T, String>
where F: FnOnce(&Lua) -> Result<T, String>
{
    let LuaPtr(ptr) = PLATFORM_LUA.get().ok_or("Platform Lua not initialized")?;
    let lua: &Lua = unsafe { &**ptr };
    f(lua)
}

pub struct HomeScene {
    initialized: bool,
}

impl HomeScene {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl Scene for HomeScene {
    fn id(&self) -> SceneId { SceneId::Home }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        with_lua(|lua| {
            lua.load(LAUNCHER_SOURCE).exec().map_err(|e| format!("Lua error: {e}"))
        })?;
        with_lua(|lua| {
            if let Ok(init_fn) = lua.globals().get::<Function>("init") {
                init_fn.call::<()>(()).ok();
            }
            Ok(())
        })?;
        self.initialized = true;
        info!("HomeScene: launcher loaded");
        Ok(SceneAction::Continue)
    }

    fn on_enter(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        with_lua(|lua| {
            if let Ok(init_fn) = lua.globals().get::<Function>("init") {
                let _ = init_fn.call::<()>(());
            }
            Ok(())
        })?;
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
