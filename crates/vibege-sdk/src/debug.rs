use std::sync::{Arc, Mutex};

use mlua::{Lua, Table};
use vibege_asset::AssetManager;

use crate::SdkState;

pub fn register_debug_api(
    lua: &Lua,
    sdk_state: &Arc<Mutex<SdkState>>,
    renderer: &Arc<vibege_renderer::Renderer>,
    assets: &Arc<AssetManager>,
) -> Result<Table, String> {
    let d = lua.create_table().map_err(|e| e.to_string())?;

    // ── Runtime statistics ──
    let st = Arc::clone(sdk_state);
    let stats_fn = lua
        .create_function(move |lua, _: ()| {
            let s = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            let t = lua.create_table()?;
            t.set("fps", s.fps)?;
            t.set("delta_time", s.delta_time_secs)?;
            t.set("frame_count", s.frame_count)?;
            t.set("game_time", s.game_time_secs)?;
            t.set("uptime", s.uptime_secs)?;
            t.set("paused", s.paused)?;
            Ok(t)
        })
        .map_err(|e| e.to_string())?;
    d.set("runtime_stats", stats_fn)
        .map_err(|e| e.to_string())?;

    // ── Asset statistics ──
    let a = Arc::clone(assets);
    let asset_stats_fn = lua
        .create_function(move |lua, _: ()| {
            let stats = a.statistics();
            let t = lua.create_table()?;
            t.set("total_assets", stats.total_assets)?;
            t.set("total_memory_bytes", stats.total_memory_bytes)?;
            t.set("cache_hit_rate", stats.hit_rate())?;
            t.set("total_loads", stats.total_loads)?;
            t.set("total_releases", stats.total_releases)?;
            t.set("total_failed_loads", stats.total_failed_loads)?;
            Ok(t)
        })
        .map_err(|e| e.to_string())?;
    d.set("asset_stats", asset_stats_fn)
        .map_err(|e| e.to_string())?;

    // ── Debug draw: rectangle with outline ──
    let ren = Arc::clone(renderer);
    let draw_rect_fn = lua
        .create_function(
            move |_, (x, y, w, h, r, g, b, a): (f32, f32, f32, f32, f32, f32, f32, f32)| {
                // Fill
                ren.draw_rect(x, y, w, h, r, g, b, a * 0.3);
                // Outline
                ren.draw_rect(x, y, w, 1.0, r, g, b, a);
                ren.draw_rect(x, y + h - 1.0, w, 1.0, r, g, b, a);
                ren.draw_rect(x, y, 1.0, h, r, g, b, a);
                ren.draw_rect(x + w - 1.0, y, 1.0, h, r, g, b, a);
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
    d.set("draw_rect", draw_rect_fn)
        .map_err(|e| e.to_string())?;

    // ── Debug draw: line ──
    let ren = Arc::clone(renderer);
    let draw_line_fn = lua
        .create_function(
            move |_, (x1, y1, x2, y2, r, g, b, a): (f32, f32, f32, f32, f32, f32, f32, f32)| {
                let dx = x2 - x1;
                let dy = y2 - y1;
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1.0 {
                    return Ok(());
                }
                // Draw line as a series of small points
                let mut i = 0u32;
                loop {
                    let t = i as f32 / ((len / 4.0).ceil()).max(1.0);
                    let px = x1 + dx * t;
                    let py = y1 + dy * t;
                    ren.draw_rect(px, py, 3.0, 3.0, r, g, b, a);
                    i += 1;
                    if i >= (len / 4.0).ceil() as u32 {
                        break;
                    }
                }
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
    d.set("draw_line", draw_line_fn)
        .map_err(|e| e.to_string())?;

    // ── Debug draw: circle approximation ──
    let ren = Arc::clone(renderer);
    let draw_circle_fn = lua
        .create_function(
            move |_, (cx, cy, radius, r, g, b, a): (f32, f32, f32, f32, f32, f32, f32)| {
                let segments = (radius * 2.0).ceil() as u32 + 8;
                let two_pi = std::f32::consts::TAU;
                for i in 0..segments {
                    let angle1 = two_pi * i as f32 / segments as f32;
                    let px = cx + angle1.cos() * radius;
                    let py = cy + angle1.sin() * radius;
                    ren.draw_rect(px - 1.0, py - 1.0, 2.0, 2.0, r, g, b, a);
                }
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
    d.set("draw_circle", draw_circle_fn)
        .map_err(|e| e.to_string())?;

    // ── Debug text ──
    let ren = Arc::clone(renderer);
    let draw_text_fn = lua
        .create_function(
            move |_, (x, y, text, r, g, b): (f32, f32, String, f32, f32, f32)| {
                ren.draw_text(x, y, &text, 8.0, r, g, b);
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
    d.set("draw_text", draw_text_fn)
        .map_err(|e| e.to_string())?;

    Ok(d)
}
