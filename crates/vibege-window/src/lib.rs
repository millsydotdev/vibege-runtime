#![allow(deprecated)] // pending winit 0.30 ApplicationHandler migration

//! # VibeGE Window
//!
//! Cross-platform window management using `winit`.
//!
//! ## Architecture
//!
//! This crate provides a layered window abstraction:
//!
//! - **`WindowManager`** — Native window creation, event loop, and lifecycle.
//! - **`OverlayManager`** — Overlay-specific state, positioning, and persistence.
//! - **`DisplayManager`** — Multi-monitor enumeration, hot-plug detection, safe bounds.
//! - **`DpiManager`** — DPI scaling calculations for logical ↔ physical conversion.
//!
//! ## Modules
//!
//! | Module       | Responsibility                                      |
//! |--------------|-----------------------------------------------------|
//! | `display`    | Monitor enumeration, safe bounds, centring          |
//! | `dpi`        | DPI scaling, logical↔physical conversion            |
//! | `overlay`    | Overlay state, positioning, persistence             |
//! | `lib`        | WindowManager, WindowMode, WindowInfo, WindowEvent  |

pub mod display;
pub mod dpi;
pub mod overlay;

use std::sync::Arc;

use tracing::{debug, info};
use vibege_core::{ErrorCode, RuntimeConfig, RuntimeError};
use winit::dpi::LogicalSize;

use crate::display::DisplayManager;
use crate::overlay::OverlayManager;

/// Describes the current window mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    /// Standard windowed mode.
    Windowed,
    /// Exclusive fullscreen on the selected monitor.
    Fullscreen,
    /// Borderless fullscreen window (no title bar).
    BorderlessFullscreen,
    /// Minimized to taskbar/dock.
    Minimized,
    /// Maximized window.
    Maximized,
}

/// Resolution, position, and state information for a window.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub mode: WindowMode,
    pub vsync: bool,
    pub fps_limit: u32,
    pub dpi_scale: f64,
    pub x: i32,
    pub y: i32,
    pub visible: bool,
}

/// Events that the window system can emit.
#[derive(Debug, Clone)]
pub enum WindowEvent {
    Resized { width: u32, height: u32 },
    Moved { x: i32, y: i32 },
    Focused,
    Blurred,
    CloseRequested,
    Destroyed,
    ScaleFactorChanged(f64),
    Minimized,
    Restored,
    DisplayChanged { count: usize },
}

/// Callback trait for receiving window events.
pub trait WindowEventHandler: Send {
    fn on_window_event(&mut self, event: &WindowEvent);
}

/// Error types specific to window management.
#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("Failed to create window: {0}")]
    CreationFailed(String),

    #[error("Failed to enter fullscreen: {0}")]
    FullscreenFailed(String),

    #[error("Event loop error: {0}")]
    EventLoopError(String),

    #[error("No window available")]
    NoWindow,

    #[error("Overlay mode not supported on this platform")]
    OverlayNotSupported,
}

impl From<WindowError> for RuntimeError {
    fn from(err: WindowError) -> Self {
        let code = match &err {
            WindowError::CreationFailed(_) => ErrorCode::INIT_FAILED,
            WindowError::FullscreenFailed(_) => ErrorCode::INIT_FAILED,
            WindowError::EventLoopError(_) => ErrorCode::INTERNAL,
            WindowError::NoWindow => ErrorCode::INTERNAL,
            WindowError::OverlayNotSupported => ErrorCode::INIT_FAILED,
        };
        RuntimeError::new(code, err.to_string())
    }
}

/// The window manager handles native window creation and event loop management.
///
/// Supports overlay mode (always-on-top), position tracking, DPI awareness,
/// and integration with the multi-monitor display system.
pub struct WindowManager {
    window: Arc<winit::window::Window>,
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    config: WindowInfo,
    event_handler: Option<Box<dyn WindowEventHandler>>,
    request_shutdown: Arc<std::sync::atomic::AtomicBool>,
    overlay: OverlayManager,
    display: DisplayManager,
}

impl WindowManager {
    /// Creates a new window manager and opens a native window.
    ///
    /// If `overlay_mode` is true, the window is created without decorations
    /// and set always-on-top.
    pub fn new(config: &RuntimeConfig, overlay_mode: bool) -> Result<Self, WindowError> {
        let event_loop =
            winit::event_loop::EventLoop::new().map_err(|e: winit::error::EventLoopError| {
                WindowError::CreationFailed(format!("Event loop: {e}"))
            })?;

        let window_config = &config.window;

        let window = event_loop
            .create_window(
                winit::window::WindowAttributes::new()
                    .with_title(&window_config.title)
                    .with_inner_size(LogicalSize::new(
                        window_config.width as f64,
                        window_config.height as f64,
                    ))
                    .with_decorations(!overlay_mode)
                    .with_fullscreen(if window_config.fullscreen {
                        Some(winit::window::Fullscreen::Borderless(None))
                    } else {
                        None
                    }),
            )
            .map_err(|e: winit::error::OsError| WindowError::CreationFailed(e.to_string()))?;

        let dpi = window.scale_factor();

        // Apply overlay attributes
        if overlay_mode {
            overlay::apply_overlay_attributes(&window, overlay::OverlayMode::AlwaysOnTop);
        }

        let display = DisplayManager::new(&window);
        let (x, y) = window
            .outer_position()
            .map(|p| (p.x, p.y))
            .unwrap_or((0, 0));
        let inner = window.inner_size();

        let mut overlay = OverlayManager::new();
        overlay.set_position(x, y);
        overlay.set_size(inner.width, inner.height);
        overlay.centre_on(&display, None);

        let window_info = WindowInfo {
            width: window_config.width,
            height: window_config.height,
            title: window_config.title.clone(),
            mode: if window_config.fullscreen {
                WindowMode::BorderlessFullscreen
            } else {
                WindowMode::Windowed
            },
            vsync: window_config.vsync,
            fps_limit: config.fps_limit,
            dpi_scale: dpi,
            x,
            y,
            visible: true,
        };

        info!(
            title = %window_info.title,
            width = window_info.width,
            height = window_info.height,
            dpi = window_info.dpi_scale,
            overlay = overlay_mode,
            "Window created"
        );

        Ok(Self {
            window: Arc::new(window),
            event_loop: Some(event_loop),
            config: window_info,
            event_handler: None,
            request_shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            overlay,
            display,
        })
    }

    // ── Accessors ─────────────────────────────────────────────────

    /// Returns information about the current window state.
    pub fn info(&self) -> &WindowInfo {
        &self.config
    }

    /// Returns the underlying `winit::Window`.
    pub fn inner_window(&self) -> &winit::window::Window {
        &self.window
    }

    /// Returns an `Arc` to the window for use in renderers.
    pub fn window_arc(&self) -> Arc<winit::window::Window> {
        Arc::clone(&self.window)
    }

    /// Reference to the overlay manager.
    pub fn overlay(&self) -> &OverlayManager {
        &self.overlay
    }

    /// Mutable reference to the overlay manager.
    pub fn overlay_mut(&mut self) -> &mut OverlayManager {
        &mut self.overlay
    }

    /// Reference to the display manager.
    pub fn display(&self) -> &DisplayManager {
        &self.display
    }

    /// Replace the overlay manager (e.g. from persisted state).
    pub fn set_overlay(&mut self, overlay: OverlayManager) {
        self.overlay = overlay;
    }

    // ── Window Lifecycle ──────────────────────────────────────────

    /// Show the window.
    pub fn show(&self) {
        self.window.set_visible(true);
    }

    /// Hide the window.
    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    /// Minimize the window.
    pub fn minimize(&self) {
        self.window.set_minimized(true);
    }

    /// Restore the window from minimized state.
    pub fn restore(&self) {
        self.window.set_minimized(false);
    }

    /// Check if the window is visible.
    pub fn is_visible(&self) -> bool {
        self.window.is_visible().unwrap_or(true)
    }

    /// Set the window title.
    pub fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
        self.config.title = title.to_string();
    }

    // ── Window Mode ───────────────────────────────────────────────

    /// Sets the window mode.
    pub fn set_mode(&mut self, mode: WindowMode) -> Result<(), WindowError> {
        match mode {
            WindowMode::Windowed => {
                self.window.set_fullscreen(None);
            }
            WindowMode::Fullscreen => {
                self.window
                    .set_fullscreen(Some(winit::window::Fullscreen::Exclusive(
                        self.window
                            .current_monitor()
                            .and_then(|m| m.video_modes().next())
                            .ok_or_else(|| {
                                WindowError::FullscreenFailed("No video modes".into())
                            })?,
                    )));
            }
            WindowMode::BorderlessFullscreen => {
                self.window
                    .set_fullscreen(Some(winit::window::Fullscreen::Borderless(
                        self.window.current_monitor(),
                    )));
            }
            WindowMode::Minimized => {
                self.window.set_minimized(true);
            }
            WindowMode::Maximized => {
                self.window.set_maximized(true);
            }
        }
        self.config.mode = mode;
        Ok(())
    }

    /// Toggles between windowed and borderless fullscreen.
    pub fn toggle_fullscreen(&mut self) -> Result<(), WindowError> {
        let new_mode = if self.config.mode == WindowMode::Windowed {
            WindowMode::BorderlessFullscreen
        } else {
            WindowMode::Windowed
        };
        self.set_mode(new_mode)
    }

    // ── Overlay Integration ───────────────────────────────────────

    /// Toggle overlay visibility and sync with the underlying window.
    pub fn toggle_overlay(&mut self) {
        self.overlay.toggle();
        let visible = self.overlay.is_visible();
        self.window.set_visible(visible);
        self.config.visible = visible;
        debug!(visible, "Overlay toggled");

        // Re-apply topmost on show
        if visible {
            overlay::apply_overlay_attributes(&self.window, self.overlay.mode());
        }
    }

    /// Ensure overlay position is within visible bounds.
    pub fn ensure_overlay_visible(&mut self) {
        self.overlay.clamp_to_visible_bounds(&self.display);
        let (x, y) = self.overlay.position();
        self.window
            .set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }

    // ── Display Management ────────────────────────────────────────

    /// Refresh display info (call on `ScaleFactorChanged` or monitor events).
    pub fn refresh_displays(&mut self) {
        self.display.refresh(&self.window);
        self.overlay.clamp_to_visible_bounds(&self.display);
        if let Some(handler) = &mut self.event_handler {
            handler.on_window_event(&WindowEvent::DisplayChanged {
                count: self.display.count(),
            });
        }
    }

    // ── Event Handler ─────────────────────────────────────────────

    /// Sets the event handler for window events.
    pub fn set_event_handler(&mut self, handler: Box<dyn WindowEventHandler>) {
        self.event_handler = Some(handler);
    }

    /// Requests the window to close.
    pub fn request_close(&self) {
        self.request_shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Runs the event loop, blocking until the window is closed.
    ///
    /// Consumes the window manager and integrates with the runtime lifecycle.
    pub fn run_event_loop(mut self) -> Result<(), WindowError> {
        let window = Arc::clone(&self.window);
        let shutdown = Arc::clone(&self.request_shutdown);
        let mut handler = self.event_handler.take();
        let mut display_manager = DisplayManager::new(&window);
        let mut last_monitor_count = display_manager.count();

        let event_loop = self
            .event_loop
            .take()
            .ok_or_else(|| WindowError::EventLoopError("Event loop already consumed".into()))?;

        event_loop
            .run(move |event, elwt| {
                if shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                    info!("Window shutdown requested");
                    elwt.exit();
                    return;
                }

                // Notify event handler
                if let Some(ref mut h) = handler
                    && let winit::event::Event::WindowEvent { event: we, .. } = &event
                {
                    match we {
                        winit::event::WindowEvent::Resized(size) => {
                            h.on_window_event(&WindowEvent::Resized {
                                width: size.width,
                                height: size.height,
                            });
                        }
                        winit::event::WindowEvent::Focused(true) => {
                            h.on_window_event(&WindowEvent::Focused);
                        }
                        winit::event::WindowEvent::Focused(false) => {
                            h.on_window_event(&WindowEvent::Blurred);
                        }
                        winit::event::WindowEvent::Moved(pos) => {
                            h.on_window_event(&WindowEvent::Moved { x: pos.x, y: pos.y });
                        }
                        winit::event::WindowEvent::Occluded(true) => {
                            h.on_window_event(&WindowEvent::Minimized);
                        }
                        winit::event::WindowEvent::Occluded(false) => {
                            h.on_window_event(&WindowEvent::Restored);
                        }
                        winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            h.on_window_event(&WindowEvent::ScaleFactorChanged(*scale_factor));
                        }
                        _ => {}
                    }
                }

                match &event {
                    winit::event::Event::WindowEvent { event: we, .. } => match we {
                        winit::event::WindowEvent::CloseRequested => {
                            info!("Window close requested");
                            elwt.exit();
                        }
                        winit::event::WindowEvent::Resized(_) => {
                            window.request_redraw();
                        }
                        winit::event::WindowEvent::Moved(pos) => {
                            debug!(x = pos.x, y = pos.y, "Window moved");
                        }
                        winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            debug!(scale = scale_factor, "DPI scale changed");
                        }
                        _ => {}
                    },
                    winit::event::Event::AboutToWait => {
                        // Check for display changes
                        let current_count = DisplayManager::new(&window).count();
                        if current_count != last_monitor_count {
                            debug!(
                                before = last_monitor_count,
                                after = current_count,
                                "Display count changed"
                            );
                            display_manager.refresh(&window);
                            last_monitor_count = current_count;
                        }
                        window.request_redraw();
                    }
                    _ => {}
                }
            })
            .map_err(|e| WindowError::EventLoopError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_mode_equality() {
        assert_eq!(WindowMode::Windowed, WindowMode::Windowed);
        assert_ne!(WindowMode::Windowed, WindowMode::Fullscreen);
    }

    #[test]
    fn test_window_info_defaults() {
        let info = WindowInfo {
            width: 1280,
            height: 720,
            title: "Test".into(),
            mode: WindowMode::Windowed,
            vsync: true,
            fps_limit: 0,
            dpi_scale: 1.0,
            x: 0,
            y: 0,
            visible: true,
        };
        assert_eq!(info.width, 1280);
        assert_eq!(info.title, "Test");
        assert_eq!(info.x, 0);
        assert!(info.visible);
    }

    #[test]
    fn test_window_error_conversion() {
        let err = WindowError::CreationFailed("test".into());
        let runtime_err: RuntimeError = err.into();
        assert_eq!(runtime_err.code, ErrorCode::INIT_FAILED);

        let err = WindowError::OverlayNotSupported;
        let runtime_err: RuntimeError = err.into();
        assert_eq!(runtime_err.code, ErrorCode::INIT_FAILED);
    }

    #[test]
    fn test_window_mode_toggle_logic() {
        let mut mode = WindowMode::BorderlessFullscreen;
        assert_eq!(mode, WindowMode::BorderlessFullscreen);
        mode = WindowMode::Windowed;
        assert_eq!(mode, WindowMode::Windowed);
    }

    #[test]
    fn test_window_event_moved() {
        let event = WindowEvent::Moved { x: 100, y: 200 };
        match event {
            WindowEvent::Moved { x, y } => {
                assert_eq!(x, 100);
                assert_eq!(y, 200);
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn test_window_event_display_changed() {
        let event = WindowEvent::DisplayChanged { count: 2 };
        match event {
            WindowEvent::DisplayChanged { count } => {
                assert_eq!(count, 2);
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn test_window_event_minimized_restored() {
        let min = WindowEvent::Minimized;
        let rest = WindowEvent::Restored;
        assert!(matches!(min, WindowEvent::Minimized));
        assert!(matches!(rest, WindowEvent::Restored));
    }
}
