//! Action mapping — bind logical actions to raw inputs.
//!
//! # Architecture
//!
//! Actions decouple game logic from raw key codes. Instead of checking
//! `is_key_down(KeyCode::Space)`, games query `action_state("jump")`.
//!
//! Bindings map actions → one or more raw inputs (keys, buttons, axes).
//! An action is active when ANY of its bound inputs is active.
//!
//! # Example
//!
//! ```ignore
//! let mut map = ActionMap::new();
//! map.bind_action("jump", ActionInput::Key(KeyCode::Space));
//! map.bind_action("jump", ActionInput::Key(KeyCode::ArrowUp));
//!
//! let state = map.action_state(&input_manager, "jump");
//! ```

use std::collections::HashMap;

use crate::{ButtonState, InputManager};
use winit::keyboard::KeyCode;

use crate::gamepad::{GamepadAxis, GamepadButton};
use crate::mouse::MouseButton;

/// The source of an action binding.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionInput {
    /// A keyboard key.
    Key(KeyCode),
    /// A mouse button.
    MouseButton(MouseButton),
    /// A gamepad button.
    GamepadButton(GamepadButton),
    /// A gamepad axis (digital threshold — active when |value| > threshold).
    GamepadAxis {
        axis: GamepadAxis,
        threshold: f32,
        polarity: AxisPolarity,
    },
    /// A chord (all inputs must be active simultaneously).
    Chord(Vec<ActionInput>),
}

/// Which direction of an axis triggers the action.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxisPolarity {
    Positive,
    Negative,
    Either,
}

/// An analog axis binding, returning a value in [-1.0, 1.0].
#[derive(Debug, Clone, PartialEq)]
pub struct AxisBinding {
    pub source: AxisSource,
    pub scale: f32,
    pub dead_zone: f32,
    pub inversion: f32, // 1.0 or -1.0
}

/// Sources for an analog axis.
#[derive(Debug, Clone, PartialEq)]
pub enum AxisSource {
    /// A gamepad axis directly.
    Gamepad(GamepadAxis),
    /// Two keys forming a digital axis (e.g. A/D → -1/+1).
    DigitalAxis {
        negative: KeyCode,
        positive: KeyCode,
    },
    /// Mouse delta on an axis.
    MouseDelta { axis: MouseAxis },
    /// Mouse position on an axis (range 0..1 normalized).
    MousePosition { axis: MouseAxis },
}

/// Which mouse axis to use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseAxis {
    X,
    Y,
}

impl AxisBinding {
    /// Create a new axis binding with defaults (scale=1, dead_zone=0.1, no inversion).
    pub fn new(source: AxisSource) -> Self {
        Self {
            source,
            scale: 1.0,
            dead_zone: 0.1,
            inversion: 1.0,
        }
    }
}

/// A map of action names to their bindings.
///
/// Actions are resolved against an `InputManager` to determine their
/// current state (Pressed/Held/Released/Idle).
#[derive(Debug, Clone)]
pub struct ActionMap {
    actions: HashMap<String, Vec<ActionInput>>,
    axes: HashMap<String, Vec<AxisBinding>>,
}

impl ActionMap {
    /// Create an empty action map.
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
            axes: HashMap::new(),
        }
    }

    /// Bind a raw input to an action. Multiple inputs per action are OR'd.
    pub fn bind_action(&mut self, name: &str, input: ActionInput) {
        self.actions
            .entry(name.to_string())
            .or_default()
            .push(input);
    }

    /// Bind an axis source to a named axis. Multiple sources are averaged.
    pub fn bind_axis(&mut self, name: &str, binding: AxisBinding) {
        self.axes.entry(name.to_string()).or_default().push(binding);
    }

    /// Remove all bindings for an action.
    pub fn unbind_action(&mut self, name: &str) {
        self.actions.remove(name);
    }

    /// Remove all bindings for an axis.
    pub fn unbind_axis(&mut self, name: &str) {
        self.axes.remove(name);
    }

    /// Get the current state of a named action.
    pub fn action_state(&self, input: &InputManager, name: &str) -> ButtonState {
        let Some(bindings) = self.actions.get(name) else {
            return ButtonState::Idle;
        };

        let mut state = ButtonState::Idle;
        for binding in bindings {
            let s = resolve_action_input(input, binding);
            // Promote: Idle < Released < Pressed < Held
            state = merge_button_states(state, s);
        }
        state
    }

    /// Check if an action is active (Pressed or Held).
    pub fn is_action_active(&self, input: &InputManager, name: &str) -> bool {
        let s = self.action_state(input, name);
        s == ButtonState::Pressed || s == ButtonState::Held
    }

    /// Check if an action was just pressed.
    pub fn is_action_pressed(&self, input: &InputManager, name: &str) -> bool {
        self.action_state(input, name) == ButtonState::Pressed
    }

    /// Get the current value of a named axis.
    pub fn axis_value(&self, input: &InputManager, name: &str) -> f32 {
        let Some(bindings) = self.axes.get(name) else {
            return 0.0;
        };
        if bindings.is_empty() {
            return 0.0;
        }
        let sum: f32 = bindings
            .iter()
            .map(|b| resolve_axis_binding(input, b))
            .sum();
        sum / bindings.len() as f32
    }

    /// Returns `true` if the named action has any bindings.
    pub fn has_action(&self, name: &str) -> bool {
        self.actions.contains_key(name)
    }

    /// Returns `true` if the named axis has any bindings.
    pub fn has_axis(&self, name: &str) -> bool {
        self.axes.contains_key(name)
    }

    /// List all bound action names.
    pub fn action_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.actions.keys().map(|s| s.as_str())
    }

    /// List all bound axis names.
    pub fn axis_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.axes.keys().map(|s| s.as_str())
    }

    /// Check for conflicting bindings (same input bound to multiple actions).
    pub fn conflicts(&self) -> Vec<(ActionInput, Vec<String>)> {
        let mut input_to_actions: HashMap<String, Vec<String>> = HashMap::new();
        for (name, bindings) in &self.actions {
            for b in bindings {
                let key = format!("{b:?}");
                input_to_actions.entry(key).or_default().push(name.clone());
            }
        }
        input_to_actions
            .into_iter()
            .filter(|(_, names)| names.len() > 1)
            .map(|(key, names)| {
                // Parse the key back to a representative ActionInput
                let input = self
                    .actions
                    .values()
                    .flatten()
                    .find(|a| format!("{a:?}") == key);
                (
                    input.cloned().unwrap_or(ActionInput::Key(KeyCode::Space)),
                    names,
                )
            })
            .collect()
    }
}

impl Default for ActionMap {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_action_input(input: &InputManager, ai: &ActionInput) -> ButtonState {
    match ai {
        ActionInput::Key(kc) => input.key_state(*kc),
        ActionInput::MouseButton(btn) => input.mouse_button_state(*btn),
        ActionInput::GamepadButton(gb) => input.gamepad_button_state(*gb),
        ActionInput::GamepadAxis {
            axis,
            threshold,
            polarity,
        } => {
            let val = input.gamepad_axis(*axis) as f32;
            let active = match polarity {
                AxisPolarity::Positive => val > *threshold,
                AxisPolarity::Negative => val < -threshold,
                AxisPolarity::Either => val.abs() > *threshold,
            };
            if active {
                ButtonState::Held
            } else {
                ButtonState::Idle
            }
        }
        ActionInput::Chord(inputs) => {
            if inputs.is_empty() {
                return ButtonState::Idle;
            }
            let mut result = ButtonState::Pressed;
            for sub in inputs {
                let s = resolve_action_input(input, sub);
                if s == ButtonState::Idle || s == ButtonState::Released {
                    return ButtonState::Idle;
                }
                // Take the least active state
                if s == ButtonState::Held {
                    result = ButtonState::Held;
                }
            }
            result
        }
    }
}

fn resolve_axis_binding(input: &InputManager, binding: &AxisBinding) -> f32 {
    let raw = match &binding.source {
        AxisSource::Gamepad(axis) => input.gamepad_axis(*axis) as f32,
        AxisSource::DigitalAxis { negative, positive } => {
            let neg = input.is_key_down(*negative) as i32 as f32;
            let pos = input.is_key_down(*positive) as i32 as f32;
            pos - neg
        }
        AxisSource::MouseDelta { axis } => match axis {
            MouseAxis::X => input.mouse_delta().0 as f32,
            MouseAxis::Y => input.mouse_delta().1 as f32,
        },
        AxisSource::MousePosition { axis } => {
            let (pos, _size) = match axis {
                MouseAxis::X => (input.mouse_position().0 as f32, 800.0), // normalized by screen width (caller should provide)
                MouseAxis::Y => (input.mouse_position().1 as f32, 600.0),
            };
            pos // raw pixel position; caller should normalize
        }
    };

    // Apply dead zone
    let processed = if raw.abs() < binding.dead_zone {
        0.0
    } else {
        raw
    };

    processed * binding.scale * binding.inversion
}

fn merge_button_states(a: ButtonState, b: ButtonState) -> ButtonState {
    use ButtonState::*;
    match (a, b) {
        (Held, _) | (_, Held) => Held,
        (Pressed, _) | (_, Pressed) => Pressed,
        (Released, _) | (_, Released) => Released,
        _ => Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    fn make_input() -> InputManager {
        InputManager::new()
    }

    #[test]
    fn test_action_map_empty() {
        let map = ActionMap::new();
        let input = make_input();
        assert_eq!(map.action_state(&input, "nonexistent"), ButtonState::Idle);
        assert!(!map.is_action_active(&input, "nonexistent"));
    }

    #[test]
    fn test_action_binding_and_query() {
        let mut map = ActionMap::new();
        map.bind_action("jump", ActionInput::Key(KeyCode::Space));
        let mut input = make_input();

        // Not pressed yet
        assert_eq!(map.action_state(&input, "jump"), ButtonState::Idle);

        // Press
        input.set_key_state(KeyCode::Space, ButtonState::Pressed);
        assert_eq!(map.action_state(&input, "jump"), ButtonState::Pressed);
        assert!(map.is_action_pressed(&input, "jump"));
        assert!(map.is_action_active(&input, "jump"));

        // Held after end_frame
        input.end_frame();
        assert_eq!(map.action_state(&input, "jump"), ButtonState::Held);
        assert!(!map.is_action_pressed(&input, "jump"));
        assert!(map.is_action_active(&input, "jump"));

        // Release
        input.set_key_state(KeyCode::Space, ButtonState::Released);
        assert!(!map.is_action_active(&input, "jump"));
    }

    #[test]
    fn test_action_or_binding() {
        let mut map = ActionMap::new();
        map.bind_action("move_up", ActionInput::Key(KeyCode::KeyW));
        map.bind_action("move_up", ActionInput::Key(KeyCode::ArrowUp));

        let mut input = make_input();
        assert!(!map.is_action_active(&input, "move_up"));

        // W key activates the action
        input.set_key_state(KeyCode::KeyW, ButtonState::Pressed);
        assert!(map.is_action_active(&input, "move_up"));

        input.end_frame();
        input.end_frame(); // clear Pressed

        // ArrowUp also activates the action
        input.set_key_state(KeyCode::ArrowUp, ButtonState::Pressed);
        assert!(map.is_action_active(&input, "move_up"));
    }

    #[test]
    fn test_action_names() {
        let mut map = ActionMap::new();
        map.bind_action("jump", ActionInput::Key(KeyCode::Space));
        map.bind_action("shoot", ActionInput::MouseButton(MouseButton::Left));

        let names: Vec<&str> = map.action_names().collect();
        assert!(names.contains(&"jump"));
        assert!(names.contains(&"shoot"));
    }

    #[test]
    fn test_unbind_action() {
        let mut map = ActionMap::new();
        map.bind_action("temp", ActionInput::Key(KeyCode::KeyT));
        assert!(map.action_names().any(|n| n == "temp"));
        map.unbind_action("temp");
        assert!(!map.action_names().any(|n| n == "temp"));
    }

    #[test]
    fn test_digital_axis() {
        let mut map = ActionMap::new();
        map.bind_axis(
            "move_x",
            AxisBinding::new(AxisSource::DigitalAxis {
                negative: KeyCode::KeyA,
                positive: KeyCode::KeyD,
            }),
        );

        let mut input = make_input();
        assert!((map.axis_value(&input, "move_x") - 0.0).abs() < 1e-6);

        input.set_key_state(KeyCode::KeyD, ButtonState::Held);
        let val = map.axis_value(&input, "move_x");
        assert!((val - 1.0).abs() < 1e-6, "Expected 1.0, got {val}");

        input.set_key_state(KeyCode::KeyD, ButtonState::Released);
        input.end_frame();
        input.end_frame(); // clear
        input.set_key_state(KeyCode::KeyA, ButtonState::Held);
        let val = map.axis_value(&input, "move_x");
        assert!((val - (-1.0)).abs() < 1e-6, "Expected -1.0, got {val}");
    }

    #[test]
    fn test_conflict_detection() {
        let mut map = ActionMap::new();
        map.bind_action("jump", ActionInput::Key(KeyCode::Space));
        map.bind_action("pause", ActionInput::Key(KeyCode::Space));
        let conflicts = map.conflicts();
        assert!(!conflicts.is_empty());
    }

    #[test]
    fn test_no_conflicts_unique_bindings() {
        let mut map = ActionMap::new();
        map.bind_action("jump", ActionInput::Key(KeyCode::Space));
        map.bind_action("shoot", ActionInput::MouseButton(MouseButton::Left));
        assert!(map.conflicts().is_empty());
    }

    #[test]
    fn test_axis_names() {
        let mut map = ActionMap::new();
        map.bind_axis(
            "move_x",
            AxisBinding::new(AxisSource::Gamepad(GamepadAxis::LeftStickX)),
        );
        map.bind_axis(
            "move_y",
            AxisBinding::new(AxisSource::Gamepad(GamepadAxis::LeftStickY)),
        );

        let names: Vec<&str> = map.axis_names().collect();
        assert!(names.contains(&"move_x"));
        assert!(names.contains(&"move_y"));
    }

    #[test]
    fn test_axis_unbind() {
        let mut map = ActionMap::new();
        map.bind_axis(
            "temp",
            AxisBinding::new(AxisSource::Gamepad(GamepadAxis::LeftStickX)),
        );
        assert!(map.axis_names().any(|n| n == "temp"));
        map.unbind_axis("temp");
        assert!(!map.axis_names().any(|n| n == "temp"));
    }

    #[test]
    fn test_chord_active() {
        use ActionInput as AI;
        let mut map = ActionMap::new();
        map.bind_action(
            "save",
            AI::Chord(vec![AI::Key(KeyCode::ControlLeft), AI::Key(KeyCode::KeyS)]),
        );

        let mut input = make_input();
        assert!(!map.is_action_active(&input, "save"));

        input.set_key_state(KeyCode::ControlLeft, ButtonState::Held);
        assert!(!map.is_action_active(&input, "save"));

        input.set_key_state(KeyCode::KeyS, ButtonState::Pressed);
        assert!(map.is_action_active(&input, "save"));
    }

    #[test]
    fn test_chord_inactive_when_one_missing() {
        use ActionInput as AI;
        let mut map = ActionMap::new();
        map.bind_action(
            "save",
            AI::Chord(vec![AI::Key(KeyCode::ControlLeft), AI::Key(KeyCode::KeyS)]),
        );

        let mut input = make_input();
        input.set_key_state(KeyCode::ControlLeft, ButtonState::Held);
        // KeyS not pressed
        assert!(!map.is_action_active(&input, "save"));
    }

    #[test]
    fn test_gamepad_axis_threshold_action() {
        use ActionInput as AI;
        let mut map = ActionMap::new();
        map.bind_action(
            "trigger",
            AI::GamepadAxis {
                axis: GamepadAxis::RightTrigger,
                threshold: 0.5,
                polarity: AxisPolarity::Positive,
            },
        );

        let mut input = make_input();
        input.set_gamepad_axis(GamepadAxis::RightTrigger, 0.3);
        assert!(!map.is_action_active(&input, "trigger"));

        input.set_gamepad_axis(GamepadAxis::RightTrigger, 0.7);
        assert!(map.is_action_active(&input, "trigger"));
    }
}
