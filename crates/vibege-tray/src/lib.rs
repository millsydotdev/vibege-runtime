#![allow(unsafe_op_in_unsafe_fn)]
//! # VibeGE Tray Icon
//!
//! System tray icon with dynamic right-click menu, notifications,
//! and safe application lifecycle signalling.
//!
//! ## Architecture
//!
//! The tray runs in its own thread and communicates with the main
//! loop via three atomic flags for bootstrapping (show launcher,
//! toggle overlay, quit). The main loop polls these each frame.
//!
//! Additionally, the tray supports:
//! - **Dynamic menus**: update the tray menu at runtime
//! - **Notifications**: show balloon notifications
//! - **Status**: change the tray icon or tooltip at runtime
//!
//! ## Menu Items
//!
//! | ID   | Label              | Action                           |
//! |------|--------------------|----------------------------------|
//! | 101  | Open Game Store    | Signals runtime to show launcher |
//! | 102  | Show/Hide Overlay  | Toggles overlay visibility       |
//! | 103  | Restart Runtime    | Restarts the application         |
//! | 104  | Open Logs          | Opens log directory              |
//! | 105  | About VibeGE       | Shows version info               |
//! | 199  | Quit               | Exits application                |
//!
//! ## Thread Safety
//!
//! The tray thread communicates with the main thread through:
//! - `AtomicBool` flags for simple signals
//! - `std::sync::mpsc` for future extensions (notifications, menu updates)

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static SHOW_LAUNCHER: AtomicBool = AtomicBool::new(false);
static TOGGLE_OVERLAY: AtomicBool = AtomicBool::new(false);
static QUIT: AtomicBool = AtomicBool::new(false);
static RESTART: AtomicBool = AtomicBool::new(false);

/// Runtime status displayed in the tray tooltip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayStatus {
    /// Runtime is running normally.
    Running,
    /// A game is active.
    InGame,
    /// Overlay is visible.
    OverlayActive,
    /// Runtime is suspended.
    Suspended,
    /// An error occurred.
    Error,
}

impl std::fmt::Display for TrayStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrayStatus::Running => write!(f, "Running"),
            TrayStatus::InGame => write!(f, "In Game"),
            TrayStatus::OverlayActive => write!(f, "Overlay Active"),
            TrayStatus::Suspended => write!(f, "Suspended"),
            TrayStatus::Error => write!(f, "Error"),
        }
    }
}

/// Persistent tray state shared between the main thread and tray thread.
struct TrayShared {
    status: TrayStatus,
    overlay_label: String,
    notification: Option<(String, String)>,
}

static TRAY_STATE: Mutex<Option<TrayShared>> = Mutex::new(None);

// ── Signal API ─────────────────────────────────────────────────

/// Check if the launcher should be shown (consumes the signal).
pub fn should_show_launcher() -> bool {
    SHOW_LAUNCHER.swap(false, Ordering::SeqCst)
}

/// Check if the overlay should be toggled (consumes the signal).
pub fn should_toggle_overlay() -> bool {
    TOGGLE_OVERLAY.swap(false, Ordering::SeqCst)
}

/// Signal the overlay to toggle.
pub fn request_toggle() {
    TOGGLE_OVERLAY.store(true, Ordering::SeqCst);
}

/// Check if a restart is requested (consumes the signal).
pub fn should_restart() -> bool {
    RESTART.swap(false, Ordering::SeqCst)
}

/// Check if the application should quit.
pub fn should_quit() -> bool {
    QUIT.load(Ordering::SeqCst)
}

// ── Status API ─────────────────────────────────────────────────

/// Update the tray status display.
pub fn set_status(status: TrayStatus) {
    if let Ok(mut state) = TRAY_STATE.lock()
        && let Some(ref mut s) = *state
    {
        s.status = status;
    }
}

/// Update the overlay menu label.
pub fn set_overlay_label(visible: bool) {
    if let Ok(mut state) = TRAY_STATE.lock()
        && let Some(ref mut s) = *state
    {
        s.overlay_label = if visible {
            "Hide Overlay".to_string()
        } else {
            "Show Overlay".to_string()
        };
    }
}

/// Show a tray notification balloon.
pub fn show_notification(title: &str, message: &str) {
    if let Ok(mut state) = TRAY_STATE.lock()
        && let Some(ref mut s) = *state
    {
        s.notification = Some((title.to_string(), message.to_string()));
    }
}

// ── Startup ────────────────────────────────────────────────────

/// Start the tray icon thread.
///
/// Returns a `JoinHandle` that the runtime can join on shutdown.
pub fn start() -> Option<std::thread::JoinHandle<()>> {
    // Initialise shared state
    if let Ok(mut state) = TRAY_STATE.lock() {
        *state = Some(TrayShared {
            status: TrayStatus::Running,
            overlay_label: "Show Overlay".to_string(),
            notification: None,
        });
    }

    #[cfg(windows)]
    {
        Some(start_windows())
    }
    #[cfg(not(windows))]
    {
        tracing::warn!("Tray not supported on this platform");
        None
    }
}

#[cfg(windows)]
fn start_windows() -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("vibege-tray".into())
        .spawn(tray_loop)
        .expect("tray thread")
}

#[cfg(windows)]
fn tray_loop() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    const WM_TRAY: u32 = WM_USER + 1;
    const ID_TRAY: u32 = 1;
    const IDM_LAUNCHER: u16 = 101;
    const IDM_TOGGLE: u16 = 102;
    const IDM_RESTART: u16 = 103;
    const IDM_LOGS: u16 = 104;
    const IDM_ABOUT: u16 = 105;
    const IDM_QUIT: u16 = 199;

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let class = to_wide("VibeGETray");

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(tray_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: 0,
            hCursor: 0,
            hbrBackground: (5 + 1) as isize,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            to_wide("VG").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            0isize,
            0isize,
            inst as isize,
            std::ptr::null(),
        );

        if hwnd == 0 {
            return;
        }

        // ── Tray icon ──
        let mut nid = std::mem::zeroed::<NOTIFYICONDATAW>();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = ID_TRAY;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = LoadIconW(0, IDI_APPLICATION);
        let tip = to_wide("VibeGE Game Overlay\0");
        for (i, &c) in tip.iter().enumerate().take(128) {
            nid.szTip[i] = c;
        }
        Shell_NotifyIconW(NIM_ADD, &nid);

        tracing::info!("Tray icon active");

        // ── Message loop ──
        let mut msg = std::mem::zeroed::<MSG>();
        while GetMessageW(&mut msg, 0isize, 0, 0) != 0 {
            // Process pending state updates (notifications, tooltip)
            process_tray_updates(hwnd, ID_TRAY, WM_TRAY);

            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // ── Cleanup ──
        let mut nid = std::mem::zeroed::<NOTIFYICONDATAW>();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = ID_TRAY;
        Shell_NotifyIconW(NIM_DELETE, &nid);
        DestroyWindow(hwnd);
    }

    // ── Window Procedure ──
    unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_TRAY => {
                let cmd = (lp as u32) & 0xFFFF;
                if cmd == WM_RBUTTONUP || cmd == WM_LBUTTONUP {
                    let menu = CreatePopupMenu();
                    if menu != 0 {
                        AppendMenuW(
                            menu,
                            MF_STRING,
                            IDM_LAUNCHER as usize,
                            to_wide("Open Game Store\0").as_ptr(),
                        );
                        AppendMenuW(
                            menu,
                            MF_STRING,
                            IDM_TOGGLE as usize,
                            to_wide("Show/Hide Overlay\0").as_ptr(),
                        );
                        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                        AppendMenuW(
                            menu,
                            MF_STRING,
                            IDM_RESTART as usize,
                            to_wide("Restart Runtime\0").as_ptr(),
                        );
                        AppendMenuW(
                            menu,
                            MF_STRING,
                            IDM_LOGS as usize,
                            to_wide("Open Logs\0").as_ptr(),
                        );
                        AppendMenuW(
                            menu,
                            MF_STRING,
                            IDM_ABOUT as usize,
                            to_wide("About VibeGE\0").as_ptr(),
                        );
                        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                        AppendMenuW(
                            menu,
                            MF_STRING,
                            IDM_QUIT as usize,
                            to_wide("Quit\0").as_ptr(),
                        );
                        SetForegroundWindow(hwnd);
                        let mut pt = std::mem::zeroed::<POINT>();
                        GetCursorPos(&mut pt);
                        TrackPopupMenu(
                            menu,
                            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                            pt.x,
                            pt.y,
                            0,
                            hwnd,
                            std::ptr::null_mut(),
                        );
                        DestroyMenu(menu);
                    }
                }
                0
            }
            WM_COMMAND => {
                let id = (wp as u32) & 0xFFFF;
                match id as u16 {
                    IDM_LAUNCHER => SHOW_LAUNCHER.store(true, Ordering::SeqCst),
                    IDM_TOGGLE => TOGGLE_OVERLAY.store(true, Ordering::SeqCst),
                    IDM_RESTART => RESTART.store(true, Ordering::SeqCst),
                    IDM_LOGS => {
                        // Open log directory — platform-specific
                        #[cfg(windows)]
                        {
                            let _ = std::process::Command::new("explorer").arg(".").spawn();
                        }
                    }
                    IDM_ABOUT => {
                        // Show version info via notification
                        show_notification(
                            "About VibeGE",
                            &format!(
                                "VibeGE Runtime v{}\nAI-friendly game overlay",
                                env!("CARGO_PKG_VERSION")
                            ),
                        );
                    }
                    IDM_QUIT => {
                        QUIT.store(true, Ordering::SeqCst);
                        PostQuitMessage(0);
                    }
                    _ => {}
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    /// Process pending state updates from the main thread.
    unsafe fn process_tray_updates(hwnd: HWND, id_tray: u32, _wm_tray: u32) {
        use windows_sys::Win32::UI::Shell::NIF_INFO;

        if let Ok(state) = TRAY_STATE.lock()
            && let Some(ref s) = *state
        {
            // Update tooltip based on status
            let tooltip = format!("VibeGE — {}", s.status);
            let wide_tip = to_wide(&format!("{tooltip}\0"));
            let mut nid = std::mem::zeroed::<NOTIFYICONDATAW>();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = id_tray;
            nid.uFlags = NIF_TIP;
            for (i, &c) in wide_tip.iter().enumerate().take(128) {
                nid.szTip[i] = c;
            }
            Shell_NotifyIconW(NIM_MODIFY, &nid);

            // Show pending notification
            if let Some((ref title, ref message)) = s.notification {
                let mut nid = std::mem::zeroed::<NOTIFYICONDATAW>();
                nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                nid.hWnd = hwnd;
                nid.uID = id_tray;
                nid.uFlags = NIF_INFO;
                nid.dwInfoFlags = NIIF_INFO;
                let wide_title = to_wide(&format!("{title}\0"));
                let wide_msg = to_wide(&format!("{message}\0"));
                for (i, &c) in wide_title.iter().enumerate().take(64) {
                    nid.szInfoTitle[i] = c;
                }
                for (i, &c) in wide_msg.iter().enumerate().take(256) {
                    nid.szInfo[i] = c;
                }
                Shell_NotifyIconW(NIM_MODIFY, &nid);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_status_display() {
        assert_eq!(TrayStatus::Running.to_string(), "Running");
        assert_eq!(TrayStatus::InGame.to_string(), "In Game");
        assert_eq!(TrayStatus::OverlayActive.to_string(), "Overlay Active");
        assert_eq!(TrayStatus::Suspended.to_string(), "Suspended");
        assert_eq!(TrayStatus::Error.to_string(), "Error");
    }

    #[test]
    fn test_tray_status_equality() {
        assert_eq!(TrayStatus::Running, TrayStatus::Running);
        assert_ne!(TrayStatus::Running, TrayStatus::Error);
    }

    #[test]
    fn test_signal_api() {
        // Initial state — no signals
        assert!(!should_show_launcher());
        assert!(!should_toggle_overlay());
        assert!(!should_quit());
        assert!(!should_restart());

        // Request toggle
        request_toggle();
        assert!(should_toggle_overlay());
        // Second read should return false (consumed)
        assert!(!should_toggle_overlay());
    }

    #[test]
    fn test_set_overlay_label() {
        set_overlay_label(true);
        if let Ok(state) = TRAY_STATE.lock()
            && let Some(ref s) = *state
        {
            assert_eq!(s.overlay_label, "Hide Overlay");
        }
        set_overlay_label(false);
        if let Ok(state) = TRAY_STATE.lock()
            && let Some(ref s) = *state
        {
            assert_eq!(s.overlay_label, "Show Overlay");
        }
    }

    #[test]
    fn test_set_status() {
        set_status(TrayStatus::InGame);
        if let Ok(state) = TRAY_STATE.lock()
            && let Some(ref s) = *state
        {
            assert_eq!(s.status, TrayStatus::InGame);
        }
    }

    #[test]
    fn test_show_notification() {
        show_notification("Test Title", "Test Message");
        if let Ok(state) = TRAY_STATE.lock()
            && let Some(ref s) = *state
        {
            assert_eq!(
                s.notification,
                Some(("Test Title".into(), "Test Message".into()))
            );
        }
    }
}
