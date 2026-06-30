use std::sync::{Arc, Mutex};

use mlua::{Lua, Table};

use crate::SdkState;

pub fn register_scene_api(lua: &Lua, sdk_state: &Arc<Mutex<SdkState>>) -> Result<Table, String> {
    let s = lua.create_table().map_err(|e| e.to_string())?;

    // ── screen_size() → {width, height} ──
    let st = Arc::clone(sdk_state);
    let screen_fn = lua
        .create_function(move |lua, _: ()| {
            let state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            let t = lua.create_table()?;
            t.set("width", state.screen_width)?;
            t.set("height", state.screen_height)?;
            Ok(t)
        })
        .map_err(|e| e.to_string())?;
    s.set("screen_size", screen_fn).map_err(|e| e.to_string())?;

    // ── camera_position() → x, y ──
    let st = Arc::clone(sdk_state);
    let cam_pos_fn = lua
        .create_function(move |_, ()| {
            let state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok((state.camera_x, state.camera_y))
        })
        .map_err(|e| e.to_string())?;
    s.set("camera_position", cam_pos_fn)
        .map_err(|e| e.to_string())?;

    // ── set_camera_position(x, y) ──
    let st = Arc::clone(sdk_state);
    let set_cam_fn = lua
        .create_function(move |_, (x, y): (f64, f64)| {
            let mut state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            state.camera_x = x;
            state.camera_y = y;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    s.set("set_camera_position", set_cam_fn)
        .map_err(|e| e.to_string())?;

    // ── camera_zoom() → f64 ──
    let st = Arc::clone(sdk_state);
    let zoom_fn = lua
        .create_function(move |_, ()| {
            let state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(state.camera_zoom)
        })
        .map_err(|e| e.to_string())?;
    s.set("camera_zoom", zoom_fn).map_err(|e| e.to_string())?;

    // ── set_camera_zoom(zoom) ──
    let st = Arc::clone(sdk_state);
    let set_zoom_fn = lua
        .create_function(move |_, zoom: f64| {
            let mut state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            state.camera_zoom = zoom.max(0.01);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    s.set("set_camera_zoom", set_zoom_fn)
        .map_err(|e| e.to_string())?;

    // ── world_to_screen(wx, wy) → sx, sy ──
    let st = Arc::clone(sdk_state);
    let w2s_fn = lua
        .create_function(move |_, (wx, wy): (f64, f64)| {
            let state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            let sx = (wx - state.camera_x) * state.camera_zoom + state.screen_width as f64 / 2.0;
            let sy = (wy - state.camera_y) * state.camera_zoom + state.screen_height as f64 / 2.0;
            Ok((sx, sy))
        })
        .map_err(|e| e.to_string())?;
    s.set("world_to_screen", w2s_fn)
        .map_err(|e| e.to_string())?;

    // ── screen_to_world(sx, sy) → wx, wy ──
    let st = Arc::clone(sdk_state);
    let s2w_fn = lua
        .create_function(move |_, (sx, sy): (f64, f64)| {
            let state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            let wx = (sx - state.screen_width as f64 / 2.0) / state.camera_zoom + state.camera_x;
            let wy = (sy - state.screen_height as f64 / 2.0) / state.camera_zoom + state.camera_y;
            Ok((wx, wy))
        })
        .map_err(|e| e.to_string())?;
    s.set("screen_to_world", s2w_fn)
        .map_err(|e| e.to_string())?;

    // ── viewport() → {x, y, width, height} ──
    let st = Arc::clone(sdk_state);
    let viewport_fn = lua
        .create_function(move |lua, _: ()| {
            let state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            let t = lua.create_table()?;
            let hw = state.screen_width as f64 / 2.0 / state.camera_zoom;
            let hh = state.screen_height as f64 / 2.0 / state.camera_zoom;
            t.set("x", state.camera_x - hw)?;
            t.set("y", state.camera_y - hh)?;
            t.set("width", hw * 2.0)?;
            t.set("height", hh * 2.0)?;
            Ok(t)
        })
        .map_err(|e| e.to_string())?;
    s.set("viewport", viewport_fn).map_err(|e| e.to_string())?;

    Ok(s)
}
