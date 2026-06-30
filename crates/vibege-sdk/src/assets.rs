use std::sync::Arc;

use mlua::{Lua, Table};
use vibege_asset::AssetManager;

pub fn register_assets_api(lua: &Lua, assets: &Arc<AssetManager>) -> Result<Table, String> {
    let asset_table = lua.create_table().map_err(|e| e.to_string())?;

    let a = Arc::clone(assets);
    let exists_fn = lua
        .create_function(move |_, key: String| Ok(a.exists(&key)))
        .map_err(|e| e.to_string())?;
    asset_table
        .set("exists", exists_fn)
        .map_err(|e| e.to_string())?;

    let a = Arc::clone(assets);
    let release_fn = lua
        .create_function(move |_, key: String| {
            let found = a.exists(&key);
            if found {
                if a.has_texture(&key) {
                    a.release_texture(&key);
                }
                if a.has_audio(&key) {
                    a.release_audio(&key);
                }
                if a.has_lua_source(&key) {
                    a.release_lua_source(&key);
                }
            }
            Ok(found)
        })
        .map_err(|e| e.to_string())?;
    asset_table
        .set("release", release_fn)
        .map_err(|e| e.to_string())?;

    let a = Arc::clone(assets);
    let stats_fn = lua
        .create_function(move |lua, _: ()| {
            let s = a.statistics();
            let tbl = lua.create_table()?;
            tbl.set("total_assets", s.total_assets)?;
            tbl.set("total_memory_bytes", s.total_memory_bytes)?;
            tbl.set("cache_hit_rate", s.hit_rate())?;
            tbl.set("total_loads", s.total_loads)?;
            tbl.set("total_releases", s.total_releases)?;
            tbl.set("total_failed_loads", s.total_failed_loads)?;
            Ok(tbl)
        })
        .map_err(|e| e.to_string())?;
    asset_table
        .set("statistics", stats_fn)
        .map_err(|e| e.to_string())?;

    let a = Arc::clone(assets);
    let metadata_fn = lua
        .create_function(move |lua, key: String| {
            if !a.exists(&key) {
                return Ok(mlua::Value::Nil);
            }
            let tbl = lua.create_table()?;
            tbl.set("key", key.clone())?;
            if a.has_texture(&key) {
                tbl.set("asset_type", "texture")?;
                if let Some(data) = a.get_texture_data(&key) {
                    tbl.set("width", data.width)?;
                    tbl.set("height", data.height)?;
                }
            } else if a.has_audio(&key) {
                tbl.set("asset_type", "audio")?;
                if let Some(data) = a.get_audio_data(&key) {
                    tbl.set("duration_secs", data.duration_secs)?;
                }
            } else if a.has_lua_source(&key) {
                tbl.set("asset_type", "lua_source")?;
            }
            Ok(mlua::Value::Table(tbl))
        })
        .map_err(|e| e.to_string())?;
    asset_table
        .set("metadata", metadata_fn)
        .map_err(|e| e.to_string())?;

    Ok(asset_table)
}
