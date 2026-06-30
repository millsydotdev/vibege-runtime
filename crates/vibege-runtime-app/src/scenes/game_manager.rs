use std::sync::Arc;
use std::sync::Mutex;
use mlua::{Function, Lua};
use tracing::{info, warn};
use vibege_audio::AudioSystem;
use vibege_input::InputManager;
use vibege_renderer::Renderer;

/// Live game session with its own isolated Lua VM.
/// Uses a raw pointer to break the !Send constraint on mlua::Lua.
pub struct GameSession {
    lua: *mut Lua,
    has_update: bool,
    has_render: bool,
}

unsafe impl Send for GameSession {}

impl Drop for GameSession {
    fn drop(&mut self) {
        if !self.lua.is_null() {
            unsafe { drop(Box::from_raw(self.lua)); }
        }
    }
}

impl GameSession {
    pub fn load(
        source: &str,
        renderer: &Arc<Renderer>,
        input: &Arc<Mutex<InputManager>>,
        audio: &Option<Arc<AudioSystem>>,
    ) -> Result<Self, String> {
        let lua = Lua::new();
        let vibege = lua.create_table().map_err(|e| e.to_string())?;

        // Input bindings
        let input_table = lua.create_table().map_err(|e| e.to_string())?;
        {
            let inp = Arc::clone(input);
            let is_down = lua.create_function(move |_, key: String| {
                Ok(inp.lock().unwrap().is_key_down(crate::key_name_to_code(&key)))
            }).map_err(|e| e.to_string())?;
            input_table.set("is_key_down", is_down).map_err(|e| e.to_string())?;
        }
        {
            let inp = Arc::clone(input);
            let is_pr = lua.create_function(move |_, key: String| {
                Ok(inp.lock().unwrap().is_key_pressed(crate::key_name_to_code(&key)))
            }).map_err(|e| e.to_string())?;
            input_table.set("is_key_pressed", is_pr).map_err(|e| e.to_string())?;
        }
        vibege.set("input", input_table).map_err(|e| e.to_string())?;

        // Render bindings
        let render_table = lua.create_table().map_err(|e| e.to_string())?;
        {
            let ren = Arc::clone(renderer);
            let dr = lua.create_function(move |_, (x, y, w, h, r, g, b, a): (f32, f32, f32, f32, f32, f32, f32, f32)| {
                ren.draw_rect(x, y, w, h, r, g, b, a);
                Ok(())
            }).map_err(|e| e.to_string())?;
            render_table.set("draw_rect", dr).map_err(|e| e.to_string())?;
        }
        {
            let ren = Arc::clone(renderer);
            let clr = lua.create_function(move |_, (r, g, b, a): (f32, f32, f32, f32)| {
                ren.set_clear(r, g, b, a);
                Ok(())
            }).map_err(|e| e.to_string())?;
            render_table.set("clear", clr).map_err(|e| e.to_string())?;
        }
        {
            let ren = Arc::clone(renderer);
            let dt = lua.create_function(move |_, (x, y, text, cw, r, g, b): (f32, f32, String, f32, f32, f32, f32)| {
                ren.draw_text(x, y, &text, cw, r, g, b);
                Ok(())
            }).map_err(|e| e.to_string())?;
            render_table.set("draw_text", dt).map_err(|e| e.to_string())?;
        }
        vibege.set("render", render_table).map_err(|e| e.to_string())?;

        // Audio bindings
        if let Some(sys) = audio {
            let audio_table = lua.create_table().map_err(|e| e.to_string())?;
            let hit = Arc::new(vibege_audio::generate_test_tone(220.0, 0.08));
            let score = Arc::new(vibege_audio::generate_test_tone(440.0, 0.15));
            let bounce = Arc::new(vibege_audio::generate_test_tone(330.0, 0.05));

            let s = Arc::clone(sys); let h = Arc::clone(&hit);
            audio_table.set("play_hit", lua.create_function(move |_, ()| { s.play_sfx(&h); Ok(()) }).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;

            let s2 = Arc::clone(sys); let sc = Arc::clone(&score);
            audio_table.set("play_score", lua.create_function(move |_, ()| { s2.play_sfx(&sc); Ok(()) }).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;

            let s3 = Arc::clone(sys); let b = Arc::clone(&bounce);
            audio_table.set("play_bounce", lua.create_function(move |_, ()| { s3.play_sfx(&b); Ok(()) }).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;

            vibege.set("audio", audio_table).map_err(|e| e.to_string())?;
        }

        lua.globals().set("vibege", vibege).map_err(|e| e.to_string())?;

        lua.load(source).exec().map_err(|e| format!("Lua load error: {e}"))?;

        let has_update = lua.globals().get::<Function>("update").is_ok();
        let has_render = lua.globals().get::<Function>("render").is_ok();

        if let Ok(init_fn) = lua.globals().get::<Function>("init") {
            if let Err(e) = init_fn.call::<()>(()) {
                warn!("Game init() error: {e}");
            }
        }

        info!("Game session created");
        let lua_ptr = Box::into_raw(Box::new(lua));
        Ok(Self { lua: lua_ptr, has_update, has_render })
    }

    fn lua(&self) -> &Lua {
        unsafe { &*self.lua }
    }

    pub fn update(&self, dt: f64) -> Result<(), String> {
        if self.has_update {
            let lua = self.lua();
            if let Ok(update_fn) = lua.globals().get::<Function>("update") {
                update_fn.call::<()>(dt).map_err(|e| format!("Game update error: {e}"))?;
            }
        }
        Ok(())
    }

    pub fn render(&self) -> Result<(), String> {
        if self.has_render {
            let lua = self.lua();
            if let Ok(render_fn) = lua.globals().get::<Function>("render") {
                render_fn.call::<()>(()).map_err(|e| format!("Game render error: {e}"))?;
            }
        }
        Ok(())
    }

    pub fn suspend(&self) {
        let lua = self.lua();
        if let Ok(suspend_fn) = lua.globals().get::<Function>("suspend") {
            let _ = suspend_fn.call::<()>(());
        }
    }

    pub fn resume(&self) {
        let lua = self.lua();
        if let Ok(resume_fn) = lua.globals().get::<Function>("resume") {
            let _ = resume_fn.call::<()>(());
        }
    }

    pub fn get_state(&self) -> Option<String> {
        let lua = self.lua();
        if let Ok(state_fn) = lua.globals().get::<Function>("get_state") {
            state_fn.call::<String>("").ok()
        } else {
            None
        }
    }
}
