use serde::{Deserialize, Serialize};

use crate::validation::Validate;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeveloperConfig {
    pub debug_logging: bool,
    pub dev_mode: bool,
    pub show_fps: bool,
    pub show_metrics: bool,
}

impl Validate for DeveloperConfig {
    fn validate(&self) -> Result<(), Vec<String>> {
        Ok(())
    }

    fn sanitize(&mut self) {}
}
