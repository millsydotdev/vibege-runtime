//! System tray and global hotkey support (stub for future implementation).
//!
//! Global hotkey for the overlay is currently handled directly in the runtime app
//! via winit's platform-specific EventLoopBuilderExtWindows::with_msg_hook.

use std::sync::atomic::{AtomicBool, Ordering};

static TOGGLE_OVERLAY: AtomicBool = AtomicBool::new(false);

/// Check if the overlay should be toggled.
pub fn should_toggle_overlay() -> bool {
    TOGGLE_OVERLAY.swap(false, Ordering::SeqCst)
}

/// Request overlay toggle from external code.
pub fn request_toggle() {
    TOGGLE_OVERLAY.store(true, Ordering::SeqCst);
}

/// Start system tray icon (optional, not yet implemented on all platforms).
pub fn start() -> Option<()> {
    None
}
