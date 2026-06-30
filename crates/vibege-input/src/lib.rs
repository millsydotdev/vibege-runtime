//! # VibeGE Input System
//!
//! Cross-platform input abstraction for keyboard, mouse, and gamepad,
//! with an action mapping system and input contexts.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │                    InputManager                      │
//! │  • Processes winit events each frame                 │
//! │  • Tracks keyboard, mouse, gamepad device state      │
//! │  • Exposes raw key/button/axis queries               │
//! └──────┬──────────┬──────────────┬─────────────────────┘
//!        │          │              │
//!        ▼          ▼              ▼
//! ┌──────────┐ ┌──────────┐ ┌──────────────┐
//! │ ActionMap│ │ CtxStack │ │ GamepadState │
//! │ • actions│ │ • stack  │ │ • 4 slots    │
//! │ • axes   │ │ • top-   │ │ • dead zones │
//! │ • chords │ │   down   │ │ • axis conf  │
//! │ • confs  │ │   resolve│ │              │
//! └──────────┘ └──────────┘ └──────────────┘
//! ```
//!
//! # Frame Lifecycle
//!
//! 1. **Poll** — winit events arrive via `handle_window_event()` /
//!    `handle_device_event()`
//! 2. **Process** — InputManager updates raw device state
//! 3. **Query** — Game code queries actions, axes, mouse, gamepad
//! 4. **End** — `end_frame()` transitions Pressed→Held, Released→Idle,
//!    clears per-frame deltas
//!
//! # Thread Safety
//!
//! `InputManager` is **not** `Send + Sync`. It must be accessed from the
//! main thread (typically behind `Arc<Mutex<InputManager>>`).
//!
//! `ActionMap` and `ContextStack` are `Clone + Send`.

pub mod action;
pub mod context;
pub mod gamepad;
pub mod mouse;

use std::collections::HashMap;

use winit::event::MouseScrollDelta;
use winit::keyboard::{KeyCode, PhysicalKey};

pub use gamepad::{GamepadAxis, GamepadButton};
pub use mouse::MouseButton;

// ---------------------------------------------------------------------------
// ButtonState
// ---------------------------------------------------------------------------

/// Current state of a single key or button input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    /// Pressed this frame (edge trigger).
    Pressed,
    /// Held down from a previous frame.
    Held,
    /// Released this frame.
    Released,
    /// Not active.
    Idle,
}

// ---------------------------------------------------------------------------
// InputManager
// ---------------------------------------------------------------------------

/// Accumulated input state for the current frame.
///
/// Processes winit events and exposes a clean API for game code, action
/// mapping, and input contexts.
pub struct InputManager {
    keyboard: HashMap<KeyCode, ButtonState>,
    mouse: mouse::MouseState,
    gamepad: gamepad::GamepadSystem,
    /// Whether the window is focused.
    focused: bool,
}

impl InputManager {
    /// Creates a new input manager with all devices in idle state.
    pub fn new() -> Self {
        Self {
            keyboard: HashMap::new(),
            mouse: mouse::MouseState::new(),
            gamepad: gamepad::GamepadSystem::new(),
            focused: true,
        }
    }

    // ─── Focus ──────────────────────────────────────────────────────

    /// Whether the window currently has focus.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Set the window focus state.
    /// When focus is lost, all keys and gamepad states are released
    /// (the OS may not deliver key-up events after focus loss).
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            for state in self.keyboard.values_mut() {
                *state = ButtonState::Idle;
            }
            for pad in &mut self.gamepad.pads {
                for state in pad.button_states.values_mut() {
                    *state = ButtonState::Idle;
                }
            }
        }
    }

    // ─── Keyboard API ────────────────────────────────────────────────

    /// Returns `true` if the key is currently held down.
    pub fn is_key_down(&self, key: KeyCode) -> bool {
        matches!(
            self.keyboard.get(&key),
            Some(ButtonState::Pressed) | Some(ButtonState::Held)
        )
    }

    /// Returns `true` if the key was pressed this frame (edge trigger).
    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keyboard.get(&key) == Some(&ButtonState::Pressed)
    }

    /// Returns `true` if the key was released this frame.
    pub fn is_key_released(&self, key: KeyCode) -> bool {
        self.keyboard.get(&key) == Some(&ButtonState::Released)
    }

    /// Returns the state of a specific key.
    pub fn key_state(&self, key: KeyCode) -> ButtonState {
        self.keyboard
            .get(&key)
            .copied()
            .unwrap_or(ButtonState::Idle)
    }

    /// Returns an iterator over all currently pressed keys.
    pub fn pressed_keys(&self) -> impl Iterator<Item = KeyCode> + '_ {
        self.keyboard.iter().filter_map(|(&k, &s)| {
            if s == ButtonState::Pressed || s == ButtonState::Held {
                Some(k)
            } else {
                None
            }
        })
    }

    /// Directly set a key state (for testing or programmatic control).
    pub fn set_key_state(&mut self, key: KeyCode, state: ButtonState) {
        self.keyboard.insert(key, state);
    }

    // ─── Mouse API ───────────────────────────────────────────────────

    /// Returns the current mouse position in window coordinates.
    pub fn mouse_position(&self) -> (f64, f64) {
        self.mouse.position
    }

    /// Returns the mouse movement delta since last frame.
    pub fn mouse_delta(&self) -> (f64, f64) {
        self.mouse.delta
    }

    /// Returns `true` if the specified mouse button is held down.
    pub fn is_mouse_button_down(&self, button: MouseButton) -> bool {
        matches!(
            self.mouse.button_states.get(&button),
            Some(ButtonState::Pressed) | Some(ButtonState::Held)
        )
    }

    /// Returns `true` if the specified mouse button was pressed this frame.
    pub fn is_mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.mouse.button_states.get(&button) == Some(&ButtonState::Pressed)
    }

    /// Returns `true` if the specified mouse button was released this frame.
    pub fn is_mouse_button_released(&self, button: MouseButton) -> bool {
        self.mouse.button_states.get(&button) == Some(&ButtonState::Released)
    }

    /// Returns the scroll wheel delta since last frame.
    pub fn scroll_delta(&self) -> (f64, f64) {
        self.mouse.scroll_delta
    }

    /// Mouse button state (for action system).
    pub fn mouse_button_state(&self, button: MouseButton) -> ButtonState {
        self.mouse
            .button_states
            .get(&button)
            .copied()
            .unwrap_or(ButtonState::Idle)
    }

    /// Was the mouse button double-clicked?
    pub fn is_double_click(&self, button: MouseButton) -> bool {
        self.mouse.double_click.contains_key(&button)
    }

    /// Is the mouse button currently being dragged?
    pub fn is_dragging(&self, button: MouseButton) -> bool {
        self.mouse
            .is_dragging
            .get(&button)
            .copied()
            .unwrap_or(false)
    }

    /// Set cursor visibility.
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.mouse.cursor_visible = visible;
    }

    /// Is the cursor visible?
    pub fn cursor_visible(&self) -> bool {
        self.mouse.cursor_visible
    }

    /// Set cursor lock (grab).
    pub fn set_cursor_locked(&mut self, locked: bool) {
        self.mouse.cursor_locked = locked;
    }

    /// Is the cursor locked?
    pub fn cursor_locked(&self) -> bool {
        self.mouse.cursor_locked
    }

    // ─── Gamepad API ─────────────────────────────────────────────────

    /// Returns `true` if at least one gamepad is connected.
    pub fn is_gamepad_connected(&self) -> bool {
        self.gamepad.any_connected()
    }

    /// Number of connected gamepads.
    pub fn gamepad_count(&self) -> usize {
        self.gamepad.connected_count()
    }

    /// Returns `true` if a specific gamepad slot is connected.
    pub fn is_gamepad_slot_connected(&self, slot: usize) -> bool {
        self.gamepad.get(slot).map(|p| p.connected).unwrap_or(false)
    }

    /// Returns `true` if the specified gamepad button is held down (slot 0).
    pub fn is_gamepad_button_down(&self, button: GamepadButton) -> bool {
        self.is_gamepad_button_down_slot(0, button)
    }

    /// Returns `true` if the specified gamepad button was pressed this frame (slot 0).
    pub fn is_gamepad_button_pressed(&self, button: GamepadButton) -> bool {
        self.is_gamepad_button_pressed_slot(0, button)
    }

    /// Returns `true` if the specified gamepad button is held down on a specific slot.
    pub fn is_gamepad_button_down_slot(&self, slot: usize, button: GamepadButton) -> bool {
        self.gamepad.get(slot).is_some_and(|p| {
            matches!(
                p.button_states.get(&button),
                Some(ButtonState::Pressed) | Some(ButtonState::Held)
            )
        })
    }

    /// Returns `true` if the specified gamepad button was pressed this frame on a specific slot.
    pub fn is_gamepad_button_pressed_slot(&self, slot: usize, button: GamepadButton) -> bool {
        self.gamepad
            .get(slot)
            .is_some_and(|p| p.button_states.get(&button) == Some(&ButtonState::Pressed))
    }

    /// Gamepad button state (for action system).
    pub fn gamepad_button_state(&self, button: GamepadButton) -> ButtonState {
        self.gamepad_button_state_slot(0, button)
    }

    /// Gamepad button state for a specific slot.
    pub fn gamepad_button_state_slot(&self, slot: usize, button: GamepadButton) -> ButtonState {
        self.gamepad
            .get(slot)
            .and_then(|p| p.button_states.get(&button))
            .copied()
            .unwrap_or(ButtonState::Idle)
    }

    /// Returns the value of a gamepad axis (slot 0, range -1.0 to 1.0).
    pub fn gamepad_axis(&self, axis: GamepadAxis) -> f64 {
        self.gamepad_axis_slot(0, axis)
    }

    /// Returns the value of a gamepad axis for a specific slot.
    pub fn gamepad_axis_slot(&self, slot: usize, axis: GamepadAxis) -> f64 {
        self.gamepad
            .get(slot)
            .and_then(|p| p.axes.get(&axis))
            .copied()
            .unwrap_or(0.0)
    }

    /// Set a raw axis value for a gamepad slot.
    pub fn set_gamepad_axis(&mut self, axis: GamepadAxis, value: f64) {
        if let Some(pad) = self.gamepad.get_mut(0) {
            pad.axes.insert(axis, value);
        }
    }

    /// Mark a gamepad slot as connected/disconnected.
    pub fn set_gamepad_connected(&mut self, slot: usize, connected: bool) {
        self.gamepad.set_connected(slot, connected);
    }

    /// Set the name of a connected gamepad.
    pub fn set_gamepad_name(&mut self, slot: usize, name: &str) {
        if let Some(pad) = self.gamepad.get_mut(slot) {
            pad.name = Some(name.to_string());
        }
    }

    /// Get the name of a connected gamepad.
    pub fn gamepad_name(&self, slot: usize) -> Option<&str> {
        self.gamepad.get(slot).and_then(|p| p.name.as_deref())
    }

    // ─── Event Processing ────────────────────────────────────────────

    /// Processes a winit window event and updates input state.
    pub fn handle_window_event(&mut self, event: &winit::event::WindowEvent) {
        match event {
            winit::event::WindowEvent::Focused(focused) => {
                self.focused = *focused;
                if !focused {
                    self.release_all();
                }
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    match event.state {
                        winit::event::ElementState::Pressed => {
                            self.keyboard.insert(keycode, ButtonState::Pressed);
                        }
                        winit::event::ElementState::Released => {
                            self.keyboard.insert(keycode, ButtonState::Released);
                        }
                    }
                }
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                self.mouse.on_move((position.x, position.y));
            }
            winit::event::WindowEvent::MouseInput { button, state, .. } => {
                let btn = MouseButton::from(*button);
                match state {
                    winit::event::ElementState::Pressed => {
                        self.mouse.on_button_down(btn);
                        self.mouse.button_states.insert(btn, ButtonState::Pressed);
                    }
                    winit::event::ElementState::Released => {
                        self.mouse.on_button_up(btn);
                        self.mouse.button_states.insert(btn, ButtonState::Released);
                    }
                }
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(x, y) => {
                    self.mouse.scroll_delta.0 += *x as f64;
                    self.mouse.scroll_delta.1 += *y as f64;
                }
                MouseScrollDelta::PixelDelta(pos) => {
                    self.mouse.scroll_delta.0 += pos.x;
                    self.mouse.scroll_delta.1 += pos.y;
                }
            },
            _ => {}
        }
    }

    /// Processes a winit device event for gamepad/controller input.
    pub fn handle_device_event(&mut self, event: &winit::event::DeviceEvent) {
        match event {
            winit::event::DeviceEvent::Button { button, state } => {
                let b = *button as u16;
                if b <= 16 {
                    let gp_btn = gamepad::raw_button_to_gamepad(b);
                    let pad_state = match state {
                        winit::event::ElementState::Pressed => ButtonState::Pressed,
                        winit::event::ElementState::Released => ButtonState::Released,
                    };
                    if let Some(pad) = self.gamepad.get_mut(0) {
                        pad.button_states.insert(gp_btn, pad_state);
                        pad.connected = true;
                    }
                }
            }
            winit::event::DeviceEvent::MouseMotion { delta } => {
                self.mouse.delta = (self.mouse.delta.0 + delta.0, self.mouse.delta.1 + delta.1);
            }
            _ => {}
        }
    }

    /// Releases all pressed keys and buttons (used on focus loss).
    fn release_all(&mut self) {
        for state in self.keyboard.values_mut() {
            match *state {
                ButtonState::Pressed | ButtonState::Held => *state = ButtonState::Released,
                _ => {}
            }
        }
        for state in self.mouse.button_states.values_mut() {
            match *state {
                ButtonState::Pressed | ButtonState::Held => *state = ButtonState::Released,
                _ => {}
            }
        }
        for pad in &mut self.gamepad.pads {
            for state in pad.button_states.values_mut() {
                match *state {
                    ButtonState::Pressed | ButtonState::Held => *state = ButtonState::Released,
                    _ => {}
                }
            }
            pad.axes.clear();
        }
    }

    /// Advances to the next frame.
    ///
    /// Call this once per frame after processing all events.
    /// Transitions Pressed→Held, Released→Idle, clears per-frame deltas.
    pub fn end_frame(&mut self) {
        // Keyboard
        for state in self.keyboard.values_mut() {
            match *state {
                ButtonState::Pressed => *state = ButtonState::Held,
                ButtonState::Released => *state = ButtonState::Idle,
                _ => {}
            }
        }

        // Mouse
        self.mouse.delta = (0.0, 0.0);
        self.mouse.scroll_delta = (0.0, 0.0);
        for state in self.mouse.button_states.values_mut() {
            match *state {
                ButtonState::Pressed => *state = ButtonState::Held,
                ButtonState::Released => *state = ButtonState::Idle,
                _ => {}
            }
        }

        // Gamepad
        for pad in &mut self.gamepad.pads {
            for state in pad.button_states.values_mut() {
                match *state {
                    ButtonState::Pressed => *state = ButtonState::Held,
                    ButtonState::Released => *state = ButtonState::Idle,
                    _ => {}
                }
            }
        }
    }
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Key name conversion
// ---------------------------------------------------------------------------

/// Convert a string key name to a winit KeyCode.
/// Used by Lua bindings to map string keys like "left", "space" to platform keycodes.
pub fn key_name_to_code(name: &str) -> KeyCode {
    use KeyCode::*;
    match name.to_lowercase().as_str() {
        "a" => KeyA,
        "b" => KeyB,
        "c" => KeyC,
        "d" => KeyD,
        "e" => KeyE,
        "f" => KeyF,
        "g" => KeyG,
        "h" => KeyH,
        "i" => KeyI,
        "j" => KeyJ,
        "k" => KeyK,
        "l" => KeyL,
        "m" => KeyM,
        "n" => KeyN,
        "o" => KeyO,
        "p" => KeyP,
        "q" => KeyQ,
        "r" => KeyR,
        "s" => KeyS,
        "t" => KeyT,
        "u" => KeyU,
        "v" => KeyV,
        "w" => KeyW,
        "x" => KeyX,
        "y" => KeyY,
        "z" => KeyZ,
        "0" => Digit0,
        "1" => Digit1,
        "2" => Digit2,
        "3" => Digit3,
        "4" => Digit4,
        "5" => Digit5,
        "6" => Digit6,
        "7" => Digit7,
        "8" => Digit8,
        "9" => Digit9,
        "f1" => F1,
        "f2" => F2,
        "f3" => F3,
        "f4" => F4,
        "f5" => F5,
        "f6" => F6,
        "f7" => F7,
        "f8" => F8,
        "f9" => F9,
        "f10" => F10,
        "f11" => F11,
        "f12" => F12,
        "up" => ArrowUp,
        "down" => ArrowDown,
        "left" => ArrowLeft,
        "right" => ArrowRight,
        "shift" => ShiftLeft,
        "ctrl" | "control" => ControlLeft,
        "alt" => AltLeft,
        "tab" => Tab,
        "space" => Space,
        "enter" | "return" => Enter,
        "escape" | "esc" => Escape,
        "backspace" => Backspace,
        "delete" => Delete,
        "home" => Home,
        "end" => End,
        "pageup" => PageUp,
        "pagedown" => PageDown,
        _ => Space,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn test_input_manager_creation() {
        let im = InputManager::new();
        assert_eq!(im.mouse_position(), (0.0, 0.0));
        assert!(!im.is_gamepad_connected());
        assert!(im.is_focused());
    }

    #[test]
    fn test_key_press_and_release() {
        let mut im = InputManager::new();
        assert!(!im.is_key_down(KeyCode::Space));

        im.set_key_state(KeyCode::Space, ButtonState::Pressed);
        assert!(im.is_key_pressed(KeyCode::Space));
        assert!(im.is_key_down(KeyCode::Space));

        im.end_frame();
        assert!(!im.is_key_pressed(KeyCode::Space));
        assert!(im.is_key_down(KeyCode::Space));

        im.set_key_state(KeyCode::Space, ButtonState::Released);
        assert!(im.is_key_released(KeyCode::Space));
        assert!(!im.is_key_down(KeyCode::Space));

        im.end_frame();
        assert!(!im.is_key_released(KeyCode::Space));
        assert_eq!(im.key_state(KeyCode::Space), ButtonState::Idle);
    }

    #[test]
    fn test_mouse_position_and_delta() {
        let mut im = InputManager::new();
        assert_eq!(im.mouse_position(), (0.0, 0.0));
        assert_eq!(im.mouse_delta(), (0.0, 0.0));

        // Simulate cursor move via internal mouse state
        im.mouse = mouse::MouseState::new();
        im.mouse.position = (100.0, 200.0);
        im.mouse.delta = (100.0, 200.0);
        assert_eq!(im.mouse_position(), (100.0, 200.0));
        assert_eq!(im.mouse_delta(), (100.0, 200.0));

        im.end_frame();
        assert_eq!(im.mouse_delta(), (0.0, 0.0));
        assert_eq!(im.mouse_position(), (100.0, 200.0));
    }

    #[test]
    fn test_mouse_button() {
        let mut im = InputManager::new();
        assert!(!im.is_mouse_button_down(MouseButton::Left));

        im.mouse
            .button_states
            .insert(MouseButton::Left, ButtonState::Pressed);
        assert!(im.is_mouse_button_pressed(MouseButton::Left));
        assert!(im.is_mouse_button_down(MouseButton::Left));

        im.end_frame();
        assert!(!im.is_mouse_button_pressed(MouseButton::Left));
        assert!(im.is_mouse_button_down(MouseButton::Left));
    }

    #[test]
    fn test_scroll_delta() {
        let mut im = InputManager::new();
        assert_eq!(im.scroll_delta(), (0.0, 0.0));

        im.mouse.scroll_delta = (0.0, 10.0);
        assert_eq!(im.scroll_delta(), (0.0, 10.0));

        im.end_frame();
        assert_eq!(im.scroll_delta(), (0.0, 0.0));
    }

    #[test]
    fn test_gamepad_connection() {
        let mut im = InputManager::new();
        assert!(!im.is_gamepad_connected());
        im.set_gamepad_connected(0, true);
        assert!(im.is_gamepad_connected());
    }

    #[test]
    fn test_gamepad_button() {
        let mut im = InputManager::new();
        assert!(!im.is_gamepad_button_down(GamepadButton::South));

        if let Some(pad) = im.gamepad.get_mut(0) {
            pad.button_states
                .insert(GamepadButton::South, ButtonState::Pressed);
        }
        assert!(im.is_gamepad_button_pressed(GamepadButton::South));

        im.end_frame();
        assert!(!im.is_gamepad_button_pressed(GamepadButton::South));
        assert!(im.is_gamepad_button_down(GamepadButton::South));
    }

    #[test]
    fn test_gamepad_axis() {
        let mut im = InputManager::new();
        assert_eq!(im.gamepad_axis(GamepadAxis::LeftStickX), 0.0);

        im.set_gamepad_axis(GamepadAxis::LeftStickX, 0.5);
        assert!((im.gamepad_axis(GamepadAxis::LeftStickX) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_pressed_keys_iterator() {
        let mut im = InputManager::new();
        im.set_key_state(KeyCode::Space, ButtonState::Pressed);
        im.set_key_state(KeyCode::KeyW, ButtonState::Held);

        let pressed: Vec<KeyCode> = im.pressed_keys().collect();
        assert!(pressed.contains(&KeyCode::Space));
        assert!(pressed.contains(&KeyCode::KeyW));
    }

    #[test]
    fn test_end_frame_clears_states() {
        let mut im = InputManager::new();
        im.set_key_state(KeyCode::KeyA, ButtonState::Pressed);
        im.mouse
            .button_states
            .insert(MouseButton::Left, ButtonState::Pressed);
        im.mouse.delta = (5.0, 3.0);
        im.mouse.scroll_delta = (0.0, -1.0);

        im.end_frame();

        assert!(!im.is_key_pressed(KeyCode::KeyA));
        assert!(im.is_key_down(KeyCode::KeyA));
        assert!(!im.is_mouse_button_pressed(MouseButton::Left));
        assert_eq!(im.mouse_delta(), (0.0, 0.0));
        assert_eq!(im.scroll_delta(), (0.0, 0.0));
    }

    #[test]
    fn test_focus_tracking() {
        let mut im = InputManager::new();
        assert!(im.is_focused());
        im.set_focused(false);
        assert!(!im.is_focused());
    }

    #[test]
    fn test_gamepad_multi_controller() {
        let mut im = InputManager::new();
        im.set_gamepad_connected(0, true);
        im.set_gamepad_connected(1, true);
        im.set_gamepad_connected(2, false);

        assert_eq!(im.gamepad_count(), 2);
        assert!(im.is_gamepad_slot_connected(0));
        assert!(im.is_gamepad_slot_connected(1));
        assert!(!im.is_gamepad_slot_connected(2));

        if let Some(pad) = im.gamepad.get_mut(1) {
            pad.button_states
                .insert(GamepadButton::East, ButtonState::Pressed);
        }
        assert!(im.is_gamepad_button_pressed_slot(1, GamepadButton::East));
        assert!(!im.is_gamepad_button_pressed_slot(0, GamepadButton::East));
    }

    #[test]
    fn test_cursor_control() {
        let mut im = InputManager::new();
        assert!(im.cursor_visible());
        assert!(!im.cursor_locked());

        im.set_cursor_visible(false);
        assert!(!im.cursor_visible());

        im.set_cursor_locked(true);
        assert!(im.cursor_locked());
    }

    #[test]
    fn test_double_click() {
        let mut im = InputManager::new();
        // Simulate two rapid clicks
        im.mouse.on_button_down(MouseButton::Left);
        im.mouse.on_button_down(MouseButton::Left);
        assert!(im.is_double_click(MouseButton::Left));
    }

    #[test]
    fn test_gamepad_name() {
        let mut im = InputManager::new();
        assert!(im.gamepad_name(0).is_none());

        im.set_gamepad_name(0, "Xbox Controller");
        assert_eq!(im.gamepad_name(0), Some("Xbox Controller"));
    }

    #[test]
    fn test_focus_loss_releases_keys() {
        let mut im = InputManager::new();
        im.set_key_state(KeyCode::KeyW, ButtonState::Held);
        assert!(im.is_key_down(KeyCode::KeyW));

        im.set_focused(false);
        assert!(!im.is_key_down(KeyCode::KeyW));
    }
}
