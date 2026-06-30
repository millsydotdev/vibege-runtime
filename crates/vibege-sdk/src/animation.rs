use std::sync::{Arc, Mutex};

use mlua::{Lua, Table};

use crate::{SdkState, TweenEntry};

pub fn register_animation_api(
    lua: &Lua,
    sdk_state: &Arc<Mutex<SdkState>>,
) -> Result<Table, String> {
    let a = lua.create_table().map_err(|e| e.to_string())?;

    // ── tween(id, target, duration, from, to, easing?) → id ──
    let st = Arc::clone(sdk_state);
    let tween_fn = lua
        .create_function(
            move |_, (id, duration, from, to, easing): (String, f64, f64, f64, Option<u8>)| {
                let mut state = st
                    .lock()
                    .map_err(|e| mlua::Error::external(e.to_string()))?;
                let tween_id = state.next_tween_id;
                state.next_tween_id += 1;
                state.tweens.push(TweenEntry {
                    id: tween_id,
                    remaining: duration,
                    duration,
                    from,
                    to,
                    value: from,
                    done: false,
                    easing: easing.unwrap_or(0),
                    on_complete: Some(id),
                });
                Ok(tween_id as f64)
            },
        )
        .map_err(|e| e.to_string())?;
    a.set("tween", tween_fn).map_err(|e| e.to_string())?;

    // ── get_tween_value(id) → f64 or nil ──
    let st = Arc::clone(sdk_state);
    let get_val_fn = lua
        .create_function(move |_, id: f64| {
            let state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            let id = id as u64;
            for t in &state.tweens {
                if t.id == id {
                    return Ok(Some(t.value));
                }
            }
            Ok(None)
        })
        .map_err(|e| e.to_string())?;
    a.set("get_tween_value", get_val_fn)
        .map_err(|e| e.to_string())?;

    // ── is_tween_done(id) → bool ──
    let st = Arc::clone(sdk_state);
    let is_done_fn = lua
        .create_function(move |_, id: f64| {
            let state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            let id = id as u64;
            for t in &state.tweens {
                if t.id == id {
                    return Ok(t.done);
                }
            }
            Ok(true)
        })
        .map_err(|e| e.to_string())?;
    a.set("is_tween_done", is_done_fn)
        .map_err(|e| e.to_string())?;

    // ── cancel_tween(id) ──
    let st = Arc::clone(sdk_state);
    let cancel_fn = lua
        .create_function(move |_, id: f64| {
            let mut state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            let id = id as u64;
            state.tweens.retain(|t| t.id != id);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    a.set("cancel_tween", cancel_fn)
        .map_err(|e| e.to_string())?;

    // ── cancel_all_tweens() ──
    let st = Arc::clone(sdk_state);
    let cancel_all_fn = lua
        .create_function(move |_, ()| {
            let mut state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            state.tweens.clear();
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    a.set("cancel_all_tweens", cancel_all_fn)
        .map_err(|e| e.to_string())?;

    // ── tween_count() → int ──
    let st = Arc::clone(sdk_state);
    let count_fn = lua
        .create_function(move |_, ()| {
            let state = st
                .lock()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(state.tweens.len() as f64)
        })
        .map_err(|e| e.to_string())?;
    a.set("tween_count", count_fn).map_err(|e| e.to_string())?;

    // ── Easing constants ──
    let easings = lua.create_table().map_err(|e| e.to_string())?;
    easings.set("linear", 0.0).map_err(|e| e.to_string())?;
    easings.set("quad_in", 1.0).map_err(|e| e.to_string())?;
    easings.set("quad_out", 2.0).map_err(|e| e.to_string())?;
    easings.set("quad_in_out", 3.0).map_err(|e| e.to_string())?;
    easings.set("cubic_in", 4.0).map_err(|e| e.to_string())?;
    easings.set("cubic_out", 5.0).map_err(|e| e.to_string())?;
    a.set("easing", easings).map_err(|e| e.to_string())?;

    Ok(a)
}
