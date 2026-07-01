//! WebView UI — the phone-like overlay gaming app UI.
//!
//! Uses wry's built-in IPC: JavaScript calls `window.ipc.postMessage(json_string)`
//! and Rust responds by calling `webview.evaluate_script()`.

use std::sync::Arc;
use std::sync::Mutex;

use serde_json::json;
use tracing::info;
use wry::WebView;
use vibege_config::ConfigHandle;
use vibege_core::{EventBus, RuntimeEvent};

use raw_window_handle::HasWindowHandle;

/// Controls the embedded WebView2 child window.
pub struct WebViewHandle {
    /// Shared reference so IPC handler can send responses
    webview: Arc<Mutex<Option<WebView>>>,
    visible: bool,
    config: Arc<ConfigHandle>,
    event_bus: Option<Arc<EventBus>>,
}

impl WebViewHandle {
    pub fn new(config: Arc<ConfigHandle>, event_bus: Option<Arc<EventBus>>) -> Self {
        Self {
            webview: Arc::new(Mutex::new(None)),
            visible: false,
            config,
            event_bus,
        }
    }

    pub fn init(&mut self, window_handle: &impl HasWindowHandle) -> Result<(), String> {
        let html = include_str!("../../../resources/ui/index.html");
        info!("Creating webview UI");

        let cfg = Arc::clone(&self.config);
        let eb = self.event_bus.clone();
        let wv_shared = Arc::clone(&self.webview);

        let webview = wry::WebViewBuilder::new()
            .with_html(html.to_string())
            .with_ipc_handler(move |request: wry::http::Request<String>| {
                let body = request.body(); // String body from JavaScript
                let resp = handle_ipc(body, &cfg, &eb);
                if let Ok(guard) = wv_shared.lock() {
                    if let Some(ref wv) = *guard {
                        let js = format!(
                            "window.__VIBEGE_RESPONSE__({})",
                            serde_json::to_string(&resp).unwrap_or_default()
                        );
                        let _ = wv.evaluate_script(&js);
                    }
                }
            })
            .build_as_child(window_handle)
            .map_err(|e| format!("Webview init: {e}"))?;

        webview.set_visible(true);
        *self.webview.lock().unwrap() = Some(webview);
        self.visible = true;
        info!("Webview active");
        Ok(())
    }

    pub fn set_visible(&mut self, visible: bool) {
        if let Ok(guard) = self.webview.lock() {
            if let Some(ref wv) = *guard {
                wv.set_visible(visible);
                self.visible = visible;
            }
        }
    }

    pub fn is_visible(&self) -> bool { self.visible }
    pub fn toggle(&mut self) { self.set_visible(!self.visible); }
}

fn handle_ipc(
    request: &str,
    config: &ConfigHandle,
    event_bus: &Option<Arc<EventBus>>,
) -> serde_json::Value {
    let parsed: serde_json::Value = serde_json::from_str(request).unwrap_or(json!({"path": request}));
    let path = parsed.get("path").and_then(|v| v.as_str()).unwrap_or(request);

    match path {
        "/api/list-installed" => {
            json!({"games": [], "total": 0})
        }
        p if p.starts_with("/api/launch/") => {
            let name = p.trim_start_matches("/api/launch/");
            if let Some(bus) = event_bus {
                bus.publish(&RuntimeEvent::GameStarted { name: name.to_string() });
            }
            json!({"success": true})
        }
        "/api/config" => {
            let data = parsed.get("data");
            if let Some(changes) = data {
                let mut c = config.get();
                if let Some(ov) = changes.get("overlay") {
                    if let Some(mods) = ov.get("hotkey_modifiers").and_then(|v| v.as_str()) {
                        c.overlay.hotkey_modifiers = mods.to_string();
                    }
                    if let Some(key) = ov.get("hotkey_key").and_then(|v| v.as_str()) {
                        c.overlay.hotkey_key = key.to_string();
                    }
                }
                if let Some(audio) = changes.get("audio") {
                    if let Some(vol) = audio.get("volume").and_then(|v| v.as_f64()) {
                        c.audio.volume = vol as f32;
                    }
                }
                if let Some(general) = changes.get("general") {
                    if let Some(sb) = general.get("startup_behavior").and_then(|v| v.as_str()) {
                        c.general.startup_behavior = sb.to_string();
                    }
                    if let Some(pm) = general.get("performance_mode").and_then(|v| v.as_str()) {
                        c.general.performance_mode = pm.to_string();
                    }
                }
                config.set(c);
                json!({"success": true})
            } else {
                // GET config — return current config
                let cfg = config.get();
                serde_json::to_value(&cfg).unwrap_or_default()
            }
        }
        "/api/list-windows" => {
            let windows = enumerate_windows();
            json!({"windows": windows})
        }
        "/api/about" => {
            json!({"version": env!("CARGO_PKG_VERSION"), "name": "VibeGE Runtime"})
        }
        _ => json!({"error": "Not found"})
    }
}

fn enumerate_windows() -> Vec<serde_json::Value> {
    let mut windows = Vec::new();
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, GetWindowTextLengthW, IsWindowVisible,
        };

        unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: isize) -> i32 {
            if IsWindowVisible(hwnd) == 0 { return 1 }
            let len = GetWindowTextLengthW(hwnd);
            if len == 0 { return 1 }
            let mut buf = vec![0u16; (len + 1) as usize];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1);
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            if title.contains("VibeGE") { return 1 }
            let w = &mut *(lparam as *mut Vec<serde_json::Value>);
            w.push(json!({"hwnd": format!("{:#x}", hwnd as u64), "title": title}));
            1
        }

        unsafe {
            EnumWindows(Some(enum_cb), &mut windows as *mut _ as isize);
        }
    }
    windows
}
