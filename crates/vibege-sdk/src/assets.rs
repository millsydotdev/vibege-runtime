use std::sync::Arc;

use mlua::{Lua, Table};
use vibege_asset::AssetManager;

pub fn register_assets_api(lua: &Lua, assets: &Arc<AssetManager>) -> Result<Table, String> {
    let a = lua.create_table().map_err(|e| e.to_string())?;

    // ── exists(key) → bool ──
    let ass = Arc::clone(assets);
    let exists_fn = lua
        .create_function(move |_, key: String| Ok(ass.exists(&key)))
        .map_err(|e| e.to_string())?;
    a.set("exists", exists_fn).map_err(|e| e.to_string())?;

    // ── is_loaded(key) → bool ──
    let ass = Arc::clone(assets);
    let loaded_fn = lua
        .create_function(move |_, key: String| Ok(ass.exists(&key)))
        .map_err(|e| e.to_string())?;
    a.set("is_loaded", loaded_fn).map_err(|e| e.to_string())?;

    // ── metadata(key) → table or nil ──
    let ass = Arc::clone(assets);
    let meta_fn = lua
        .create_function(move |lua, key: String| {
            if !ass.exists(&key) {
                return Ok(mlua::Value::Nil);
            }
            let t = lua.create_table()?;
            t.set("key", key.clone())?;
            if ass.has_texture(&key) {
                t.set("asset_type", "texture")?;
                if let Some(data) = ass.get_texture_data(&key) {
                    t.set("width", data.width)?;
                    t.set("height", data.height)?;
                }
            } else if ass.has_audio(&key) {
                t.set("asset_type", "audio")?;
                if let Some(data) = ass.get_audio_data(&key) {
                    t.set("duration_secs", data.duration_secs)?;
                }
            } else if ass.has_lua_source(&key) {
                t.set("asset_type", "lua_source")?;
            } else if ass.has_package(&key) {
                t.set("asset_type", "package")?;
            } else {
                t.set("asset_type", "raw")?;
            }
            let all = ass.all_metadata();
            let m = all.iter().find(|m| m.key == key);
            if let Some(m) = m {
                t.set("size_bytes", m.size_bytes)?;
                t.set("source", format!("{:?}", m.source))?;
            }
            Ok(mlua::Value::Table(t))
        })
        .map_err(|e| e.to_string())?;
    a.set("metadata", meta_fn).map_err(|e| e.to_string())?;

    // ── size(key) → number ──
    let ass = Arc::clone(assets);
    let size_fn = lua
        .create_function(move |_, key: String| {
            let all = ass.all_metadata();
            let m = all.iter().find(|m| m.key == key);
            Ok(m.map(|m| m.size_bytes as f64).unwrap_or(0.0))
        })
        .map_err(|e| e.to_string())?;
    a.set("size", size_fn).map_err(|e| e.to_string())?;

    // ── asset_type(key) → string or nil ──
    let ass = Arc::clone(assets);
    let type_fn = lua
        .create_function(move |_, key: String| {
            if !ass.exists(&key) {
                return Ok(None);
            }
            let t = if ass.has_texture(&key) {
                "texture"
            } else if ass.has_audio(&key) {
                "audio"
            } else if ass.has_lua_source(&key) {
                "lua_source"
            } else if ass.has_package(&key) {
                "package"
            } else {
                "raw"
            };
            Ok(Some(t.to_string()))
        })
        .map_err(|e| e.to_string())?;
    a.set("asset_type", type_fn).map_err(|e| e.to_string())?;

    // ── release(key) → bool ──
    let ass = Arc::clone(assets);
    let release_fn = lua
        .create_function(move |_, key: String| {
            let found = ass.exists(&key);
            if found {
                if ass.has_texture(&key) {
                    ass.release_texture(&key);
                } else if ass.has_audio(&key) {
                    ass.release_audio(&key);
                } else if ass.has_lua_source(&key) {
                    ass.release_lua_source(&key);
                }
            }
            Ok(found)
        })
        .map_err(|e| e.to_string())?;
    a.set("release", release_fn).map_err(|e| e.to_string())?;

    // ── unload(key) → bool (alias for release) ──
    let ass = Arc::clone(assets);
    let unload_fn = lua
        .create_function(move |_, key: String| {
            let found = ass.exists(&key);
            if found {
                if ass.has_texture(&key) {
                    ass.release_texture(&key);
                } else if ass.has_audio(&key) {
                    ass.release_audio(&key);
                } else if ass.has_lua_source(&key) {
                    ass.release_lua_source(&key);
                }
            }
            Ok(found)
        })
        .map_err(|e| e.to_string())?;
    a.set("unload", unload_fn).map_err(|e| e.to_string())?;

    // ── enumerate() → table of keys ──
    let ass = Arc::clone(assets);
    let enum_fn = lua
        .create_function(move |lua, _: ()| {
            let all = ass.all_metadata();
            let t = lua.create_table()?;
            for (i, m) in all.iter().enumerate() {
                t.set(i + 1, m.key.clone())?;
            }
            Ok(t)
        })
        .map_err(|e| e.to_string())?;
    a.set("enumerate", enum_fn).map_err(|e| e.to_string())?;

    // ── memory_usage() → number ──
    let ass = Arc::clone(assets);
    let mem_fn = lua
        .create_function(move |_, ()| {
            let s = ass.statistics();
            Ok(s.total_memory_bytes as f64)
        })
        .map_err(|e| e.to_string())?;
    a.set("memory_usage", mem_fn).map_err(|e| e.to_string())?;

    // ── statistics() → table ──
    let ass = Arc::clone(assets);
    let stats_fn = lua
        .create_function(move |lua, _: ()| {
            let s = ass.statistics();
            let t = lua.create_table()?;
            t.set("total_assets", s.total_assets)?;
            t.set("total_memory_bytes", s.total_memory_bytes)?;
            t.set("cache_hit_rate", s.hit_rate())?;
            t.set("total_loads", s.total_loads)?;
            t.set("total_releases", s.total_releases)?;
            t.set("total_failed_loads", s.total_failed_loads)?;
            Ok(t)
        })
        .map_err(|e| e.to_string())?;
    a.set("statistics", stats_fn).map_err(|e| e.to_string())?;

    Ok(a)
}
