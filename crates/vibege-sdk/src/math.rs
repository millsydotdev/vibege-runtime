use mlua::{Lua, Table};

use crate::lua_err;

pub fn register_math_api(lua: &Lua) -> Result<Table, String> {
    let m = lua.create_table().map_err(lua_err)?;

    // ── Vec2 constructor ──
    let vec2_fn = lua
        .create_function(|lua, (x, y): (f64, f64)| {
            let t = lua.create_table()?;
            t.set("x", x)?;
            t.set("y", y)?;
            Ok(t)
        })
        .map_err(lua_err)?;
    m.set("vec2", vec2_fn).map_err(lua_err)?;

    // ── Rect constructor ──
    let rect_fn = lua
        .create_function(|lua, (x, y, w, h): (f64, f64, f64, f64)| {
            let t = lua.create_table()?;
            t.set("x", x)?;
            t.set("y", y)?;
            t.set("width", w)?;
            t.set("height", h)?;
            Ok(t)
        })
        .map_err(lua_err)?;
    m.set("rect", rect_fn).map_err(lua_err)?;

    // ── Color constructor ──
    let color_fn = lua
        .create_function(|lua, (r, g, b, a): (f64, f64, f64, f64)| {
            let t = lua.create_table()?;
            t.set("r", r.clamp(0.0, 1.0))?;
            t.set("g", g.clamp(0.0, 1.0))?;
            t.set("b", b.clamp(0.0, 1.0))?;
            t.set("a", a.clamp(0.0, 1.0))?;
            Ok(t)
        })
        .map_err(lua_err)?;
    m.set("color", color_fn).map_err(lua_err)?;

    // ── Basic math ──
    let clamp_fn = lua
        .create_function(|_, (v, lo, hi): (f64, f64, f64)| Ok(v.clamp(lo, hi)))
        .map_err(lua_err)?;
    m.set("clamp", clamp_fn).map_err(lua_err)?;

    let lerp_fn = lua
        .create_function(|_, (a, b, t): (f64, f64, f64)| Ok(a + (b - a) * t.clamp(0.0, 1.0)))
        .map_err(lua_err)?;
    m.set("lerp", lerp_fn).map_err(lua_err)?;

    let inv_lerp_fn = lua
        .create_function(|_, (a, b, v): (f64, f64, f64)| {
            if (b - a).abs() < 1e-10 {
                Ok(0.0)
            } else {
                Ok((v - a) / (b - a))
            }
        })
        .map_err(lua_err)?;
    m.set("inverse_lerp", inv_lerp_fn).map_err(lua_err)?;

    let remap_fn = lua
        .create_function(
            |_, (v, from_lo, from_hi, to_lo, to_hi): (f64, f64, f64, f64, f64)| {
                let t = if (from_hi - from_lo).abs() < 1e-10 {
                    0.0
                } else {
                    (v - from_lo) / (from_hi - from_lo)
                };
                Ok(to_lo + t * (to_hi - to_lo))
            },
        )
        .map_err(lua_err)?;
    m.set("remap", remap_fn).map_err(lua_err)?;

    let smoothstep_fn = lua
        .create_function(|_, (lo, hi, v): (f64, f64, f64)| {
            let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
            Ok(t * t * (3.0 - 2.0 * t))
        })
        .map_err(lua_err)?;
    m.set("smoothstep", smoothstep_fn).map_err(lua_err)?;

    let sign_fn = lua
        .create_function(|_, v: f64| {
            if v > 0.0 {
                Ok(1.0)
            } else if v < 0.0 {
                Ok(-1.0)
            } else {
                Ok(0.0)
            }
        })
        .map_err(lua_err)?;
    m.set("sign", sign_fn).map_err(lua_err)?;

    // ── Rounding ──
    let round_fn = lua
        .create_function(|_, v: f64| Ok(v.round()))
        .map_err(lua_err)?;
    m.set("round", round_fn).map_err(lua_err)?;
    let floor_fn = lua
        .create_function(|_, v: f64| Ok(v.floor()))
        .map_err(lua_err)?;
    m.set("floor", floor_fn).map_err(lua_err)?;
    let ceil_fn = lua
        .create_function(|_, v: f64| Ok(v.ceil()))
        .map_err(lua_err)?;
    m.set("ceil", ceil_fn).map_err(lua_err)?;
    let abs_fn = lua
        .create_function(|_, v: f64| Ok(v.abs()))
        .map_err(lua_err)?;
    m.set("abs", abs_fn).map_err(lua_err)?;

    // ── Min / Max ──
    let min_fn = lua
        .create_function(|_, (a, b): (f64, f64)| Ok(a.min(b)))
        .map_err(lua_err)?;
    m.set("min", min_fn).map_err(lua_err)?;
    let max_fn = lua
        .create_function(|_, (a, b): (f64, f64)| Ok(a.max(b)))
        .map_err(lua_err)?;
    m.set("max", max_fn).map_err(lua_err)?;

    // ── Geometry ──
    let distance_fn = lua
        .create_function(|_, (x1, y1, x2, y2): (f64, f64, f64, f64)| {
            let dx = x2 - x1;
            let dy = y2 - y1;
            Ok((dx * dx + dy * dy).sqrt())
        })
        .map_err(lua_err)?;
    m.set("distance", distance_fn).map_err(lua_err)?;

    let normalize_fn = lua
        .create_function(|_, (x, y): (f64, f64)| {
            let len = (x * x + y * y).sqrt();
            if len < 1e-10 {
                Ok((0.0, 0.0))
            } else {
                Ok((x / len, y / len))
            }
        })
        .map_err(lua_err)?;
    m.set("normalize", normalize_fn).map_err(lua_err)?;

    // ── Angle helpers ──
    let radians_fn = lua
        .create_function(|_, degrees: f64| Ok(degrees * std::f64::consts::PI / 180.0))
        .map_err(lua_err)?;
    m.set("radians", radians_fn).map_err(lua_err)?;

    let degrees_fn = lua
        .create_function(|_, radians: f64| Ok(radians * 180.0 / std::f64::consts::PI))
        .map_err(lua_err)?;
    m.set("degrees", degrees_fn).map_err(lua_err)?;

    Ok(m)
}
