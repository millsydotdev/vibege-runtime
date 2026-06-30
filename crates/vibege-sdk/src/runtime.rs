use std::sync::{Arc, Mutex};

use mlua::{Lua, Table};

use crate::SdkState;

pub fn register_runtime_api(
    lua: &Lua,
    _event_bus: &Option<Arc<vibege_core::EventBus>>,
    engine_version: &str,
    screen_width: u32,
    screen_height: u32,
    sdk_state: &Arc<Mutex<SdkState>>,
) -> Result<Table, String> {
    let rt = lua.create_table().map_err(|e| e.to_string())?;

    // ── Static information (captured at init) ──
    let ver = engine_version.to_string();
    let w = screen_width;
    let h = screen_height;

    let version_fn = lua
        .create_function(move |_, ()| Ok(ver.clone()))
        .map_err(|e| e.to_string())?;
    rt.set("engine_version", version_fn)
        .map_err(|e| e.to_string())?;

    let screen_fn = lua
        .create_function(move |lua, _: ()| {
            let t = lua.create_table()?;
            t.set("width", w)?;
            t.set("height", h)?;
            Ok(t)
        })
        .map_err(|e| e.to_string())?;
    rt.set("screen_size", screen_fn)
        .map_err(|e| e.to_string())?;

    let platform_fn = lua
        .create_function(|_, ()| Ok(std::env::consts::OS.to_string()))
        .map_err(|e| e.to_string())?;
    rt.set("platform", platform_fn).map_err(|e| e.to_string())?;

    let arch_fn = lua
        .create_function(|_, ()| Ok(std::env::consts::ARCH.to_string()))
        .map_err(|e| e.to_string())?;
    rt.set("architecture", arch_fn).map_err(|e| e.to_string())?;

    let build_fn = lua
        .create_function(|_, ()| Ok(env!("CARGO_PKG_VERSION").to_string()))
        .map_err(|e| e.to_string())?;
    rt.set("build_version", build_fn)
        .map_err(|e| e.to_string())?;

    // ── Frame timing (from SdkState) ──
    let dt_state = Arc::clone(sdk_state);
    let dt_fn = lua
        .create_function(move |_, ()| {
            let s = dt_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.delta_time_secs)
        })
        .map_err(|e| e.to_string())?;
    rt.set("delta_time", dt_fn).map_err(|e| e.to_string())?;

    let fc_state = Arc::clone(sdk_state);
    let fc_fn = lua
        .create_function(move |_, ()| {
            let s = fc_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.frame_count)
        })
        .map_err(|e| e.to_string())?;
    rt.set("frame_count", fc_fn).map_err(|e| e.to_string())?;

    let gt_state = Arc::clone(sdk_state);
    let gt_fn = lua
        .create_function(move |_, ()| {
            let s = gt_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.game_time_secs)
        })
        .map_err(|e| e.to_string())?;
    rt.set("game_time", gt_fn).map_err(|e| e.to_string())?;

    let fps_state = Arc::clone(sdk_state);
    let fps_fn = lua
        .create_function(move |_, ()| {
            let s = fps_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.fps)
        })
        .map_err(|e| e.to_string())?;
    rt.set("fps", fps_fn).map_err(|e| e.to_string())?;

    let uptime_state = Arc::clone(sdk_state);
    let uptime_fn = lua
        .create_function(move |_, ()| {
            let s = uptime_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.uptime_secs)
        })
        .map_err(|e| e.to_string())?;
    rt.set("uptime", uptime_fn).map_err(|e| e.to_string())?;

    // ── State control ──
    let pause_state = Arc::clone(sdk_state);
    let pause_fn = lua
        .create_function(move |_, paused: bool| {
            let mut s = pause_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            s.paused = paused;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    rt.set("set_paused", pause_fn).map_err(|e| e.to_string())?;

    let is_paused_state = Arc::clone(sdk_state);
    let is_paused_fn = lua
        .create_function(move |_, ()| {
            let s = is_paused_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.paused)
        })
        .map_err(|e| e.to_string())?;
    rt.set("is_paused", is_paused_fn)
        .map_err(|e| e.to_string())?;

    let debug_state = Arc::clone(sdk_state);
    let debug_fn = lua
        .create_function(move |_, enabled: bool| {
            let mut s = debug_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            s.debug_mode = enabled;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    rt.set("set_debug", debug_fn).map_err(|e| e.to_string())?;

    let is_debug_state = Arc::clone(sdk_state);
    let is_debug_fn = lua
        .create_function(move |_, ()| {
            let s = is_debug_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.debug_mode)
        })
        .map_err(|e| e.to_string())?;
    rt.set("is_debug", is_debug_fn).map_err(|e| e.to_string())?;

    Ok(rt)
}
