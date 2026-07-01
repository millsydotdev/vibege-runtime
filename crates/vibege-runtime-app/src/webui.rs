//! WebView UI — the phone-like overlay gaming app UI.
//!
//! Uses wry's built-in IPC: JavaScript calls `window.ipc.postMessage(json_string)`
//! and Rust responds by calling `webview.evaluate_script()`.

use std::io::Read;
use std::sync::Arc;
use std::sync::Mutex;

use serde_json::json;
use tracing::info;
use vibege_config::ConfigHandle;
use vibege_core::{EventBus, RuntimeEvent};
use wry::WebView;

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

    pub fn is_visible(&self) -> bool {
        self.visible
    }
    pub fn toggle(&mut self) {
        self.set_visible(!self.visible);
    }
}

fn handle_ipc(
    request: &str,
    config: &ConfigHandle,
    event_bus: &Option<Arc<EventBus>>,
) -> serde_json::Value {
    let parsed: serde_json::Value =
        serde_json::from_str(request).unwrap_or(json!({"path": request}));
    let path = parsed
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(request);
    let data = parsed.get("data");

    match path {
        // ── List installed games (scan ~/.vibege/games/) ──
        "/api/list-installed" => {
            let games = scan_installed_games(config);
            json!({"games": games, "total": games.len()})
        }

        // ── Launch a game ──
        p if p.starts_with("/api/launch/") => {
            let name = p.trim_start_matches("/api/launch/");
            if let Some(bus) = event_bus {
                bus.publish(&RuntimeEvent::GameStarted {
                    name: name.to_string(),
                });
            }
            json!({"success": true})
        }

        // ── List store games (fetch from backend API) ──
        "/api/list-store" => match list_store_games(config) {
            Ok(games) => json!({"games": games, "total": games.len()}),
            Err(e) => json!({"error": e, "games": [], "total": 0}),
        },

        // ── Install a game from the store ──
        "/api/install" => {
            if let Some(d) = data {
                let game_id = d.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let game_name = d.get("name").and_then(|v| v.as_str()).unwrap_or("");
                match install_game(config, game_id, game_name) {
                    Ok(()) => {
                        let games = scan_installed_games(config);
                        json!({"success": true, "games": games, "total": games.len()})
                    }
                    Err(e) => json!({"success": false, "error": e}),
                }
            } else {
                json!({"success": false, "error": "Missing data"})
            }
        }

        // ── Get/set config ──
        "/api/config" => {
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

        _ => json!({"error": "Not found"}),
    }
}

fn scan_installed_games(config: &ConfigHandle) -> Vec<serde_json::Value> {
    let games_dir = vibege_config::installed_games_dir();
    let mut games = Vec::new();

    if !games_dir.exists() {
        return games;
    }

    if let Ok(entries) = std::fs::read_dir(&games_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Try to read vibege.json/manifest.json for metadata
                let mut entry_path = String::from("src/main.lua");
                let mut version = String::from("0.1.0");
                for manifest_name in &["vibege.json", "manifest.json"] {
                    let mf = path.join(manifest_name);
                    if let Ok(content) = std::fs::read_to_string(&mf) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(ep) = json["entry"].as_str() {
                                entry_path = ep.to_string();
                            }
                            if let Some(v) = json["version"].as_str() {
                                version = v.to_string();
                            }
                        }
                    }
                }

                games.push(json!({
                    "id": name,
                    "name": name,
                    "version": version,
                    "entry": entry_path,
                    "installed": true,
                }));
            }
        }
    }
    games
}

fn list_store_games(config: &ConfigHandle) -> Result<Vec<serde_json::Value>, String> {
    let backend_url = config.get().general.backend_url;
    let url = format!("{}/registry?limit=50", backend_url);

    let mut body_str = String::new();
    ureq::get(&url)
        .call()
        .map_err(|e| format!("Failed to fetch store: {e}"))?
        .into_body()
        .into_reader()
        .read_to_string(&mut body_str)
        .map_err(|e| format!("Failed to read store response: {e}"))?;

    let body: serde_json::Value = serde_json::from_str(&body_str)
        .map_err(|e| format!("Failed to parse store response: {e}"))?;

    let packages = body
        .get("packages")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    let installed = scan_installed_games(config);
    let installed_ids: Vec<String> = installed
        .iter()
        .filter_map(|g| g.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();

    let mut result = Vec::new();
    for pkg in packages {
        let id = pkg
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let already_installed = installed_ids.contains(&id)
            || installed_ids.contains(
                &pkg.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase(),
            );
        result.push(json!({
            "id": id,
            "name": pkg.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown"),
            "description": pkg.get("description").and_then(|v| v.as_str()).unwrap_or(""),
            "version": pkg.get("version").and_then(|v| v.as_str()).unwrap_or("0.1.0"),
            "owner": pkg.get("ownerName").and_then(|v| v.as_str()).unwrap_or("Unknown"),
            "rating": pkg.get("rating").and_then(|v| v.as_f64()).unwrap_or(0.0),
            "downloads": pkg.get("downloads").and_then(|v| v.as_i64()).unwrap_or(0),
            "tags": pkg.get("tags").and_then(|v| v.as_array()).cloned().unwrap_or_default(),
            "installed": already_installed,
        }));
    }
    Ok(result)
}

fn install_game(config: &ConfigHandle, game_id: &str, game_name: &str) -> Result<(), String> {
    let backend_url = config.get().general.backend_url;
    let url = format!("{}/registry/{}/download", backend_url, game_id);

    // Download via ureq (blocking, runs in IPC thread — OK for short downloads)
    let mut data: Vec<u8> = Vec::new();
    ureq::get(&url)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?
        .into_body()
        .into_reader()
        .read_to_end(&mut data)
        .map_err(|e| format!("Read failed: {e}"))?;

    // Extract .vibepkg (ZIP) to the installed games directory
    let install_dir = vibege_config::installed_games_dir().join(sanitize_name(game_name));
    std::fs::create_dir_all(&install_dir).map_err(|e| format!("Create dir failed: {e}"))?;

    let cursor = std::io::Cursor::new(&data);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid package: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Zip entry {i}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        // Basic path traversal protection
        let name = entry.name().replace("..", "_").replace('\\', "/");
        let out_path = install_dir.join(&name);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .map_err(|e| format!("Read entry {i}: {e}"))?;
        std::fs::write(&out_path, &content).map_err(|e| format!("Write {name}: {e}"))?;
    }

    info!("Installed {} to {:?}", game_name, install_dir);
    Ok(())
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn enumerate_windows() -> Vec<serde_json::Value> {
    let mut windows = Vec::new();
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
        };

        unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: isize) -> i32 {
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let len = GetWindowTextLengthW(hwnd);
            if len == 0 {
                return 1;
            }
            let mut buf = vec![0u16; (len + 1) as usize];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1);
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            if title.contains("VibeGE") {
                return 1;
            }
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
