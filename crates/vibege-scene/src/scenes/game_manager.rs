use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use mlua::{Function, Lua};
use tracing::{info, warn};
use vibege_asset::AssetManager;
use vibege_audio::AudioSystem;
use vibege_core::{EventBus, RuntimeEvent};
use vibege_input::InputManager;
use vibege_renderer::Renderer;
use vibege_sdk::SdkState;

const LUA_TIMEOUT: Duration = Duration::from_millis(1000);

/// A live game session with its own isolated Lua VM.
pub struct GameSession {
    lua: Lua,
    has_update: bool,
    has_render: bool,
    game_name: String,
    event_bus: Option<Arc<EventBus>>,
    sdk_state: Arc<Mutex<SdkState>>,
}

impl Drop for GameSession {
    fn drop(&mut self) {
        if let Some(ref bus) = self.event_bus {
            bus.publish(&RuntimeEvent::GameExited {
                name: self.game_name.clone(),
            });
        }
    }
}

impl GameSession {
    pub fn load(
        game_name: &str,
        source: &str,
        renderer: &Arc<Renderer>,
        input: &Arc<Mutex<InputManager>>,
        audio: &Option<Arc<AudioSystem>>,
        assets: &Arc<AssetManager>,
        event_bus: Option<Arc<EventBus>>,
        screen_width: u32,
        screen_height: u32,
        engine_version: &str,
        sdk_state: &Arc<Mutex<SdkState>>,
    ) -> Result<Self, String> {
        let lua = Lua::new();
        sandbox_lua(&lua);

        let vibege = vibege_sdk::register_game_api(
            &lua,
            renderer,
            input,
            audio,
            assets,
            &event_bus,
            screen_width,
            screen_height,
            engine_version,
            sdk_state,
            game_name,
        )?;
        lua.globals()
            .set("vibege", vibege)
            .map_err(|e| e.to_string())?;

        lua.load(source)
            .exec()
            .map_err(|e| format!("Lua load error: {e}"))?;

        let has_update = lua.globals().get::<Function>("update").is_ok();
        let has_render = lua.globals().get::<Function>("render").is_ok();

        if let Ok(init_fn) = lua.globals().get::<Function>("init") {
            if let Err(e) = init_fn.call::<()>(()) {
                warn!("Game init() error: {e}");
            }
        }

        if let Some(ref bus) = event_bus {
            bus.publish(&RuntimeEvent::GameStarted {
                name: game_name.to_string(),
            });
        }
        info!("Game session created");
        let eb = event_bus.as_ref().map(Arc::clone);
        Ok(Self {
            lua,
            has_update,
            has_render,
            game_name: game_name.to_string(),
            event_bus: eb,
            sdk_state: Arc::clone(sdk_state),
        })
    }

    /// Execute the game's update function.
    /// NOTE: Lua is not `Send`, so we cannot use thread-based timeouts.
    /// A long-running Lua script (infinite loop) will block the engine.
    /// This will be resolved when games run in sandboxed processes (Wave 21).
    pub fn update(&self, dt: f64) -> Result<(), String> {
        SdkState::tick(&self.sdk_state, dt);
        if self.has_update {
            if let Ok(update_fn) = self.lua.globals().get::<Function>("update") {
                update_fn
                    .call::<()>(dt)
                    .map_err(|e| format!("Game update error: {e}"))?;
            }
        }
        Ok(())
    }

    /// Execute the game's render function.
    /// NOTE: Same threading limitation as update().
    pub fn render(&self) -> Result<(), String> {
        if self.has_render {
            if let Ok(render_fn) = self.lua.globals().get::<Function>("render") {
                render_fn
                    .call::<()>(())
                    .map_err(|e| format!("Game render error: {e}"))?;
            }
        }
        Ok(())
    }

    pub fn suspend(&self) {
        if let Ok(suspend_fn) = self.lua.globals().get::<Function>("suspend") {
            let _ = suspend_fn.call::<()>(());
        }
        if let Some(ref bus) = self.event_bus {
            bus.publish(&RuntimeEvent::GameSuspended {
                name: self.game_name.clone(),
            });
        }
    }

    pub fn resume(&self) {
        if let Ok(resume_fn) = self.lua.globals().get::<Function>("resume") {
            let _ = resume_fn.call::<()>(());
        }
        if let Some(ref bus) = self.event_bus {
            bus.publish(&RuntimeEvent::GameResumed {
                name: self.game_name.clone(),
            });
        }
    }

    pub fn get_state(&self) -> Option<String> {
        if let Ok(state_fn) = self.lua.globals().get::<Function>("get_state") {
            state_fn.call::<String>("").ok()
        } else {
            None
        }
    }
}

/// Remove dangerous global functions from the Lua environment.
fn sandbox_lua(lua: &mlua::Lua) {
    let globals = lua.globals();
    let dangerous = [
        "io", "loadfile", "dofile", "require", "package", "debug",
    ];
    for name in &dangerous {
        globals.set(*name, mlua::Value::Nil).ok();
    }
}
