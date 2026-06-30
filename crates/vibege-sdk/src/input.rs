use std::sync::Arc;
use std::sync::Mutex;

use mlua::{Lua, Table};
use vibege_input::InputManager;

fn lock_input(input: &Arc<Mutex<InputManager>>) -> std::sync::MutexGuard<'_, InputManager> {
    input.lock().unwrap_or_else(|e| {
        tracing::warn!("Input mutex poisoned — recovering inner data");
        e.into_inner()
    })
}

pub fn register_input_api(lua: &Lua, input: &Arc<Mutex<InputManager>>) -> Result<Table, String> {
    let input_table = lua.create_table().map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let is_down = lua
        .create_function(move |_, key: String| {
            Ok(lock_input(&inp).is_key_down(vibege_input::key_name_to_code(&key)))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("is_key_down", is_down)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let is_pr = lua
        .create_function(move |_, key: String| {
            Ok(lock_input(&inp).is_key_pressed(vibege_input::key_name_to_code(&key)))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("is_key_pressed", is_pr)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let is_rel = lua
        .create_function(move |_, key: String| {
            Ok(lock_input(&inp).is_key_released(vibege_input::key_name_to_code(&key)))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("is_key_released", is_rel)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let key_state_fn = lua
        .create_function(move |_, key: String| {
            let state = lock_input(&inp).key_state(vibege_input::key_name_to_code(&key));
            Ok(match state {
                vibege_input::ButtonState::Pressed => "pressed",
                vibege_input::ButtonState::Held => "held",
                vibege_input::ButtonState::Released => "released",
                vibege_input::ButtonState::Idle => "idle",
            }
            .to_string())
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("key_state", key_state_fn)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let mpos = lua
        .create_function(move |_, ()| {
            let lock = lock_input(&inp);
            let (x, y) = lock.mouse_position();
            Ok((x, y))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("mouse_position", mpos)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let mdelta = lua
        .create_function(move |_, ()| {
            let lock = lock_input(&inp);
            let (x, y) = lock.mouse_delta();
            Ok((x, y))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("mouse_delta", mdelta)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let sdelta = lua
        .create_function(move |_, ()| {
            let lock = lock_input(&inp);
            let (x, y) = lock.scroll_delta();
            Ok((x, y))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("scroll_delta", sdelta)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let is_mb_down = lua
        .create_function(move |_, btn: String| {
            Ok(lock_input(&inp).is_mouse_button_down(name_to_mouse_button(&btn)))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("is_mouse_down", is_mb_down)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let is_mb_pr = lua
        .create_function(move |_, btn: String| {
            Ok(lock_input(&inp).is_mouse_button_pressed(name_to_mouse_button(&btn)))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("is_mouse_pressed", is_mb_pr)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let gp_conn = lua
        .create_function(move |_, ()| Ok(lock_input(&inp).is_gamepad_connected()))
        .map_err(|e| e.to_string())?;
    input_table
        .set("is_gamepad_connected", gp_conn)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let gp_down = lua
        .create_function(move |_, btn: String| {
            Ok(lock_input(&inp).is_gamepad_button_down(name_to_gamepad_button(&btn)))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("is_gamepad_down", gp_down)
        .map_err(|e| e.to_string())?;

    let inp = Arc::clone(input);
    let gp_axis = lua
        .create_function(move |_, axis: String| {
            Ok(lock_input(&inp).gamepad_axis(name_to_gamepad_axis(&axis)))
        })
        .map_err(|e| e.to_string())?;
    input_table
        .set("gamepad_axis", gp_axis)
        .map_err(|e| e.to_string())?;

    Ok(input_table)
}

pub(crate) fn name_to_mouse_button(name: &str) -> vibege_input::MouseButton {
    match name.to_lowercase().as_str() {
        "left" => vibege_input::MouseButton::Left,
        "right" => vibege_input::MouseButton::Right,
        "middle" => vibege_input::MouseButton::Middle,
        "back" => vibege_input::MouseButton::Back,
        "forward" => vibege_input::MouseButton::Forward,
        _ => vibege_input::MouseButton::Left,
    }
}

pub(crate) fn name_to_gamepad_button(name: &str) -> vibege_input::GamepadButton {
    match name.to_lowercase().as_str() {
        "south" | "a" => vibege_input::GamepadButton::South,
        "north" | "y" => vibege_input::GamepadButton::North,
        "east" | "b" => vibege_input::GamepadButton::East,
        "west" | "x" => vibege_input::GamepadButton::West,
        "left_trigger" => vibege_input::GamepadButton::LeftTrigger,
        "right_trigger" => vibege_input::GamepadButton::RightTrigger,
        "left_shoulder" => vibege_input::GamepadButton::LeftShoulder,
        "right_shoulder" => vibege_input::GamepadButton::RightShoulder,
        "select" | "back" => vibege_input::GamepadButton::Select,
        "start" => vibege_input::GamepadButton::Start,
        "left_stick" => vibege_input::GamepadButton::LeftStick,
        "right_stick" => vibege_input::GamepadButton::RightStick,
        "dpad_up" => vibege_input::GamepadButton::DPadUp,
        "dpad_down" => vibege_input::GamepadButton::DPadDown,
        "dpad_left" => vibege_input::GamepadButton::DPadLeft,
        "dpad_right" => vibege_input::GamepadButton::DPadRight,
        _ => vibege_input::GamepadButton::South,
    }
}

pub(crate) fn name_to_gamepad_axis(name: &str) -> vibege_input::GamepadAxis {
    match name.to_lowercase().as_str() {
        "left_stick_x" | "lx" => vibege_input::GamepadAxis::LeftStickX,
        "left_stick_y" | "ly" => vibege_input::GamepadAxis::LeftStickY,
        "right_stick_x" | "rx" => vibege_input::GamepadAxis::RightStickX,
        "right_stick_y" | "ry" => vibege_input::GamepadAxis::RightStickY,
        "left_trigger" | "lt" => vibege_input::GamepadAxis::LeftTrigger,
        "right_trigger" | "rt" => vibege_input::GamepadAxis::RightTrigger,
        _ => vibege_input::GamepadAxis::LeftStickX,
    }
}
