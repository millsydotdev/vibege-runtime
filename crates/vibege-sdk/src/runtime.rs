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
    let runtime_table = lua.create_table().map_err(|e| e.to_string())?;
    let ver = engine_version.to_string();

    // Engine version
    let v = ver.clone();
    let version_fn = lua
        .create_function(move |_, ()| Ok(v.clone()))
        .map_err(|e| e.to_string())?;
    runtime_table
        .set("engine_version", version_fn)
        .map_err(|e| e.to_string())?;

    // Screen size
    let w = screen_width;
    let h = screen_height;
    let screen_fn = lua
        .create_function(move |lua, _: ()| {
            let tbl = lua.create_table()?;
            tbl.set("width", w)?;
            tbl.set("height", h)?;
            Ok(tbl)
        })
        .map_err(|e| e.to_string())?;
    runtime_table
        .set("screen_size", screen_fn)
        .map_err(|e| e.to_string())?;

    // Platform
    let platform_fn = lua
        .create_function(|_, ()| Ok(std::env::consts::OS.to_string()))
        .map_err(|e| e.to_string())?;
    runtime_table
        .set("platform", platform_fn)
        .map_err(|e| e.to_string())?;

    // Delta time (seconds since last frame)
    let dt_state = Arc::clone(sdk_state);
    let dt_fn = lua
        .create_function(move |_, ()| {
            let s = dt_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.delta_time_secs)
        })
        .map_err(|e| e.to_string())?;
    runtime_table
        .set("delta_time", dt_fn)
        .map_err(|e| e.to_string())?;

    // Frame count (total frames rendered)
    let fc_state = Arc::clone(sdk_state);
    let fc_fn = lua
        .create_function(move |_, ()| {
            let s = fc_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.frame_count)
        })
        .map_err(|e| e.to_string())?;
    runtime_table
        .set("frame_count", fc_fn)
        .map_err(|e| e.to_string())?;

    // Game time (total elapsed seconds)
    let gt_state = Arc::clone(sdk_state);
    let gt_fn = lua
        .create_function(move |_, ()| {
            let s = gt_state
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(s.game_time_secs)
        })
        .map_err(|e| e.to_string())?;
    runtime_table
        .set("game_time", gt_fn)
        .map_err(|e| e.to_string())?;

    Ok(runtime_table)
}
