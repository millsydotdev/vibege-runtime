mod config;
mod scene;
mod scenes;

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;

use clap::Parser;
use mlua::{Function, Lua};
use tracing::{error, info, warn};
use vibege_audio::AudioSystem;
use vibege_core::{install_panic_hook, logging, LogLevel};
use vibege_input::InputManager;
use vibege_renderer::Renderer;
use vibege_suspension::{SuspensionConfig, SuspensionEngine};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

use scene::{SceneContext, SceneAction, SceneManager};
use scenes::boot_scene::BootScene;
use scenes::game_scene::GameScene;

#[derive(Parser)]
#[command(name = "vibege-runtime", version, about = "VibeGE Game Runtime — AI-friendly overlay")]
struct RuntimeCli {
    #[arg(short = 'p', long = "project", default_value = "", required = false)]
    project_dir: String,

    #[arg(short = 'e', long = "entry", default_value = "", required = false)]
    entry: String,

    #[arg(long = "width", default_value = "800")]
    width: u32,

    #[arg(long = "height", default_value = "600")]
    height: u32,

    #[arg(long = "overlay")]
    overlay: bool,

    #[arg(long = "hidden", default_value_t = false)]
    start_hidden: bool,

    #[arg(long = "show", default_value_t = false)]
    show: bool,
}

#[allow(deprecated)]
fn main() -> anyhow::Result<()> {
    install_panic_hook();
    let cli = RuntimeCli::parse();

    let project_base = if cli.project_dir.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&cli.project_dir)
    };

    let has_game = !cli.entry.is_empty() && !cli.project_dir.is_empty();
    let game_source: Option<String> = if has_game {
        let project_dir = PathBuf::from(&cli.project_dir);
        let game_path = project_dir.join(&cli.entry);
        if game_path.exists() {
            info!(entry = %game_path.display(), "Loading game");
            Some(std::fs::read_to_string(&game_path)?)
        } else {
            warn!("Game entry not found — starting in launcher mode");
            None
        }
    } else {
        info!("Player mode — waiting for hotkey");
        None
    };

    logging::init_logging(LogLevel::Info);

    // Load player config
    let cfg = Arc::new(config::ConfigHandle::new());
    let player_config = cfg.get();
    info!(first_run = cfg.is_first_run(), "Player config loaded");

    let start_visible = cli.overlay || cli.show || has_game || !cli.start_hidden
        || player_config.general.startup_behavior == "shown";

    if start_visible {
        info!("Window will be visible on startup");
    } else {
        info!("Runtime started in background — tray icon active");
    }

    if cfg.is_first_run() {
        info!("First run detected");
    }

    let event_loop = EventLoop::new()
        .map_err(|e| anyhow::anyhow!("Event loop: {e}"))?;

    // Window
    let window = Arc::new(
        event_loop.create_window(
            winit::window::WindowAttributes::default()
                .with_title("VibeGE")
                .with_inner_size(LogicalSize::new(cli.width as f64, cli.height as f64))
                .with_decorations(!cli.overlay)
                .with_visible(start_visible)
        )
        .map_err(|e| anyhow::anyhow!("Window: {e}"))?,
    );

    if cli.overlay {
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOSIZE, SWP_NOMOVE};
            use windows_sys::Win32::Foundation::HWND;
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(w32) = handle.as_ref() {
                    let hwnd = w32.hwnd.get() as HWND;
                    unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE); }
                }
            }
        }
        info!("Overlay mode enabled");
    }

    // System tray
    let _tray_handle = vibege_tray::start();
    if _tray_handle.is_some() {
        info!("System tray active");
    }

    // GPU
    let (w, h) = {
        let size = window.inner_size();
        (size.width, size.height)
    };
    info!("Initialising GPU...");
    let renderer = Arc::new(pollster::block_on(Renderer::new(Arc::clone(&window), w, h))?);
    info!("Renderer ready");

    // Audio
    let audio = AudioSystem::new().map(Arc::new);
    if audio.is_some() {
        info!("Audio system ready");
    }

    // Input
    let input = Arc::new(Mutex::new(InputManager::new()));

    // Suspension engine
    let snap_dir = project_base.join(".vibege").join("snapshots");
    std::fs::create_dir_all(&snap_dir).ok();
    let mut suspension = SuspensionEngine::with_config(SuspensionConfig {
        snapshot_dir: snap_dir,
        enable_compression: false,
        ..Default::default()
    })?;

    // ── Platform Lua VM (temporary — for launcher/first-run migration) ──
    let platform_lua = Rc::new(Lua::new());
    let vibege = platform_lua.create_table().expect("create vibege table");

    // Input bindings for platform Lua
    let input_table = platform_lua.create_table().expect("create input table");
    {
        let inp = Arc::clone(&input);
        input_table.set("is_key_down", platform_lua.create_function(move |_, key: String| {
            Ok(inp.lock().unwrap().is_key_down(key_name_to_code(&key)))
        }).expect("create")).expect("set");
    }
    {
        let inp = Arc::clone(&input);
        input_table.set("is_key_pressed", platform_lua.create_function(move |_, key: String| {
            Ok(inp.lock().unwrap().is_key_pressed(key_name_to_code(&key)))
        }).expect("create")).expect("set");
    }
    vibege.set("input", input_table).expect("set input");

    // Render bindings for platform Lua
    let render_table = platform_lua.create_table().expect("create render table");
    {
        let ren = Arc::clone(&renderer);
        render_table.set("draw_rect", platform_lua.create_function(move |_, (x, y, w, h, r, g, b, a): (f32, f32, f32, f32, f32, f32, f32, f32)| {
            ren.draw_rect(x, y, w, h, r, g, b, a);
            Ok(())
        }).expect("create draw_rect")).expect("set draw_rect");
    }
    {
        let ren = Arc::clone(&renderer);
        render_table.set("clear", platform_lua.create_function(move |_, (bg_r, bg_g, bg_b, bg_a): (f32, f32, f32, f32)| {
            ren.set_clear(bg_r, bg_g, bg_b, bg_a);
            Ok(())
        }).expect("create clear")).expect("set clear");
    }
    {
        let ren = Arc::clone(&renderer);
        render_table.set("draw_text", platform_lua.create_function(move |_, (x, y, text, cw, r, g, b): (f32, f32, String, f32, f32, f32, f32)| {
            ren.draw_text(x, y, &text, cw, r, g, b);
            Ok(())
        }).expect("create draw_text")).expect("set draw_text");
    }
    vibege.set("render", render_table).expect("set render");

    // Audio bindings for platform Lua
    if let Some(ref audio_sys) = audio {
        let audio_table = platform_lua.create_table().expect("create audio table");
        let hit = Arc::new(vibege_audio::generate_test_tone(220.0, 0.08));
        let score = Arc::new(vibege_audio::generate_test_tone(440.0, 0.15));
        let bounce = Arc::new(vibege_audio::generate_test_tone(330.0, 0.05));

        let sys = Arc::clone(audio_sys); let h = Arc::clone(&hit);
        audio_table.set("play_hit", platform_lua.create_function(move |_, ()| { sys.play_sfx(&h); Ok(()) }).expect("")).expect("");
        let sys2 = Arc::clone(audio_sys); let s = Arc::clone(&score);
        audio_table.set("play_score", platform_lua.create_function(move |_, ()| { sys2.play_sfx(&s); Ok(()) }).expect("")).expect("");
        let sys3 = Arc::clone(audio_sys); let b = Arc::clone(&bounce);
        audio_table.set("play_bounce", platform_lua.create_function(move |_, ()| { sys3.play_sfx(&b); Ok(()) }).expect("")).expect("");

        vibege.set("audio", audio_table).expect("set audio");
    }

    // Runtime bindings for platform Lua — switch_game triggers a scene transition
    let pending_switch: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let runtime_table = platform_lua.create_table().expect("create runtime table");
    {
        let ps = Arc::clone(&pending_switch);
        runtime_table.set("switch_game", platform_lua.create_function(move |_, name: String| {
            *ps.lock().unwrap() = Some(name);
            Ok(())
        }).expect("create switch_game")).expect("set switch_game");
    }
    {
        runtime_table.set("list_installed", platform_lua.create_function(move |lua, ()| {
            let game_dirs = installed_games_dir();
            let results = lua.create_table().expect("create results");
            let mut idx = 1;
            if let Ok(entries) = std::fs::read_dir(&game_dirs) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() { continue; }
                    let meta_path = path.join(".vibege-install.json");
                    if !meta_path.exists() { continue; }
                    if let Ok(content) = std::fs::read_to_string(&meta_path) {
                        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                            let name = meta["name"].as_str().unwrap_or("");
                            let entry_p = meta["entry"].as_str().unwrap_or("src/main.lua");
                            if !name.is_empty() {
                                let game_table = lua.create_table().expect("game table");
                                let _ = game_table.set("name", name);
                                let _ = game_table.set("desc", "Installed game");
                                let _ = game_table.set("author", "Local");
                                let _ = game_table.set("status", "installed");
                                let _ = game_table.set("path", path.join(entry_p).to_string_lossy().to_string());
                                let _ = results.set(idx, game_table);
                                idx += 1;
                            }
                        }
                    }
                }
            }
            Ok(results)
        }).expect("create list_installed")).expect("set list_installed");
    }
    vibege.set("runtime", runtime_table).expect("set runtime");

    // Settings bindings
    {
        let cfg_handle = Arc::clone(&cfg);
        let settings_table = platform_lua.create_table().expect("create settings table");

        let ch = Arc::clone(&cfg_handle);
        settings_table.set("get", platform_lua.create_function(move |lua, ()| {
            let c = ch.get();
            let t = lua.create_table().expect("t");
            let _ = t.set("hotkey_modifiers", c.overlay.hotkey_modifiers);
            let _ = t.set("hotkey_key", c.overlay.hotkey_key);
            let _ = t.set("position", c.overlay.position);
            let _ = t.set("width", c.overlay.width as i64);
            let _ = t.set("height", c.overlay.height as i64);
            let _ = t.set("volume", c.audio.volume);
            let _ = t.set("startup_behavior", c.general.startup_behavior);
            let _ = t.set("performance_mode", c.general.performance_mode);
            Ok(t)
        }).expect("create settings get")).expect("set settings get");

        let ch2 = Arc::clone(&cfg_handle);
        settings_table.set("set", platform_lua.create_function(move |_, (key, value): (String, String)| {
            let mut c = ch2.get();
            match key.as_str() {
                "hotkey_modifiers" => c.overlay.hotkey_modifiers = value,
                "hotkey_key" => c.overlay.hotkey_key = value,
                "position" => c.overlay.position = value,
                "volume" => { if let Ok(v) = value.parse::<f32>() { c.audio.volume = v.clamp(0.0, 1.0); } }
                "startup_behavior" => c.general.startup_behavior = value,
                "performance_mode" => c.general.performance_mode = value,
                _ => return Err(mlua::Error::RuntimeError(format!("Unknown setting: {key}"))),
            }
            ch2.set(c);
            Ok(())
        }).expect("create settings set")).expect("set settings set");

        let ch3 = Arc::clone(&cfg_handle);
        settings_table.set("is_first_run", platform_lua.create_function(move |_, ()| {
            Ok(ch3.is_first_run())
        }).expect("create is_first_run")).expect("set is_first_run");

        vibege.set("settings", settings_table).expect("set settings");
    }

    platform_lua.globals().set("vibege", vibege).expect("set vibege globals");

    // Embedded game sources for switch_game fallback
    let launcher_src = include_str!("../../../resources/launcher.lua");
    let demo_src = include_str!("../../../resources/demo-game.lua");
    let first_run_src = include_str!("../../../resources/first-run.lua");
    let embedded_games: HashMap<&str, &str> = [
        ("launcher", launcher_src),
        ("demo", demo_src),
        ("first-run", first_run_src),
    ].into_iter().collect();

    // Track whether a game was loaded from CLI (for snapshot restore, etc.)
    let has_game = game_source.is_some();

    // Clone audio for the event loop closure
    let audio_clone = audio.clone();

    // ── Scene Manager ──
    let mut scene_ctx = SceneContext::new(
        w, h,
        Arc::clone(&renderer),
        Arc::clone(&input),
        Arc::clone(&cfg),
        Rc::clone(&platform_lua),
    );
    let mut scene_manager = scene::SceneManager::new();
    scene_manager.push(Box::new(BootScene::new()), &mut scene_ctx)
        .map_err(|e| anyhow::anyhow!("Scene push: {e}"))?;

    // Main loop
    info!("Entering main loop");
    let mut last_frame = std::time::Instant::now();
    let mut state_restored = false;

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { event, .. } => {
                input.lock().unwrap().handle_window_event(&event);

                match event {
                    WindowEvent::CloseRequested => {
                        // Save game state if a game is active
                        info!("Window closed");
                        elwt.exit();
                    }
                    WindowEvent::Focused(true) => {
                        info!("Focus gained");
                    }
                    WindowEvent::Focused(false) => {
                        info!("Focus lost");
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                // Hotkey polling
                #[cfg(target_os = "windows")]
                {
                    let k_mod = cfg.get().overlay.hotkey_modifiers.clone();
                    let k_key = cfg.get().overlay.hotkey_key.clone();
                    let (mod_ctrl, mod_shift, mod_alt) = (
                        k_mod.contains("ctrl"),
                        k_mod.contains("shift"),
                        k_mod.contains("alt"),
                    );
                    unsafe {
                        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
                        let ctrl = GetAsyncKeyState(0x11);
                        let shift = GetAsyncKeyState(0x10);
                        let alt = GetAsyncKeyState(0x12);
                        let vk = match k_key.as_str() {
                            "v" => 0x56, "g" => 0x47, "b" => 0x42,
                            "h" => 0x48, "space" => 0x20, "tab" => 0x09,
                            _ => 0x56,
                        };
                        let key = GetAsyncKeyState(vk);
                        let mod_match = (!mod_ctrl || ctrl < 0) && (!mod_shift || shift < 0) && (!mod_alt || alt < 0);
                        if mod_match && key < 0 {
                            vibege_tray::request_toggle();
                        }
                    }
                }

                // Tray signals
                if vibege_tray::should_show_launcher() {
                    window.set_visible(true);
                }
                if vibege_tray::should_toggle_overlay() {
                    let visible = window.is_visible().unwrap_or(true);
                    window.set_visible(!visible);
                }
                if vibege_tray::should_quit() {
                    info!("Quit requested from tray");
                    scene_manager.shutdown(&mut scene_ctx);
                    elwt.exit();
                    return;
                }

                // Restore saved state once
                if has_game && !state_restored {
                    state_restored = true;
                    if let Some(snap) = suspension.list_snapshots().first().cloned() {
                        if let Ok(snapshot) = suspension.resume(&snap.id) {
                            // TODO: route to game scene restore_state
                            info!("Snapshot found, would restore");
                        }
                    }
                }

                // Time
                let now = std::time::Instant::now();
                let dt = now.duration_since(last_frame).as_secs_f64();
                last_frame = now;

                // Update scene manager
                let action = match scene_manager.update(&mut scene_ctx, dt) {
                    Ok(action) => action,
                    Err(e) => {
                        error!("Scene error: {e}");
                        SceneAction::Exit
                    }
                };

                // Apply navigation action from scene
                if let Err(e) = scene_manager.apply(action, &mut scene_ctx) {
                    error!("Navigation error: {e}");
                }

                // Check for pending game switch (from platform Lua's switch_game)
                if let Some(game_name) = pending_switch.lock().unwrap().take() {
                    let script = embedded_games.get(game_name.as_str()).copied();
                    let src = script.unwrap_or_else(|| {
                        // Try loading from file
                        let path = PathBuf::from(&game_name);
                        if path.exists() {
                            info!(game = %game_name, "Loading game from file");
                            Box::leak(std::fs::read_to_string(&path).unwrap_or_default().into_boxed_str())
                        } else {
                            ""
                        }
                    });

                    if !src.is_empty() {
                        let game_scene = GameScene::new(
                            src.to_string(),
                            Arc::clone(&renderer),
                            Arc::clone(&input),
                            audio_clone.clone(),
                        );
                        let _ = scene_manager.push(Box::new(game_scene), &mut scene_ctx);
                    }
                }

                // Render scene
                let action = match scene_manager.render(&mut scene_ctx) {
                    Ok(action) => action,
                    Err(e) => {
                        error!("Scene render error: {e}");
                        SceneAction::Exit
                    }
                };
                if let Err(e) = scene_manager.apply(action, &mut scene_ctx) {
                    error!("Navigation error: {e}");
                }

                // Present
                if let Err(e) = renderer.render() {
                    error!("GPU render: {e}");
                }

                input.lock().unwrap().end_frame();
                window.request_redraw();

                // Check if scene manager wants to exit
                if scene_manager.is_empty() {
                    elwt.exit();
                }
            }
            _ => {}
        }
    }).map_err(|e| anyhow::anyhow!("Event loop: {e}"))?;

    info!("Runtime exited");
    Ok(())
}

/// Return the directory where locally-installed games are stored.
fn installed_games_dir() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("vibege").join("games")
    } else {
        PathBuf::from(".vibege/installed-games")
    }
}

fn key_name_to_code(name: &str) -> KeyCode {
    match name.to_lowercase().as_str() {
        "a" => KeyCode::KeyA, "b" => KeyCode::KeyB, "c" => KeyCode::KeyC,
        "d" => KeyCode::KeyD, "e" => KeyCode::KeyE, "f" => KeyCode::KeyF,
        "g" => KeyCode::KeyG, "h" => KeyCode::KeyH, "i" => KeyCode::KeyI,
        "j" => KeyCode::KeyJ, "k" => KeyCode::KeyK, "l" => KeyCode::KeyL,
        "m" => KeyCode::KeyM, "n" => KeyCode::KeyN, "o" => KeyCode::KeyO,
        "p" => KeyCode::KeyP, "q" => KeyCode::KeyQ, "r" => KeyCode::KeyR,
        "s" => KeyCode::KeyS, "t" => KeyCode::KeyT, "u" => KeyCode::KeyU,
        "v" => KeyCode::KeyV, "w" => KeyCode::KeyW, "x" => KeyCode::KeyX,
        "y" => KeyCode::KeyY, "z" => KeyCode::KeyZ,
        "0" => KeyCode::Digit0, "1" => KeyCode::Digit1, "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3, "4" => KeyCode::Digit4, "5" => KeyCode::Digit5,
        "6" => KeyCode::Digit6, "7" => KeyCode::Digit7, "8" => KeyCode::Digit8,
        "9" => KeyCode::Digit9,
        "f1" => KeyCode::F1, "f2" => KeyCode::F2, "f3" => KeyCode::F3,
        "f4" => KeyCode::F4, "f5" => KeyCode::F5, "f6" => KeyCode::F6,
        "f7" => KeyCode::F7, "f8" => KeyCode::F8, "f9" => KeyCode::F9,
        "f10" => KeyCode::F10, "f11" => KeyCode::F11, "f12" => KeyCode::F12,
        "up" => KeyCode::ArrowUp, "down" => KeyCode::ArrowDown,
        "left" => KeyCode::ArrowLeft, "right" => KeyCode::ArrowRight,
        "shift" => KeyCode::ShiftLeft, "ctrl" | "control" => KeyCode::ControlLeft,
        "alt" => KeyCode::AltLeft, "tab" => KeyCode::Tab,
        "space" => KeyCode::Space, "enter" | "return" => KeyCode::Enter,
        "escape" | "esc" => KeyCode::Escape, "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete, "home" => KeyCode::Home, "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp, "pagedown" => KeyCode::PageDown,
        _ => KeyCode::Space,
    }
}
