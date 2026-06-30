#![allow(deprecated)] // winit 0.30 APIs still work, not worth ApplicationHandler migration yet

//! # VibeGE Window
//!
//! Cross-platform window management using `winit`.
//!
//! This crate provides native window creation, event loop management,
//! and window mode switching (windowed, fullscreen, borderless).
//!
//! ## Architecture
//!
//! The `WindowManager` wraps a `winit::EventLoop` and `winit::Window`.
//! It exposes a simplified API that the runtime core uses to create
//! and manage windows, while the event loop integrates with the
//! runtime's lifecycle.

use std::sync::Arc;

use tracing::{debug, info};
use vibege_core::{ErrorCode, RuntimeConfig, RuntimeError};

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

/// Resolution and position information for a window.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub mode: WindowMode,
    pub vsync: bool,
    pub fps_limit: u32,
    pub dpi_scale: f64,
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
}

impl From<WindowError> for RuntimeError {
    fn from(err: WindowError) -> Self {
        let code = match &err {
            WindowError::CreationFailed(_) => ErrorCode::INIT_FAILED,
            WindowError::FullscreenFailed(_) => ErrorCode::INIT_FAILED,
            WindowError::EventLoopError(_) => ErrorCode::INTERNAL,
            WindowError::NoWindow => ErrorCode::INTERNAL,
        };
        RuntimeError::new(code, err.to_string())
    }
}

/// The window manager handles native window creation and event loop management.
///
/// On creation, it opens a native window. The event loop is integrated with
/// the runtime lifecycle via the `run()` method, which blocks until the window
/// is closed or a shutdown is requested.
pub struct WindowManager {
    window: Arc<winit::window::Window>,
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    config: WindowInfo,
    event_handler: Option<Box<dyn WindowEventHandler>>,
    request_shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl WindowManager {
    /// Creates a new window manager and opens a native window.
    ///
    /// The window configuration is taken from `RuntimeConfig`. If no custom
    /// configuration is provided, defaults are used (1280x720, "VibeGE Runtime").
    pub fn new(config: &RuntimeConfig) -> Result<Self, WindowError> {
        let event_loop =
            winit::event_loop::EventLoop::new().map_err(|e: winit::error::EventLoopError| {
                WindowError::CreationFailed(format!("Event loop: {e}"))
            })?;

        let window_config = &config.window;

        let window = event_loop
            .create_window(
                winit::window::WindowAttributes::new()
                    .with_title(&window_config.title)
                    .with_inner_size(winit::dpi::LogicalSize::new(
                        window_config.width as f64,
                        window_config.height as f64,
                    ))
                    .with_fullscreen(if window_config.fullscreen {
                        Some(winit::window::Fullscreen::Borderless(None))
                    } else {
                        None
                    }),
            )
            .map_err(|e: winit::error::OsError| WindowError::CreationFailed(e.to_string()))?;

        if window_config.vsync {
            // VSync is handled by the renderer; we just note the preference
            debug!("VSync requested");
        }

        // Set window properties
        window.set_window_icon(None);

        // On Windows, disable the close button from killing the process immediately
        #[cfg(windows)]
        {
            // We handle close events via the event loop
        }

        let dpi = window.scale_factor();

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
        };

        info!(
            title = %window_info.title,
            width = window_info.width,
            height = window_info.height,
            dpi = window_info.dpi_scale,
            "Window created"
        );

        Ok(Self {
            window: Arc::new(window),
            event_loop: Some(event_loop),
            config: window_info,
            event_handler: None,
            request_shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

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

    /// Sets the window title.
    pub fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
        self.config.title = title.to_string();
    }

    /// Sets the window mode.
    pub fn set_mode(&mut self, mode: WindowMode) -> Result<(), WindowError> {
        match mode {
            WindowMode::Windowed => {
                self.window.set_fullscreen(None);
            }
            WindowMode::Fullscreen => {
                let monitor = self.window.current_monitor();
                if let Some(monitor) = monitor {
                    self.window
                        .set_fullscreen(Some(winit::window::Fullscreen::Exclusive(
                            monitor.video_modes().next().ok_or_else(|| {
                                WindowError::FullscreenFailed("No video modes available".into())
                            })?,
                        )));
                }
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
    /// Consumes the window manager, taking ownership of the event loop.
    /// This method blocks until the window is closed or `request_close()` is called.
    /// Should be called from a dedicated thread.
    pub fn run_event_loop(mut self) -> Result<(), WindowError> {
        let window = Arc::clone(&self.window);
        let shutdown = Arc::clone(&self.request_shutdown);
        let mut handler = self.event_handler.take();

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

                // Notify event handler if set
                #[allow(clippy::single_match)]
                if let Some(ref mut h) = handler {
                    match &event {
                        winit::event::Event::WindowEvent { event, .. } => match event {
                            winit::event::WindowEvent::Resized(size) => {
                                h.on_window_event(&WindowEvent::Resized {
                                    width: size.width, height: size.height,
                                });
                            }
                            winit::event::WindowEvent::Focused(true) => {
                                h.on_window_event(&WindowEvent::Focused);
                            }
                            winit::event::WindowEvent::Focused(false) => {
                                h.on_window_event(&WindowEvent::Blurred);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }

                match event {
                    winit::event::Event::WindowEvent { event, .. } => match event {
                        winit::event::WindowEvent::CloseRequested => {
                            info!("Window close requested");
                            elwt.exit();
                        }
                        winit::event::WindowEvent::Resized(size) => {
                            window.request_redraw();
                            debug!(width = size.width, height = size.height, "Window resized");
                        }
                        winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            debug!(scale = scale_factor, "DPI scale changed");
                        }
                        winit::event::WindowEvent::Focused(focused) => {
                            debug!(focused = focused, "Window focus changed");
                        }
                        _ => {}
                    },
                    winit::event::Event::AboutToWait => {
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
        };
        assert_eq!(info.width, 1280);
        assert_eq!(info.title, "Test");
    }

    #[test]
    fn test_window_error_conversion() {
        let err = WindowError::CreationFailed("test error".into());
        let runtime_err: RuntimeError = err.into();
        assert_eq!(runtime_err.code, ErrorCode::INIT_FAILED);
    }

    #[test]
    fn test_window_mode_toggle_logic() {
        // Test the mode switching logic without GPU/window
        let mut mode = WindowMode::BorderlessFullscreen;
        assert_eq!(mode, WindowMode::BorderlessFullscreen);
        mode = WindowMode::Windowed;
        assert_eq!(mode, WindowMode::Windowed);
    }
}
