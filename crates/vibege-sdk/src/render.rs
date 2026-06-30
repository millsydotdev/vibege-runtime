use std::sync::Arc;

use mlua::{Lua, Table};
use vibege_renderer::Renderer;

pub fn register_render_api(lua: &Lua, renderer: &Arc<Renderer>) -> Result<Table, String> {
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
    render_table
        .set("draw_rect", dr)
        .map_err(|e| e.to_string())?;

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

    Ok(render_table)
}
