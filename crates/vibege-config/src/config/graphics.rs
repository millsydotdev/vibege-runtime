use serde::{Deserialize, Serialize};

use crate::validation::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsConfig {
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub fps_limit: u32,
    pub fullscreen: bool,
    pub borderless: bool,
    pub dpi_scale: f64,
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            vsync: true,
            fps_limit: 0,
            fullscreen: false,
            borderless: false,
            dpi_scale: 1.0,
        }
    }
}

impl Validate for GraphicsConfig {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.width < 320 || self.width > 7680 {
            errors.push(format!(
                "graphics.width ({}) out of range 320–7680",
                self.width
            ));
        }
        if self.height < 200 || self.height > 4320 {
            errors.push(format!(
                "graphics.height ({}) out of range 200–4320",
                self.height
            ));
        }
        if self.fps_limit > 360 {
            errors.push(format!(
                "graphics.fps_limit ({}) exceeds 360",
                self.fps_limit
            ));
        }
        if self.dpi_scale < 0.5 || self.dpi_scale > 4.0 {
            errors.push(format!(
                "graphics.dpi_scale ({}) out of range 0.5–4.0",
                self.dpi_scale
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn sanitize(&mut self) {
        self.width = self.width.clamp(320, 7680);
        self.height = self.height.clamp(200, 4320);
        self.fps_limit = self.fps_limit.min(360);
        self.dpi_scale = self.dpi_scale.clamp(0.5, 4.0);
    }
}
