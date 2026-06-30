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
//! - `vibege.runtime.*` — Engine version, screen info, platform
//! - `vibege.util.*` — Logging, math utilities

pub mod assets;
pub mod audio;
pub mod input;
pub mod render;
pub mod runtime;
pub mod storage;
pub mod util;

use std::sync::Arc;
use std::sync::Mutex;

use mlua::{Lua, Table};
use vibege_asset::AssetManager;
use vibege_audio::AudioSystem;
use vibege_core::EventBus;
use vibege_input::InputManager;
use vibege_renderer::Renderer;

pub use storage::GameStorage;

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
) -> Result<Table, String> {
    let vibege = lua.create_table().map_err(|e| e.to_string())?;

    // ── Input API ──
    let input_table = input::register_input_api(lua, input)?;
    vibege
        .set("input", input_table)
        .map_err(|e| e.to_string())?;

    // ── Render API ──
    let render_table = render::register_render_api(lua, renderer)?;
    vibege
        .set("render", render_table)
        .map_err(|e| e.to_string())?;

    // ── Audio API ──
    if let Some(audio_table) = audio::register_audio_api(lua, audio)? {
        vibege
            .set("audio", audio_table)
            .map_err(|e| e.to_string())?;
    }

    // ── Asset API ──
    let asset_table = assets::register_assets_api(lua, assets)?;
    vibege
        .set("assets", asset_table)
        .map_err(|e| e.to_string())?;

    // ── Storage API ──
    let game_storage: &'static GameStorage = Box::leak(Box::new(GameStorage::new()));
    let storage_table = storage::register_storage_api(lua, game_storage)?;
    vibege
        .set("storage", storage_table)
        .map_err(|e| e.to_string())?;

    // ── Runtime API ──
    let runtime_table =
        runtime::register_runtime_api(lua, event_bus, engine_version, screen_width, screen_height)?;
    vibege
        .set("runtime", runtime_table)
        .map_err(|e| e.to_string())?;

    // ── Utility API ──
    let util_table = util::register_util_api(lua)?;
    vibege.set("util", util_table).map_err(|e| e.to_string())?;

    Ok(vibege)
}
