#![allow(unsafe_op_in_unsafe_fn)]
//! # VibeGE Tray Icon
//!
//! System tray icon with right-click menu:
//! - Open Game Store → signals runtime to show launcher
//! - Show/Hide Overlay → toggle overlay visibility
//! - Quit → exit application
//!
//! On Windows, uses Shell_NotifyIconW + hidden window for messages.

use std::sync::atomic::{AtomicBool, Ordering};

static SHOW_LAUNCHER: AtomicBool = AtomicBool::new(false);
static TOGGLE_OVERLAY: AtomicBool = AtomicBool::new(false);
static QUIT: AtomicBool = AtomicBool::new(false);

pub fn should_show_launcher() -> bool {
    SHOW_LAUNCHER.swap(false, Ordering::SeqCst)
}
pub fn should_toggle_overlay() -> bool {
    TOGGLE_OVERLAY.swap(false, Ordering::SeqCst)
}
pub fn request_toggle() {
    TOGGLE_OVERLAY.store(true, Ordering::SeqCst);
}
pub fn should_quit() -> bool {
    QUIT.load(Ordering::SeqCst)
}

pub fn start() -> Option<std::thread::JoinHandle<()>> {
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
    const IDM_QUIT: u16 = 103;

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
            hbrBackground: (5 + 1) as isize, // COLOR_WINDOW + 1
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

        // Add tray icon
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

        let mut msg = std::mem::zeroed::<MSG>();
        while GetMessageW(&mut msg, 0isize, 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup on quit
        let mut nid = std::mem::zeroed::<NOTIFYICONDATAW>();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = ID_TRAY;
        Shell_NotifyIconW(NIM_DELETE, &nid);
        DestroyWindow(hwnd);
    }

    unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_TRAY => {
                let cmd = (lp as u32) & 0xFFFF;
                if cmd == WM_RBUTTONUP || cmd == WM_LBUTTONUP {
                    // Show popup menu
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
}
