//! VibeGE SDK — the official public API for game development.
//!
//! This crate defines the Lua bindings that the runtime exposes to games.
//! The SDK is split into modules for clean separation of concerns.
//!
//! ## API Modules
//!
//! - `vibege.input.*` — Keyboard, mouse, and gamepad input
//! - `vibege.render.*` — Drawing and screen control
//! - `vibege.audio.*` — Sound playback and mixing
//! - `vibege.assets.*` — Asset query and release
//! - `vibege.storage.*` — Per-game key-value storage
//! - `vibege.runtime.*` — Engine version, frame timing, screen info, platform
//! - `vibege.math.*` — Vec2, Rect, Color, math utilities
//! - `vibege.debug.*` — Runtime debugging, statistics, overlay diagnostics
//! - `vibege.util.*` — Logging, randomness

pub mod animation;
pub mod assets;
pub mod audio;
pub mod debug;
pub mod input;
pub mod math;
pub mod render;
pub mod runtime;
pub mod save;
pub mod scene;
pub mod storage;
pub mod util;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mlua::{Lua, Table};
use vibege_asset::AssetManager;
use vibege_audio::AudioSystem;
use vibege_core::EventBus;
use vibege_input::InputManager;
use vibege_renderer::Renderer;

pub use storage::GameStorage;

/// A single active tween entry for the animation engine.
#[derive(Debug, Clone)]
pub struct TweenEntry {
    pub id: u64,
    pub remaining: f64,
    pub duration: f64,
    pub from: f64,
    pub to: f64,
    pub value: f64,
    pub done: bool,
    pub easing: u8,
    pub on_complete: Option<String>,
}

/// Shared runtime state accessible from Lua APIs.
pub struct SdkState {
    pub delta_time_secs: f64,
    pub game_time_secs: f64,
    pub frame_count: u64,
    pub fps: f64,
    pub uptime_secs: f64,
    pub paused: bool,
    pub debug_mode: bool,
    start_time: Instant,
    fps_frame_count: u64,
    fps_timer: Instant,
    pub screen_width: u32,
    pub screen_height: u32,
    pub engine_version: String,
    // Camera state
    pub camera_x: f64,
    pub camera_y: f64,
    pub camera_zoom: f64,
    // Animation state
    pub tweens: Vec<TweenEntry>,
    next_tween_id: u64,
}

impl SdkState {
    pub fn new(engine_version: &str, screen_width: u32, screen_height: u32) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            delta_time_secs: 0.0,
            game_time_secs: 0.0,
            frame_count: 0,
            fps: 0.0,
            uptime_secs: 0.0,
            paused: false,
            debug_mode: false,
            start_time: Instant::now(),
            fps_frame_count: 0,
            fps_timer: Instant::now(),
            screen_width,
            screen_height,
            engine_version: engine_version.to_string(),
            camera_x: 0.0,
            camera_y: 0.0,
            camera_zoom: 1.0,
            tweens: Vec::new(),
            next_tween_id: 1,
        }))
    }

    /// Called by the engine each frame to update timing state.
    pub fn tick(state: &Arc<Mutex<Self>>, dt: f64) {
        if let Ok(mut s) = state.lock() {
            s.delta_time_secs = dt;
            if !s.paused {
                s.game_time_secs += dt;
            }
            s.uptime_secs = s.start_time.elapsed().as_secs_f64();
            s.frame_count = s.frame_count.wrapping_add(1);
            s.fps_frame_count += 1;
            if s.fps_timer.elapsed().as_secs_f64() >= 0.5 {
                s.fps = s.fps_frame_count as f64 / s.fps_timer.elapsed().as_secs_f64();
                s.fps_frame_count = 0;
                s.fps_timer = Instant::now();
            }
            // Update active tweens
            s.tweens.retain_mut(|t| !t.done);
            for t in &mut s.tweens {
                t.remaining -= dt;
                if t.remaining <= 0.0 {
                    t.value = t.to;
                    t.done = true;
                } else {
                    let p = 1.0 - t.remaining / t.duration;
                    t.value = t.from + (t.to - t.from) * ease(p, t.easing);
                }
            }
        }
    }
}

fn ease(t: f64, kind: u8) -> f64 {
    match kind {
        0 => t,             // linear
        1 => t * t,         // quad in
        2 => t * (2.0 - t), // quad out
        3 => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        } // quad in-out
        4 => t * t * t,     // cubic in
        5 => {
            let t = t - 1.0;
            t * t * t + 1.0
        } // cubic out
        _ => t,
    }
}

/// Convert a Lua API registration error to a String.
pub(crate) fn lua_err(e: mlua::Error) -> String {
    e.to_string()
}

/// Register all game API bindings into a Lua VM.
/// Returns the `vibege` table that should be set as a global.
#[allow(clippy::too_many_arguments)]
pub fn register_game_api(
    lua: &Lua,
    renderer: &Arc<Renderer>,
    input: &Arc<Mutex<InputManager>>,
    audio: &Option<Arc<AudioSystem>>,
    assets: &Arc<AssetManager>,
    event_bus: &Option<Arc<EventBus>>,
    screen_width: u32,
    screen_height: u32,
    engine_version: &str,
    sdk_state: &Arc<Mutex<SdkState>>,
    game_name: &str,
) -> Result<Table, String> {
    let vibege = lua.create_table().map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let input_table = input::register_input_api(lua, &inp)?;
    vibege.set("input", input_table).map_err(lua_err)?;

    let ren = Arc::clone(renderer);
    let render_table = render::register_render_api(lua, &ren)?;
    vibege.set("render", render_table).map_err(lua_err)?;

    if let Some(audio_table) = audio::register_audio_api(lua, audio)? {
        vibege.set("audio", audio_table).map_err(lua_err)?;
    }

    let ass = Arc::clone(assets);
    let asset_table = assets::register_assets_api(lua, &ass)?;
    vibege.set("assets", asset_table).map_err(lua_err)?;

    let game_storage: &'static GameStorage = Box::leak(Box::new(GameStorage::new()));
    let storage_table = storage::register_storage_api(lua, game_storage)?;
    vibege.set("storage", storage_table).map_err(lua_err)?;

    let rt_state = Arc::clone(sdk_state);
    let runtime_table = runtime::register_runtime_api(
        lua,
        event_bus,
        engine_version,
        screen_width,
        screen_height,
        &rt_state,
    )?;
    vibege.set("runtime", runtime_table).map_err(lua_err)?;

    let math_table = math::register_math_api(lua)?;
    vibege.set("math", math_table).map_err(lua_err)?;

    let sc_state = Arc::clone(sdk_state);
    let scene_table = scene::register_scene_api(lua, &sc_state)?;
    vibege.set("scene", scene_table).map_err(lua_err)?;

    let an_state = Arc::clone(sdk_state);
    let anim_table = animation::register_animation_api(lua, &an_state)?;
    vibege.set("animation", anim_table).map_err(lua_err)?;

    let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let save_table = save::register_save_api(lua, base_dir, game_name)?;
    vibege.set("save", save_table).map_err(lua_err)?;

    let dbg_state = Arc::clone(sdk_state);
    let dbg_renderer = Arc::clone(renderer);
    let debug_table = debug::register_debug_api(lua, &dbg_state, &dbg_renderer, assets)?;
    vibege.set("debug", debug_table).map_err(lua_err)?;

    let ut_state = Arc::clone(sdk_state);
    let util_table = util::register_util_api(lua, &ut_state)?;
    vibege.set("util", util_table).map_err(lua_err)?;

    Ok(vibege)
}
