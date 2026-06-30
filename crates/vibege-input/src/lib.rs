//! # VibeGE Input
//!
//! Cross-platform input abstraction for keyboard, mouse, and gamepad.
//!
//! The `InputManager` accumulates input events each frame and exposes
//! both event-based APIs (is_key_pressed, is_key_released) and
//! state-based APIs (is_key_down, mouse_position, mouse_delta).

use winit::event::MouseScrollDelta;
use winit::keyboard::{KeyCode, PhysicalKey};

/// Represents a mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

impl From<winit::event::MouseButton> for MouseButton {
    fn from(b: winit::event::MouseButton) -> Self {
        match b {
            winit::event::MouseButton::Left => Self::Left,
            winit::event::MouseButton::Right => Self::Right,
            winit::event::MouseButton::Middle => Self::Middle,
            winit::event::MouseButton::Back => Self::Back,
            winit::event::MouseButton::Forward => Self::Forward,
            winit::event::MouseButton::Other(v) => Self::Other(v),
        }
    }
}

/// Represents a gamepad button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    South,
    North,
    East,
    West,
    LeftTrigger,
    RightTrigger,
    LeftShoulder,
    RightShoulder,
    Select,
    Start,
    LeftStick,
    RightStick,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// Represents a gamepad axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftTrigger,
    RightTrigger,
}

/// Current state of a single key or button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Held,
    Released,
    Idle,
}

/// Accumulated input state for the current frame.
///
/// Tracks the state of all input devices. `InputManager` processes
/// winit events and exposes a clean API for game code.
pub struct InputManager {
    keyboard: KeyboardState,
    mouse: MouseState,
    gamepad: GamepadState,
}

impl InputManager {
    /// Creates a new input manager with all devices in idle state.
    pub fn new() -> Self {
        Self {
            keyboard: KeyboardState::new(),
            mouse: MouseState::new(),
            gamepad: GamepadState::new(),
        }
    }

    // ─── Keyboard API ────────────────────────────────────────────────

    /// Returns `true` if the key is currently held down.
    pub fn is_key_down(&self, key: KeyCode) -> bool {
        self.keyboard.key_states.get(&key).copied() == Some(ButtonState::Pressed)
            || self.keyboard.key_states.get(&key).copied() == Some(ButtonState::Held)
    }

    /// Returns `true` if the key was pressed this frame (edge trigger).
    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keyboard.key_states.get(&key).copied() == Some(ButtonState::Pressed)
    }

    /// Returns `true` if the key was released this frame.
    pub fn is_key_released(&self, key: KeyCode) -> bool {
        self.keyboard.key_states.get(&key).copied() == Some(ButtonState::Released)
    }

    /// Returns the state of a specific key.
    pub fn key_state(&self, key: KeyCode) -> ButtonState {
        self.keyboard
            .key_states
            .get(&key)
            .copied()
            .unwrap_or(ButtonState::Idle)
    }

    /// Returns an iterator over all currently pressed keys.
    pub fn pressed_keys(&self) -> impl Iterator<Item = KeyCode> + '_ {
        self.keyboard.key_states.iter().filter_map(|(&k, &s)| {
            if s == ButtonState::Pressed || s == ButtonState::Held {
                Some(k)
            } else {
                None
            }
        })
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
        self.mouse.button_states.get(&button).copied() == Some(ButtonState::Pressed)
            || self.mouse.button_states.get(&button).copied() == Some(ButtonState::Held)
    }

    /// Returns `true` if the specified mouse button was pressed this frame.
    pub fn is_mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.mouse.button_states.get(&button).copied() == Some(ButtonState::Pressed)
    }

    /// Returns the scroll wheel delta since last frame.
    pub fn scroll_delta(&self) -> (f64, f64) {
        self.mouse.scroll_delta
    }

    // ─── Gamepad API ─────────────────────────────────────────────────

    /// Returns `true` if a gamepad is connected.
    pub fn is_gamepad_connected(&self) -> bool {
        self.gamepad.connected
    }

    /// Returns `true` if the specified gamepad button is held down.
    pub fn is_gamepad_button_down(&self, button: GamepadButton) -> bool {
        self.gamepad.button_states.get(&button).copied() == Some(ButtonState::Pressed)
            || self.gamepad.button_states.get(&button).copied() == Some(ButtonState::Held)
    }

    /// Returns `true` if the specified gamepad button was pressed this frame.
    pub fn is_gamepad_button_pressed(&self, button: GamepadButton) -> bool {
        self.gamepad.button_states.get(&button).copied() == Some(ButtonState::Pressed)
    }

    /// Returns the value of a gamepad axis (range -1.0 to 1.0).
    pub fn gamepad_axis(&self, axis: GamepadAxis) -> f64 {
        self.gamepad.axes.get(&axis).copied().unwrap_or(0.0)
    }

    // ─── Event Processing ────────────────────────────────────────────

    /// Processes a winit window event and updates input state.
    ///
    /// Call this from your event loop for each `WindowEvent` received.
    pub fn handle_window_event(&mut self, event: &winit::event::WindowEvent) {
        match event {
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    match event.state {
                        winit::event::ElementState::Pressed => {
                            self.keyboard
                                .key_states
                                .insert(keycode, ButtonState::Pressed);
                        }
                        winit::event::ElementState::Released => {
                            self.keyboard
                                .key_states
                                .insert(keycode, ButtonState::Released);
                        }
                    }
                }
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let new_pos = (position.x, position.y);
                self.mouse.delta = (
                    new_pos.0 - self.mouse.position.0,
                    new_pos.1 - self.mouse.position.1,
                );
                self.mouse.position = new_pos;
            }
            winit::event::WindowEvent::MouseInput { button, state, .. } => {
                let btn = MouseButton::from(*button);
                match state {
                    winit::event::ElementState::Pressed => {
                        self.mouse.button_states.insert(btn, ButtonState::Pressed);
                    }
                    winit::event::ElementState::Released => {
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
                    let gp_btn = raw_button_to_gamepad(b);
                    match state {
                        winit::event::ElementState::Pressed => {
                            self.gamepad
                                .button_states
                                .insert(gp_btn, ButtonState::Pressed);
                        }
                        winit::event::ElementState::Released => {
                            self.gamepad
                                .button_states
                                .insert(gp_btn, ButtonState::Released);
                        }
                    }
                    self.gamepad.connected = true;
                }
            }
            winit::event::DeviceEvent::MouseMotion { delta } => {
                // Relative mouse motion (for raw input / camera control)
                self.mouse.delta = (self.mouse.delta.0 + delta.0, self.mouse.delta.1 + delta.1);
            }
            _ => {}
        }
    }

    /// Updates gamepad connection state.
    pub fn set_gamepad_connected(&mut self, connected: bool) {
        self.gamepad.connected = connected;
    }

    /// Advances to the next frame.
    ///
    /// Call this once per frame after processing all events.
    /// This clears per-frame state (pressed/released, scroll delta, mouse delta).
    pub fn end_frame(&mut self) {
        // Clear per-frame key states
        for state in self.keyboard.key_states.values_mut() {
            match *state {
                ButtonState::Pressed => *state = ButtonState::Held,
                ButtonState::Released => *state = ButtonState::Idle,
                _ => {}
            }
        }

        // Clear per-frame mouse states
        self.mouse.delta = (0.0, 0.0);
        self.mouse.scroll_delta = (0.0, 0.0);
        for state in self.mouse.button_states.values_mut() {
            match *state {
                ButtonState::Pressed => *state = ButtonState::Held,
                ButtonState::Released => *state = ButtonState::Idle,
                _ => {}
            }
        }

        // Clear per-frame gamepad states
        for state in self.gamepad.button_states.values_mut() {
            match *state {
                ButtonState::Pressed => *state = ButtonState::Held,
                ButtonState::Released => *state = ButtonState::Idle,
                _ => {}
            }
        }
    }
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Internal State Types ──────────────────────────────────────────

struct KeyboardState {
    key_states: std::collections::HashMap<KeyCode, ButtonState>,
}

impl KeyboardState {
    fn new() -> Self {
        Self {
            key_states: std::collections::HashMap::new(),
        }
    }
}

struct MouseState {
    position: (f64, f64),
    delta: (f64, f64),
    button_states: std::collections::HashMap<MouseButton, ButtonState>,
    scroll_delta: (f64, f64),
}

impl MouseState {
    fn new() -> Self {
        Self {
            position: (0.0, 0.0),
            delta: (0.0, 0.0),
            button_states: std::collections::HashMap::new(),
            scroll_delta: (0.0, 0.0),
        }
    }
}

struct GamepadState {
    connected: bool,
    button_states: std::collections::HashMap<GamepadButton, ButtonState>,
    axes: std::collections::HashMap<GamepadAxis, f64>,
}

impl GamepadState {
    fn new() -> Self {
        Self {
            connected: false,
            button_states: std::collections::HashMap::new(),
            axes: std::collections::HashMap::new(),
        }
    }
}

fn raw_button_to_gamepad(button: u16) -> GamepadButton {
    match button {
        0 => GamepadButton::South,
        1 => GamepadButton::East,
        2 => GamepadButton::West,
        3 => GamepadButton::North,
        4 => GamepadButton::LeftShoulder,
        5 => GamepadButton::RightShoulder,
        6 => GamepadButton::LeftTrigger,
        7 => GamepadButton::RightTrigger,
        8 => GamepadButton::Select,
        9 => GamepadButton::Start,
        10 => GamepadButton::LeftStick,
        11 => GamepadButton::RightStick,
        12 => GamepadButton::DPadUp,
        13 => GamepadButton::DPadDown,
        14 => GamepadButton::DPadLeft,
        15 => GamepadButton::DPadRight,
        _ => GamepadButton::South,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn test_input_manager_creation() {
        let im = InputManager::new();
        assert_eq!(im.mouse_position(), (0.0, 0.0));
        assert!(!im.is_gamepad_connected());
    }

    #[test]
    fn test_key_press_and_release() {
        let mut im = InputManager::new();
        assert!(!im.is_key_down(KeyCode::Space));

        // Simulate a key press via winit event
        im.keyboard
            .key_states
            .insert(KeyCode::Space, ButtonState::Pressed);
        assert!(im.is_key_pressed(KeyCode::Space));
        assert!(im.is_key_down(KeyCode::Space));

        im.end_frame();
        assert!(!im.is_key_pressed(KeyCode::Space));
        assert!(im.is_key_down(KeyCode::Space));

        // Release
        im.keyboard
            .key_states
            .insert(KeyCode::Space, ButtonState::Released);
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

        // Simulate cursor move
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
        im.set_gamepad_connected(true);
        assert!(im.is_gamepad_connected());
    }

    #[test]
    fn test_gamepad_button() {
        let mut im = InputManager::new();
        assert!(!im.is_gamepad_button_down(GamepadButton::South));

        im.gamepad
            .button_states
            .insert(GamepadButton::South, ButtonState::Pressed);
        assert!(im.is_gamepad_button_pressed(GamepadButton::South));

        im.end_frame();
        assert!(!im.is_gamepad_button_pressed(GamepadButton::South));
        assert!(im.is_gamepad_button_down(GamepadButton::South));
    }

    #[test]
    fn test_gamepad_axis() {
        let mut im = InputManager::new();
        assert_eq!(im.gamepad_axis(GamepadAxis::LeftStickX), 0.0);

        im.gamepad.axes.insert(GamepadAxis::LeftStickX, 0.5);
        assert!((im.gamepad_axis(GamepadAxis::LeftStickX) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_raw_button_mapping() {
        assert_eq!(raw_button_to_gamepad(0), GamepadButton::South);
        assert_eq!(raw_button_to_gamepad(1), GamepadButton::East);
        assert_eq!(raw_button_to_gamepad(3), GamepadButton::North);
        assert_eq!(raw_button_to_gamepad(12), GamepadButton::DPadUp);
        assert_eq!(raw_button_to_gamepad(99), GamepadButton::South); // fallback
    }

    #[test]
    fn test_pressed_keys_iterator() {
        let mut im = InputManager::new();
        im.keyboard
            .key_states
            .insert(KeyCode::Space, ButtonState::Pressed);
        im.keyboard
            .key_states
            .insert(KeyCode::KeyW, ButtonState::Held);

        let pressed: Vec<KeyCode> = im.pressed_keys().collect();
        assert!(pressed.contains(&KeyCode::Space));
        assert!(pressed.contains(&KeyCode::KeyW));
    }

    #[test]
    fn test_end_frame_clears_states() {
        let mut im = InputManager::new();
        im.keyboard
            .key_states
            .insert(KeyCode::KeyA, ButtonState::Pressed);
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
}
