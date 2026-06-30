use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::{Lua, Table};
use vibege_renderer::Renderer;

/// Stores user-loaded texture handles keyed by name for the Lua SDK.
/// This is separate from the asset system's cache — it's for textures
/// loaded directly from Lua via `vibege.render.load_texture()`.
struct SdkTextureCache {
    textures: HashMap<String, (usize, u32, u32)>,
}

pub fn register_render_api(lua: &Lua, renderer: &Arc<Renderer>) -> Result<Table, String> {
    let r = lua.create_table().map_err(|e| e.to_string())?;
    let tex_cache = Arc::new(Mutex::new(SdkTextureCache {
        textures: HashMap::new(),
    }));

    // ── Clear ──
    let ren = Arc::clone(renderer);
    let clear_fn = lua
        .create_function(move |_, (r, g, b, a): (f32, f32, f32, f32)| {
            ren.set_clear(r, g, b, a);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    r.set("clear", clear_fn).map_err(|e| e.to_string())?;

    // ── Rectangle ──
    let ren = Arc::clone(renderer);
    let rect_fn = lua
        .create_function(
            move |_, (x, y, w, h, r, g, b, a): (f32, f32, f32, f32, f32, f32, f32, f32)| {
                ren.draw_rect(x, y, w, h, r, g, b, a);
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
    r.set("draw_rect", rect_fn).map_err(|e| e.to_string())?;

    // ── Text ──
    let ren = Arc::clone(renderer);
    let text_fn = lua
        .create_function(
            move |_, (x, y, text, cw, r, g, b): (f32, f32, String, f32, f32, f32, f32)| {
                ren.draw_text(x, y, &text, cw, r, g, b);
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
    r.set("draw_text", text_fn).map_err(|e| e.to_string())?;

    // ── Measure text ──
    let measure_fn = lua
        .create_function(|_, (text, char_w): (String, f32)| {
            let w = text.len() as f32 * char_w;
            let h = char_w;
            Ok((w, h))
        })
        .map_err(|e| e.to_string())?;
    r.set("measure_text", measure_fn)
        .map_err(|e| e.to_string())?;

    // ── Load texture ──
    let ren = Arc::clone(renderer);
    let cache = Arc::clone(&tex_cache);
    let load_tex_fn = lua
        .create_function(move |_, (key, data): (String, mlua::String)| {
            let bytes = data.as_bytes().to_vec();
            let tex = ren
                .load_texture_asset(&bytes)
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            let tex_idx = tex.bind_group_index;
            let mut c = cache
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            c.textures.insert(key, (tex_idx, tex.width, tex.height));
            Ok((tex.width as f64, tex.height as f64))
        })
        .map_err(|e| e.to_string())?;
    r.set("load_texture", load_tex_fn)
        .map_err(|e| e.to_string())?;

    // ── Unload texture ──
    let ren = Arc::clone(renderer);
    let cache = Arc::clone(&tex_cache);
    let unload_tex_fn = lua
        .create_function(move |_, key: String| {
            let mut c = cache
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            if let Some((idx, _, _)) = c.textures.remove(&key) {
                ren.unload_texture_slot(idx);
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    r.set("unload_texture", unload_tex_fn)
        .map_err(|e| e.to_string())?;

    // ── Has texture ──
    let cache = Arc::clone(&tex_cache);
    let has_tex_fn = lua
        .create_function(move |_, key: String| {
            let c = cache
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(c.textures.contains_key(&key))
        })
        .map_err(|e| e.to_string())?;
    r.set("has_texture", has_tex_fn)
        .map_err(|e| e.to_string())?;

    // ── Draw sprite (full texture) ──
    let ren = Arc::clone(renderer);
    let cache = Arc::clone(&tex_cache);
    let sprite_fn = lua
        .create_function(
            move |_, (tex_key, x, y, w, h): (String, f32, f32, f32, f32)| {
                let c = cache
                    .lock()
                    .map_err(|e| mlua::Error::external(e.to_string()))?;
                if let Some((idx, _, _)) = c.textures.get(&tex_key) {
                    ren.draw_sprite(*idx, x, y, w, h);
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
        )
        .map_err(|e| e.to_string())?;
    r.set("draw_sprite", sprite_fn).map_err(|e| e.to_string())?;

    // ── Draw sub-texture (sprite sheet region with tint) ──
    let ren = Arc::clone(renderer);
    let cache = Arc::clone(&tex_cache);
    #[allow(clippy::type_complexity)]
    let subtex_fn = lua
        .create_function(
            move |_,
                  (tex_key, sx, sy, sw, sh, u1, v1, u2, v2, tr, tg, tb, ta): (
                String,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
            )| {
                let c = cache
                    .lock()
                    .map_err(|e| mlua::Error::external(e.to_string()))?;
                if let Some((idx, _, _)) = c.textures.get(&tex_key) {
                    ren.draw_sprite_subtex(*idx, sx, sy, sw, sh, u1, v1, u2, v2, tr, tg, tb, ta);
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
        )
        .map_err(|e| e.to_string())?;
    r.set("draw_subtexture", subtex_fn)
        .map_err(|e| e.to_string())?;

    // ── Draw tinted sprite ──
    let ren = Arc::clone(renderer);
    let cache = Arc::clone(&tex_cache);
    let tinted_fn = lua
        .create_function(
            move |_,
                  (tex_key, x, y, w, h, tr, tg, tb, ta): (
                String,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
                f32,
            )| {
                let c = cache
                    .lock()
                    .map_err(|e| mlua::Error::external(e.to_string()))?;
                if let Some((idx, _, _)) = c.textures.get(&tex_key) {
                    ren.draw_sprite_subtex(*idx, x, y, w, h, 0.0, 0.0, 1.0, 1.0, tr, tg, tb, ta);
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
        )
        .map_err(|e| e.to_string())?;
    r.set("draw_tinted", tinted_fn).map_err(|e| e.to_string())?;

    Ok(r)
}
