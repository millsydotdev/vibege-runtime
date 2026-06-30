use serde::{Deserialize, Serialize};

use crate::validation::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub volume: f32,
    pub muted: bool,
    pub music_volume: f32,
    pub sfx_volume: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            volume: 0.7,
            muted: false,
            music_volume: 0.7,
            sfx_volume: 0.8,
        }
    }
}

impl Validate for AudioConfig {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if !(0.0..=1.0).contains(&self.volume) {
            errors.push(format!("audio.volume must be 0.0–1.0, got {}", self.volume));
        }
        if !(0.0..=1.0).contains(&self.music_volume) {
            errors.push(format!(
                "audio.music_volume must be 0.0–1.0, got {}",
                self.music_volume
            ));
        }
        if !(0.0..=1.0).contains(&self.sfx_volume) {
            errors.push(format!(
                "audio.sfx_volume must be 0.0–1.0, got {}",
                self.sfx_volume
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn sanitize(&mut self) {
        self.volume = self.volume.clamp(0.0, 1.0);
        self.music_volume = self.music_volume.clamp(0.0, 1.0);
        self.sfx_volume = self.sfx_volume.clamp(0.0, 1.0);
    }
}
