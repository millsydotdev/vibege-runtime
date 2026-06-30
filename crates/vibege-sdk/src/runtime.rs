use mlua::{Lua, Table};

use std::sync::Arc;

pub fn register_runtime_api(
    lua: &Lua,
    _event_bus: &Option<Arc<vibege_core::EventBus>>,
    engine_version: &str,
    screen_width: u32,
    screen_height: u32,
) -> Result<Table, String> {
    let runtime_table = lua.create_table().map_err(|e| e.to_string())?;

    // Engine version
    let ver = engine_version.to_string();
    let version_fn = lua
        .create_function(move |_, ()| Ok(ver.clone()))
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

    // Platform info
    let platform_fn = lua
        .create_function(|_, ()| {
            #[cfg(target_os = "windows")]
            {
                Ok("windows".to_string())
            }
            #[cfg(target_os = "linux")]
            {
                Ok("linux".to_string())
            }
            #[cfg(target_os = "macos")]
            {
                Ok("macos".to_string())
            }
        })
        .map_err(|e| e.to_string())?;
    runtime_table
        .set("platform", platform_fn)
        .map_err(|e| e.to_string())?;

    Ok(runtime_table)
}
