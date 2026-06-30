use serde::{Deserialize, Serialize};

use crate::validation::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InputConfig {
    pub mouse_sensitivity: f64,
    pub invert_y: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 1.0,
            invert_y: false,
        }
    }
}

impl Validate for InputConfig {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if !(0.1..=5.0).contains(&self.mouse_sensitivity) {
            errors.push(format!(
                "input.mouse_sensitivity ({}) out of range 0.1–5.0",
                self.mouse_sensitivity
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn sanitize(&mut self) {
        self.mouse_sensitivity = self.mouse_sensitivity.clamp(0.1, 5.0);
    }
}
