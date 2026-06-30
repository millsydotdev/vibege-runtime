use std::sync::Mutex;
use vibege_input::InputManager;

/// Caches pressed state for a set of keys.
/// Locks the input Mutex once per frame instead of once per key check.
pub struct InputState {
    pressed: Vec<bool>,
}

impl InputState {
    pub fn new(input: &Mutex<InputManager>, keys: &[&str]) -> Self {
        let lock = input.lock().expect("Input lock");
        let pressed: Vec<bool> = keys
            .iter()
            .map(|k| lock.is_key_pressed(vibege_input::key_name_to_code(k)))
            .collect();
        Self { pressed }
    }

    pub fn pressed(&self, index: usize) -> bool {
        self.pressed.get(index).copied().unwrap_or(false)
    }
}
