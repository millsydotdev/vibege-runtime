//! # Overlay Manager
//!
//! Manages the overlay window lifecycle, positioning, and state.
//!
//! ## Architecture
//!
//! The `OverlayManager` tracks:
//! - Current position and size (with persistence to config)
//! - Last-known monitor (for multi-monitor restoration)
//! - Visibility state
//! - Overlay mode (always-on-top, normal)
//!
//! It provides safe bounds checking so the overlay always appears
//! on a valid monitor, even after hot-plug events.

use tracing::debug;
use winit::window::Window;

use crate::display::{DisplayManager, centre_on_monitor, clamp_to_visible};

/// Modes the overlay window can operate in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayMode {
    /// Always-on-top overlay (default).
    #[default]
    AlwaysOnTop,
    /// Normal window — not forced to top.
    Normal,
}

/// Current overlay visibility state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayVisibility {
    /// Overlay is visible and receiving input.
    Visible,
    /// Overlay is hidden (running in tray).
    #[default]
    Hidden,
    /// Overlay is transitioning (show/hide animation).
    Transitioning,
}

/// Persistable overlay state for restoring across sessions.
#[derive(Debug, Clone)]
pub struct OverlayPersistentState {
    /// Last known X position (virtual screen coords).
    pub x: i32,
    /// Last known Y position.
    pub y: i32,
    /// Last known width.
    pub width: u32,
    /// Last known height.
    pub height: u32,
    /// Name of the last monitor the overlay was on.
    pub monitor_name: String,
    /// Whether the overlay was visible when last saved.
    pub was_visible: bool,
}

impl Default for OverlayPersistentState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            monitor_name: String::new(),
            was_visible: false,
        }
    }
}

/// Manages overlay window state and positioning.
#[derive(Debug)]
pub struct OverlayManager {
    /// Current overlay mode.
    mode: OverlayMode,
    /// Current visibility.
    visibility: OverlayVisibility,
    /// Current window position (virtual screen coordinates).
    position: (i32, i32),
    /// Current window size.
    size: (u32, u32),
    /// Last known monitor name for session persistence.
    last_monitor: String,
    /// Persisted state loaded from config.
    persisted: Option<OverlayPersistentState>,
    /// Whether position was explicitly set by the user.
    position_explicit: bool,
}

impl OverlayManager {
    /// Create a new overlay manager with defaults.
    pub fn new() -> Self {
        OverlayManager {
            mode: OverlayMode::AlwaysOnTop,
            visibility: OverlayVisibility::Hidden,
            position: (0, 0),
            size: (800, 600),
            last_monitor: String::new(),
            persisted: None,
            position_explicit: false,
        }
    }

    /// Create from a previously saved state.
    pub fn from_persistent(state: OverlayPersistentState) -> Self {
        OverlayManager {
            mode: OverlayMode::AlwaysOnTop,
            visibility: if state.was_visible {
                OverlayVisibility::Visible
            } else {
                OverlayVisibility::Hidden
            },
            position: (state.x, state.y),
            size: (state.width, state.height),
            last_monitor: state.monitor_name.clone(),
            persisted: Some(state),
            position_explicit: true,
        }
    }

    // ── Mode ──────────────────────────────────────────────────────

    /// Current overlay mode.
    pub fn mode(&self) -> OverlayMode {
        self.mode
    }

    /// Set the overlay mode.
    pub fn set_mode(&mut self, mode: OverlayMode) {
        self.mode = mode;
    }

    // ── Visibility ────────────────────────────────────────────────

    /// Current visibility state.
    pub fn visibility(&self) -> OverlayVisibility {
        self.visibility
    }

    /// Returns `true` if the overlay is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visibility == OverlayVisibility::Visible
    }

    /// Mark the overlay as visible.
    pub fn show(&mut self) {
        self.visibility = OverlayVisibility::Visible;
        debug!("Overlay shown");
    }

    /// Mark the overlay as hidden.
    pub fn hide(&mut self) {
        self.visibility = OverlayVisibility::Hidden;
        debug!("Overlay hidden");
    }

    /// Toggle visibility state.
    pub fn toggle(&mut self) {
        if self.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }

    // ── Position ──────────────────────────────────────────────────

    /// Current overlay position in virtual screen coordinates.
    pub fn position(&self) -> (i32, i32) {
        self.position
    }

    /// Current overlay size.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Set overlay position explicitly.
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.position = (x, y);
        self.position_explicit = true;
    }

    /// Set overlay size.
    pub fn set_size(&mut self, width: u32, height: u32) {
        self.size = (width, height);
    }

    /// Smart-centre the overlay on the given monitor.
    /// If no monitor specified, centres on primary.
    pub fn centre_on(&mut self, display: &DisplayManager, monitor_name: Option<&str>) {
        let monitor = monitor_name
            .and_then(|n| display.monitor_named(n))
            .or_else(|| display.primary());
        let (cx, cy) = centre_on_monitor(self.size.0, self.size.1, monitor);
        self.position = (cx, cy);
        if let Some(m) = monitor {
            self.last_monitor = m.name.clone();
        }
    }

    /// Ensure the overlay position is within visible bounds.
    /// If the stored position is off-screen, centers on the primary monitor.
    pub fn clamp_to_visible_bounds(&mut self, display: &DisplayManager) {
        let (x, y) = clamp_to_visible(
            self.position.0,
            self.position.1,
            self.size.0,
            self.size.1,
            display,
        );
        if x != self.position.0 || y != self.position.1 {
            debug!("Overlay position clamped to ({x}, {y})");
            self.position = (x, y);
        }
    }

    // ── Persistence ───────────────────────────────────────────────

    /// Build a persistent state snapshot for saving to config.
    pub fn persistent_state(&self) -> OverlayPersistentState {
        OverlayPersistentState {
            x: self.position.0,
            y: self.position.1,
            width: self.size.0,
            height: self.size.1,
            monitor_name: self.last_monitor.clone(),
            was_visible: self.is_visible(),
        }
    }

    /// Whether the overlay position was explicitly set by the user.
    pub fn is_position_explicit(&self) -> bool {
        self.position_explicit
    }

    /// Set the persisted state from config.
    pub fn set_persistent(&mut self, state: OverlayPersistentState) {
        self.persisted = Some(state.clone());
        self.position = (state.x, state.y);
        self.size = (state.width, state.height);
        self.last_monitor = state.monitor_name;
        if state.was_visible {
            self.visibility = OverlayVisibility::Visible;
        }
        self.position_explicit = true;
    }
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply platform-specific overlay window attributes.
///
/// On Windows: Sets `HWND_TOPMOST` for always-on-top.
/// On other platforms: no-op (use window level APIs).
pub fn apply_overlay_attributes(window: &Window, mode: OverlayMode) {
    if mode != OverlayMode::AlwaysOnTop {
        return;
    }

    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };

        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::Win32(w32) = handle.as_ref()
        {
            let hwnd = w32.hwnd.get() as HWND;
            unsafe {
                SetWindowPos(
                    hwnd,
                    if mode == OverlayMode::AlwaysOnTop {
                        HWND_TOPMOST
                    } else {
                        HWND_NOTOPMOST
                    },
                    0,
                    0,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOMOVE,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_mode_default() {
        assert_eq!(OverlayMode::default(), OverlayMode::AlwaysOnTop);
    }

    #[test]
    fn test_overlay_visibility_default() {
        assert_eq!(OverlayVisibility::default(), OverlayVisibility::Hidden);
    }

    #[test]
    fn test_toggle_visibility() {
        let mut mgr = OverlayManager::new();
        assert!(!mgr.is_visible());
        mgr.show();
        assert!(mgr.is_visible());
        mgr.toggle();
        assert!(!mgr.is_visible());
        mgr.toggle();
        assert!(mgr.is_visible());
    }

    #[test]
    fn test_set_position_and_size() {
        let mut mgr = OverlayManager::new();
        mgr.set_position(100, 200);
        mgr.set_size(1280, 720);
        assert_eq!(mgr.position(), (100, 200));
        assert_eq!(mgr.size(), (1280, 720));
        assert!(mgr.is_position_explicit());
    }

    #[test]
    fn test_persistent_state_roundtrip() {
        let mut mgr = OverlayManager::new();
        mgr.set_position(100, 200);
        mgr.set_size(1280, 720);
        mgr.show();
        mgr = OverlayManager::from_persistent(mgr.persistent_state());
        assert_eq!(mgr.position(), (100, 200));
        assert_eq!(mgr.size(), (1280, 720));
        assert!(mgr.is_visible());
    }

    // Tests using DisplayManager::new() require a real window handle
    // and are placed in the display module tests.

    #[test]
    fn test_set_persistent_restores_state() {
        let state = OverlayPersistentState {
            x: 100,
            y: 200,
            width: 1280,
            height: 720,
            monitor_name: "Primary".into(),
            was_visible: true,
        };
        let mut mgr = OverlayManager::new();
        mgr.set_persistent(state);
        assert_eq!(mgr.position(), (100, 200));
        assert_eq!(mgr.size(), (1280, 720));
        assert!(mgr.is_visible());
        assert!(mgr.is_position_explicit());
    }

    #[test]
    fn test_default_persistent_state() {
        let state = OverlayPersistentState::default();
        assert_eq!(state.x, 0);
        assert_eq!(state.width, 800);
        assert_eq!(state.height, 600);
        assert!(!state.was_visible);
    }
}
