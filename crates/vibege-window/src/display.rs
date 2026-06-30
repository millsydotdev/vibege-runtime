//! # Display Manager
//!
//! Monitor abstraction for multi-monitor support.
//!
//! ## Architecture
//!
//! `DisplayManager` wraps winit's monitor enumeration and provides:
//! - Primary / secondary monitor queries
//! - Monitor dimensions and work area
//! - DPI scaling per monitor
//! - Hot-plug detection (via winit events)
//! - Monitor name and identification

use tracing::debug;
use winit::monitor::MonitorHandle;
use winit::window::Window;

/// Information about a single display / monitor.
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    /// Human-readable name (e.g. "DELL U2723QE").
    pub name: String,
    /// Physical size in millimetres.
    pub size_mm: (u32, u32),
    /// Logical resolution in pixels.
    pub resolution: (u32, u32),
    /// Position of the top-left corner in virtual screen space.
    pub position: (i32, i32),
    /// DPI scale factor (1.0 = 96 DPI, 1.25 = 120 DPI, 2.0 = 192 DPI).
    pub scale_factor: f64,
    /// Whether this is the primary monitor.
    pub is_primary: bool,
    /// Refresh rate in Hz, if available.
    pub refresh_rate: Option<u32>,
}

/// Manager for multi-monitor queries and monitoring.
///
/// Created with a reference to a Window to query the OS monitor list.
/// This is required because winit 0.30 requires an `ActiveEventLoop`
/// or `Window` to enumerate monitors.
#[derive(Debug)]
pub struct DisplayManager {
    monitors: Vec<DisplayInfo>,
    primary_index: usize,
}

impl DisplayManager {
    /// Enumerate all available monitors using a window reference.
    pub fn new(window: &Window) -> Self {
        let primary = window.primary_monitor();
        let monitors: Vec<MonitorHandle> = window.available_monitors().collect();

        let infos: Vec<DisplayInfo> = monitors
            .iter()
            .map(|m| {
                let name = m.name().unwrap_or_else(|| "Unknown".to_string());
                let size = m.size();
                let position = m.position();
                let scale = m.scale_factor();
                let is_primary = primary.as_ref() == Some(m);

                DisplayInfo {
                    name,
                    size_mm: (size.width, size.height),
                    resolution: (size.width, size.height),
                    position: (position.x, position.y),
                    scale_factor: scale,
                    is_primary,
                    refresh_rate: None,
                }
            })
            .collect();

        let primary_index = infos.iter().position(|m| m.is_primary).unwrap_or(0);

        debug!(count = infos.len(), "Display manager initialised");

        DisplayManager {
            monitors: infos,
            primary_index,
        }
    }

    /// Re-scan monitors (call on winit `ScaleFactorChanged` or monitor events).
    pub fn refresh(&mut self, window: &Window) {
        let primary = window.primary_monitor();
        let monitors: Vec<MonitorHandle> = window.available_monitors().collect();

        self.monitors = monitors
            .iter()
            .map(|m| {
                let name = m.name().unwrap_or_else(|| "Unknown".to_string());
                let size = m.size();
                let position = m.position();
                let scale = m.scale_factor();
                let is_primary = primary.as_ref() == Some(m);

                DisplayInfo {
                    name,
                    size_mm: (size.width, size.height),
                    resolution: (size.width, size.height),
                    position: (position.x, position.y),
                    scale_factor: scale,
                    is_primary,
                    refresh_rate: None,
                }
            })
            .collect();

        self.primary_index = self.monitors.iter().position(|m| m.is_primary).unwrap_or(0);
        debug!(count = self.monitors.len(), "Displays refreshed");
    }

    /// Return all known monitors.
    pub fn monitors(&self) -> &[DisplayInfo] {
        &self.monitors
    }

    /// Return info for the primary monitor.
    pub fn primary(&self) -> Option<&DisplayInfo> {
        self.monitors.get(self.primary_index)
    }

    /// Find the monitor containing the given point (virtual screen coords).
    pub fn monitor_at(&self, x: i32, y: i32) -> Option<&DisplayInfo> {
        self.monitors.iter().find(|m| {
            let (mx, my) = m.position;
            let (mw, mh) = (m.resolution.0 as i32, m.resolution.1 as i32);
            x >= mx && x < mx + mw && y >= my && y < my + mh
        })
    }

    /// Find the monitor that best matches the given name.
    pub fn monitor_named(&self, name: &str) -> Option<&DisplayInfo> {
        self.monitors.iter().find(|m| m.name == name)
    }

    /// Return the number of connected monitors.
    pub fn count(&self) -> usize {
        self.monitors.len()
    }

    /// Detect whether the monitor setup changed since last refresh.
    pub fn detect_change(&self, other: &DisplayManager) -> bool {
        self.monitors.len() != other.monitors.len()
            || self
                .monitors
                .iter()
                .zip(other.monitors.iter())
                .any(|(a, b)| a.name != b.name || a.resolution != b.resolution)
    }
}

/// Find a safe window position within the bounds of available monitors.
///
/// Clamps the given (x, y, width, height) to ensure the window title bar
/// is visible on at least one monitor.
pub fn clamp_to_visible(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    displays: &DisplayManager,
) -> (i32, i32) {
    let w = width as i32;
    let h = height as i32;

    let overlaps = displays.monitors().iter().any(|m| {
        let (mx, my) = m.position;
        let (mw, mh) = (m.resolution.0 as i32, m.resolution.1 as i32);
        x + w > mx && x < mx + mw && y + h > my && y < my + mh
    });

    if overlaps {
        (x, y)
    } else if let Some(primary) = displays.primary() {
        let cx = primary.position.0 + (primary.resolution.0 as i32 - w) / 2;
        let cy = primary.position.1 + (primary.resolution.1 as i32 - h) / 2;
        (cx.max(0), cy.max(0))
    } else {
        (0, 0)
    }
}

/// Smart-centre a window on a specific monitor (or primary if None).
pub fn centre_on_monitor(width: u32, height: u32, monitor: Option<&DisplayInfo>) -> (i32, i32) {
    let Some(m) = monitor else {
        return (0, 0);
    };
    let cx = m.position.0 + (m.resolution.0 as i32 - width as i32) / 2;
    let cy = m.position.1 + (m.resolution.1 as i32 - height as i32) / 2;
    (cx.max(m.position.0), cy.max(m.position.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centre_on_monitor() {
        let info = DisplayInfo {
            name: "Test".into(),
            size_mm: (500, 300),
            resolution: (1920, 1080),
            position: (0, 0),
            scale_factor: 1.0,
            is_primary: true,
            refresh_rate: Some(60),
        };
        let (cx, cy) = centre_on_monitor(800, 600, Some(&info));
        assert_eq!(cx, (1920 - 800) / 2);
        assert_eq!(cy, (1080 - 600) / 2);
    }

    #[test]
    fn test_centre_on_monitor_none() {
        let (cx, cy) = centre_on_monitor(800, 600, None);
        assert_eq!(cx, 0);
        assert_eq!(cy, 0);
    }

    #[test]
    fn test_display_info_creation() {
        let info = DisplayInfo {
            name: "Primary".into(),
            size_mm: (500, 300),
            resolution: (1920, 1080),
            position: (0, 0),
            scale_factor: 1.5,
            is_primary: true,
            refresh_rate: Some(144),
        };
        assert_eq!(info.name, "Primary");
        assert_eq!(info.scale_factor, 1.5);
        assert_eq!(info.refresh_rate, Some(144));
    }

    #[test]
    fn test_monitor_at_returns_none_for_out_of_bounds() {
        let info = DisplayInfo {
            name: "Primary".into(),
            size_mm: (500, 300),
            resolution: (1920, 1080),
            position: (0, 0),
            scale_factor: 1.0,
            is_primary: true,
            refresh_rate: Some(60),
        };
        let dm = DisplayManager {
            monitors: vec![info],
            primary_index: 0,
        };
        assert!(dm.monitor_at(9999, 9999).is_none());
        assert!(dm.monitor_at(100, 100).is_some());
    }

    #[test]
    fn test_primary_returns_first_primary() {
        let d1 = DisplayInfo {
            name: "Secondary".into(),
            size_mm: (500, 300),
            resolution: (1920, 1080),
            position: (1920, 0),
            scale_factor: 1.0,
            is_primary: false,
            refresh_rate: Some(60),
        };
        let d2 = DisplayInfo {
            name: "Primary".into(),
            size_mm: (500, 300),
            resolution: (1920, 1080),
            position: (0, 0),
            scale_factor: 1.0,
            is_primary: true,
            refresh_rate: Some(60),
        };
        let dm = DisplayManager {
            monitors: vec![d1, d2],
            primary_index: 1,
        };
        assert_eq!(dm.primary().unwrap().name, "Primary");
    }

    #[test]
    fn test_monitor_named() {
        let info = DisplayInfo {
            name: "DELL".into(),
            size_mm: (500, 300),
            resolution: (1920, 1080),
            position: (0, 0),
            scale_factor: 1.0,
            is_primary: true,
            refresh_rate: None,
        };
        let dm = DisplayManager {
            monitors: vec![info],
            primary_index: 0,
        };
        assert!(dm.monitor_named("DELL").is_some());
        assert!(dm.monitor_named("Other").is_none());
    }

    #[test]
    fn test_clamp_to_visible_without_window() {
        let dm = DisplayManager {
            monitors: vec![],
            primary_index: 0,
        };
        let (x, y) = clamp_to_visible(-9999, -9999, 800, 600, &dm);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
    }

    #[test]
    fn test_detect_change() {
        let dm1 = DisplayManager {
            monitors: vec![DisplayInfo {
                name: "A".into(),
                size_mm: (500, 300),
                resolution: (1920, 1080),
                position: (0, 0),
                scale_factor: 1.0,
                is_primary: true,
                refresh_rate: None,
            }],
            primary_index: 0,
        };
        let dm2 = DisplayManager {
            monitors: vec![DisplayInfo {
                name: "B".into(),
                size_mm: (500, 300),
                resolution: (1920, 1080),
                position: (0, 0),
                scale_factor: 1.0,
                is_primary: true,
                refresh_rate: None,
            }],
            primary_index: 0,
        };
        assert!(dm1.detect_change(&dm2));
        assert!(!dm1.detect_change(&dm1));
    }
}
