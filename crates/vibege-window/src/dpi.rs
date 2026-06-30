//! # DPI Manager
//!
//! Handles DPI scaling calculations across monitors.
//!
//! ## Architecture
//!
//! `DpiManager` provides:
//! - Logical ↔ physical pixel conversion
//! - Per-monitor DPI scale tracking
//! - Scale factor change notifications
//! - Recommended UI scale based on DPI

/// Manages DPI scaling calculations.
#[derive(Debug, Clone)]
pub struct DpiManager {
    /// Current DPI scale factor (1.0 = 96 DPI).
    scale_factor: f64,
    /// Logical width in device-independent pixels.
    logical_width: f64,
    /// Logical height in device-independent pixels.
    logical_height: f64,
}

impl DpiManager {
    /// Create a new DPI manager with given scale and logical dimensions.
    pub fn new(scale_factor: f64, logical_width: f64, logical_height: f64) -> Self {
        DpiManager {
            scale_factor,
            logical_width,
            logical_height,
        }
    }

    /// Update the scale factor (call on `ScaleFactorChanged`).
    pub fn set_scale_factor(&mut self, factor: f64) {
        self.scale_factor = factor;
    }

    /// Current DPI scale factor.
    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    /// Logical width.
    pub fn logical_width(&self) -> f64 {
        self.logical_width
    }

    /// Logical height.
    pub fn logical_height(&self) -> f64 {
        self.logical_height
    }

    /// Update logical dimensions.
    pub fn set_logical_size(&mut self, w: f64, h: f64) {
        self.logical_width = w;
        self.logical_height = h;
    }

    /// Convert logical pixels to physical pixels.
    pub fn logical_to_physical(&self, logical: f64) -> f64 {
        logical * self.scale_factor
    }

    /// Convert physical pixels to logical pixels.
    pub fn physical_to_logical(&self, physical: f64) -> f64 {
        physical / self.scale_factor
    }

    /// Convert a logical (width, height) to physical (width, height).
    pub fn logical_size_to_physical(&self, w: f64, h: f64) -> (u32, u32) {
        (
            (w * self.scale_factor).round() as u32,
            (h * self.scale_factor).round() as u32,
        )
    }

    /// Convert a physical (width, height) to logical (width, height).
    pub fn physical_size_to_logical(&self, w: u32, h: u32) -> (f64, f64) {
        (w as f64 / self.scale_factor, h as f64 / self.scale_factor)
    }

    /// Recommended UI scale multiplier for the current DPI.
    /// Returns 1.0 for 100%, 1.25 for 125%, 1.5 for 150%, 2.0 for 200%.
    pub fn recommended_ui_scale(&self) -> f64 {
        if self.scale_factor >= 2.0 {
            2.0
        } else if self.scale_factor >= 1.5 {
            1.5
        } else if self.scale_factor >= 1.25 {
            1.25
        } else {
            1.0
        }
    }

    /// DPI value (96 * scale_factor).
    pub fn dpi(&self) -> f64 {
        96.0 * self.scale_factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logical_to_physical() {
        let dpi = DpiManager::new(2.0, 800.0, 600.0);
        assert_eq!(dpi.logical_to_physical(100.0), 200.0);
        assert_eq!(dpi.physical_to_logical(200.0), 100.0);
    }

    #[test]
    fn test_physical_size_to_logical() {
        let dpi = DpiManager::new(1.5, 0.0, 0.0);
        let (lw, lh) = dpi.physical_size_to_logical(1920, 1080);
        assert!((lw - 1280.0).abs() < 0.1);
        assert!((lh - 720.0).abs() < 0.1);
    }

    #[test]
    fn test_recommended_ui_scale() {
        assert_eq!(DpiManager::new(1.0, 0.0, 0.0).recommended_ui_scale(), 1.0);
        assert_eq!(DpiManager::new(1.25, 0.0, 0.0).recommended_ui_scale(), 1.25);
        assert_eq!(DpiManager::new(1.5, 0.0, 0.0).recommended_ui_scale(), 1.5);
        assert_eq!(DpiManager::new(2.0, 0.0, 0.0).recommended_ui_scale(), 2.0);
    }

    #[test]
    fn test_dpi_value() {
        assert!((DpiManager::new(1.0, 0.0, 0.0).dpi() - 96.0).abs() < 0.01);
        assert!((DpiManager::new(2.0, 0.0, 0.0).dpi() - 192.0).abs() < 0.01);
    }

    #[test]
    fn test_set_scale_factor_updates() {
        let mut dpi = DpiManager::new(1.0, 800.0, 600.0);
        dpi.set_scale_factor(1.5);
        assert_eq!(dpi.scale_factor(), 1.5);
        assert_eq!(dpi.logical_to_physical(100.0), 150.0);
    }

    #[test]
    fn test_logical_size_to_physical_rounding() {
        let dpi = DpiManager::new(1.25, 0.0, 0.0);
        let (pw, ph) = dpi.logical_size_to_physical(800.0, 600.0);
        assert_eq!(pw, 1000);
        assert_eq!(ph, 750);
    }
}
