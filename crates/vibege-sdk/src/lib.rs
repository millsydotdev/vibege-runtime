//! VibeGE SDK — the official public API for game development.
//!
//! This crate defines the Lua bindings that the runtime exposes to games.
//! Games import the `vibege` table and call its methods. The SDK ensures
//! a consistent, documented API surface across all runtime versions.
//!
//! ## Public API
//!
//! - `vibege.input.is_key_down(key)` — check if a key is held
//! - `vibege.input.is_key_pressed(key)` — check if a key was just pressed
//! - `vibege.render.clear(r, g, b, a)` — set background color
//! - `vibege.render.draw_rect(x, y, w, h, r, g, b, a)` — draw a rectangle
//! - `vibege.render.draw_text(x, y, text, size, r, g, b)` — draw text
//! - `vibege.audio.play_hit()` — play hit sound
//! - `vibege.audio.play_score()` — play score sound
//! - `vibege.audio.play_bounce()` — play bounce sound

use std::sync::Arc;
use std::sync::Mutex;
use mlua::{Lua, Table};
use vibege_audio::AudioSystem;
use vibege_input::InputManager;
use vibege_renderer::Renderer;

/// Register all game API bindings into a Lua VM.
/// Returns the `vibege` table that should be set as a global.
pub fn register_game_api(
    lua: &Lua,
    renderer: &Arc<Renderer>,
    input: &Arc<Mutex<InputManager>>,
    audio: &Option<Arc<AudioSystem>>,
) -> Result<Table, String> {
    let vibege = lua.create_table().map_err(|e| e.to_string())?;

    // ── Input API ──
    let input_table = lua.create_table().map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let is_down = lua
        .create_function(move |_, key: String| {
            Ok(inp
                .lock()
                .expect("Input lock")
                .is_key_down(vibege_input::key_name_to_code(&key)))
        })
        .map_err(|e| e.to_string())?;
    input_table.set("is_key_down", is_down).map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let is_pr = lua
        .create_function(move |_, key: String| {
            Ok(inp
                .lock()
                .expect("Input lock")
                .is_key_pressed(vibege_input::key_name_to_code(&key)))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("is_key_pressed", is_pr)
        .map_err(|e| e.to_string())?;

    vibege.set("input", input_table).map_err(|e| e.to_string())?;

    // ── Render API ──
    let render_table = lua.create_table().map_err(|e| e.to_string())?;

    let ren = Arc::clone(renderer);
    let dr = lua
        .create_function(
            move |_, (x, y, w, h, r, g, b, a): (f32, f32, f32, f32, f32, f32, f32, f32)| {
                ren.draw_rect(x, y, w, h, r, g, b, a);
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
    render_table.set("draw_rect", dr).map_err(|e| e.to_string())?;

    let ren = Arc::clone(renderer);
    let clr = lua
        .create_function(move |_, (r, g, b, a): (f32, f32, f32, f32)| {
            ren.set_clear(r, g, b, a);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    render_table.set("clear", clr).map_err(|e| e.to_string())?;

    let ren = Arc::clone(renderer);
    let dt = lua
        .create_function(
            move |_, (x, y, text, cw, r, g, b): (f32, f32, String, f32, f32, f32, f32)| {
                ren.draw_text(x, y, &text, cw, r, g, b);
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
    render_table
        .set("draw_text", dt)
        .map_err(|e| e.to_string())?;

    vibege.set("render", render_table).map_err(|e| e.to_string())?;

    // ── Audio API ──
    if let Some(sys) = audio {
        let audio_table = lua.create_table().map_err(|e| e.to_string())?;
        let hit = Arc::new(vibege_audio::generate_test_tone(220.0, 0.08));
        let score = Arc::new(vibege_audio::generate_test_tone(440.0, 0.15));
        let bounce = Arc::new(vibege_audio::generate_test_tone(330.0, 0.05));

        let s = Arc::clone(sys);
        let h = Arc::clone(&hit);
        audio_table
            .set(
                "play_hit",
                lua.create_function(move |_, ()| {
                    s.play_sfx(&h);
                    Ok(())
                })
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;

        let s2 = Arc::clone(sys);
        let sc = Arc::clone(&score);
        audio_table
            .set(
                "play_score",
                lua.create_function(move |_, ()| {
                    s2.play_sfx(&sc);
                    Ok(())
                })
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;

        let s3 = Arc::clone(sys);
        let b = Arc::clone(&bounce);
        audio_table
            .set(
                "play_bounce",
                lua.create_function(move |_, ()| {
                    s3.play_sfx(&b);
                    Ok(())
                })
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;

        vibege.set("audio", audio_table).map_err(|e| e.to_string())?;
    }

    Ok(vibege)
}


