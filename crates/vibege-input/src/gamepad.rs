//! Gamepad state — multi-controller support, dead zones, axis scaling.

use std::collections::HashMap;

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

/// Configuration for a gamepad axis.
#[derive(Debug, Clone, Copy)]
pub struct AxisConfig {
    /// Raw values below this are zeroed.
    pub dead_zone: f32,
    /// Sensitivity multiplier.
    pub sensitivity: f32,
    /// Inversion (1.0 or -1.0).
    pub inversion: f32,
}

impl Default for AxisConfig {
    fn default() -> Self {
        Self {
            dead_zone: 0.1,
            sensitivity: 1.0,
            inversion: 1.0,
        }
    }
}

/// State for a single connected gamepad.
#[derive(Debug, Clone)]
pub(crate) struct PadState {
    pub connected: bool,
    pub button_states: HashMap<GamepadButton, super::ButtonState>,
    pub axes: HashMap<GamepadAxis, f64>,
    pub name: Option<String>,
    #[allow(dead_code)]
    pub axis_configs: HashMap<GamepadAxis, AxisConfig>,
}

impl PadState {
    pub fn new() -> Self {
        let mut axis_configs = HashMap::new();
        for axis in &[
            GamepadAxis::LeftStickX,
            GamepadAxis::LeftStickY,
            GamepadAxis::RightStickX,
            GamepadAxis::RightStickY,
            GamepadAxis::LeftTrigger,
            GamepadAxis::RightTrigger,
        ] {
            axis_configs.insert(*axis, AxisConfig::default());
        }
        Self {
            connected: false,
            button_states: HashMap::new(),
            axes: HashMap::new(),
            name: None,
            axis_configs,
        }
    }

    /// Apply dead zone and sensitivity to a raw axis value.
    pub fn process_axis(&self, axis: &GamepadAxis, raw: f64) -> f64 {
        let cfg = self.axis_configs.get(axis).copied().unwrap_or_default();
        let val = raw as f32;
        let deadened = if val.abs() < cfg.dead_zone {
            0.0
        } else {
            // Rescale so that the value at the dead_zone edge is 0
            let sign = val.signum();
            let magnitude = (val.abs() - cfg.dead_zone) / (1.0 - cfg.dead_zone);
            sign * magnitude.max(0.0)
        };
        (deadened * cfg.sensitivity * cfg.inversion) as f64
    }
}

/// State for the gamepad system.
#[derive(Debug, Clone)]
pub(crate) struct GamepadSystem {
    pub pads: Vec<PadState>,
}

impl GamepadSystem {
    pub fn new() -> Self {
        // Pre-allocate slots for up to 4 controllers
        Self {
            pads: vec![
                PadState::new(),
                PadState::new(),
                PadState::new(),
                PadState::new(),
            ],
        }
    }

    /// Total connected gamepads.
    pub fn connected_count(&self) -> usize {
        self.pads.iter().filter(|p| p.connected).count()
    }

    /// True if at least one gamepad is connected.
    pub fn any_connected(&self) -> bool {
        self.pads.iter().any(|p| p.connected)
    }

    /// Mark a gamepad slot as connected/disconnected.
    pub fn set_connected(&mut self, slot: usize, connected: bool) {
        if slot < self.pads.len() {
            self.pads[slot].connected = connected;
            if !connected {
                self.pads[slot].button_states.clear();
                self.pads[slot].axes.clear();
            }
        }
    }

    /// Get a pad state by slot. Returns None if slot is out of range.
    pub fn get(&self, slot: usize) -> Option<&PadState> {
        self.pads.get(slot)
    }

    /// Get a pad state by slot (mutable).
    pub fn get_mut(&mut self, slot: usize) -> Option<&mut PadState> {
        self.pads.get_mut(slot)
    }

    /// Process a raw axis value through the pad's config.
    #[allow(dead_code)]
    pub fn process_axis(&self, slot: usize, axis: &GamepadAxis, raw: f64) -> f64 {
        self.pads
            .get(slot)
            .map(|p| p.process_axis(axis, raw))
            .unwrap_or(0.0)
    }
}

/// Convert a raw button number to a `GamepadButton`.
pub fn raw_button_to_gamepad(button: u16) -> GamepadButton {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_state_new() {
        let pad = PadState::new();
        assert!(!pad.connected);
        assert!(pad.button_states.is_empty());
        assert!(pad.axes.is_empty());
        assert_eq!(pad.axis_configs.len(), 6);
    }

    #[test]
    fn test_dead_zone_below_threshold() {
        let pad = PadState::new();
        let result = pad.process_axis(&GamepadAxis::LeftStickX, 0.05);
        assert!((result - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_dead_zone_above_threshold() {
        let pad = PadState::new();
        let result = pad.process_axis(&GamepadAxis::LeftStickX, 0.5);
        // raw 0.5, dead_zone 0.1 → (0.5 - 0.1) / (1.0 - 0.1) = 0.4/0.9 ≈ 0.444
        assert!(
            (result - 0.444).abs() < 0.01,
            "Expected ~0.444, got {result}"
        );
    }

    #[test]
    fn test_dead_zone_full_value() {
        let pad = PadState::new();
        let result = pad.process_axis(&GamepadAxis::LeftStickX, 1.0);
        assert!((result - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_negative_axis() {
        let pad = PadState::new();
        let result = pad.process_axis(&GamepadAxis::LeftStickY, -0.5);
        assert!(result < 0.0);
    }

    #[test]
    fn test_gamepad_system_new() {
        let sys = GamepadSystem::new();
        assert_eq!(sys.pads.len(), 4);
        assert_eq!(sys.connected_count(), 0);
    }

    #[test]
    fn test_gamepad_connect_disconnect() {
        let mut sys = GamepadSystem::new();
        sys.set_connected(0, true);
        assert_eq!(sys.connected_count(), 1);
        assert!(sys.pads[0].connected);

        sys.set_connected(0, false);
        assert_eq!(sys.connected_count(), 0);
    }

    #[test]
    fn test_gamepad_slot_out_of_range() {
        let sys = GamepadSystem::new();
        assert!(sys.get(99).is_none());
    }

    #[test]
    fn test_raw_button_mapping() {
        assert_eq!(raw_button_to_gamepad(0), GamepadButton::South);
        assert_eq!(raw_button_to_gamepad(1), GamepadButton::East);
        assert_eq!(raw_button_to_gamepad(3), GamepadButton::North);
        assert_eq!(raw_button_to_gamepad(12), GamepadButton::DPadUp);
        assert_eq!(raw_button_to_gamepad(13), GamepadButton::DPadDown);
        assert_eq!(raw_button_to_gamepad(99), GamepadButton::South);
    }

    #[test]
    fn test_axis_config_custom_dead_zone() {
        let mut pad = PadState::new();
        let cfg = AxisConfig {
            dead_zone: 0.3,
            sensitivity: 1.0,
            inversion: 1.0,
        };
        pad.axis_configs.insert(GamepadAxis::LeftStickX, cfg);

        let result = pad.process_axis(&GamepadAxis::LeftStickX, 0.2);
        assert!((result - 0.0).abs() < 1e-6);

        let result = pad.process_axis(&GamepadAxis::LeftStickX, 0.5);
        // (0.5 - 0.3) / (1.0 - 0.3) = 0.2/0.7 ≈ 0.286
        assert!((result - 0.286).abs() < 0.01);
    }

    #[test]
    fn test_sensitivity_inversion() {
        let mut pad = PadState::new();
        let cfg = AxisConfig {
            dead_zone: 0.0,
            sensitivity: 2.0,
            inversion: -1.0,
        };
        pad.axis_configs.insert(GamepadAxis::LeftStickX, cfg);

        let result = pad.process_axis(&GamepadAxis::LeftStickX, 0.5);
        assert!(
            (result - (-1.0)).abs() < 0.01,
            "Expected ~-1.0, got {result}"
        );
    }

    #[test]
    fn test_any_connected() {
        let mut sys = GamepadSystem::new();
        assert!(!sys.any_connected());
        sys.set_connected(2, true);
        assert!(sys.any_connected());
    }
}
