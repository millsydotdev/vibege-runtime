use std::collections::HashMap;
use std::path::PathBuf;
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
}

#[allow(deprecated)] // winit 0.30 EventLoop::create_window / run — still works, not worth the ApplicationHandler refactor yet
fn main() -> anyhow::Result<()> {
    install_panic_hook();
    let cli = RuntimeCli::parse();

    let project_base = if cli.project_dir.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&cli.project_dir)
    };
    let launcher_source = include_str!("../../../resources/launcher.lua");
    let demo_source = include_str!("../../../resources/demo-game.lua");

    // Embedded games accessible via vibege.runtime.switch_game(name)
    let embedded_games: HashMap<&str, &str> = [
        ("launcher", launcher_source),
        ("demo", demo_source),
    ].into_iter().collect();

    let has_game = !cli.entry.is_empty() && !cli.project_dir.is_empty();
    let game_source: Option<String> = if has_game {
        let project_dir = PathBuf::from(&cli.project_dir);
        let game_path = project_dir.join(&cli.entry);
        if game_path.exists() {
            info!(entry = %game_path.display(), "Loading game");
            Some(std::fs::read_to_string(&game_path)?)
        } else {
            warn!("Game entry not found: {} — starting in launcher mode", game_path.display());
            None
        }
    } else {
        info!("No game specified — launcher mode");
        None
    };

    logging::init_logging(LogLevel::Info);
    if let Some(ref src) = game_source {
        info!("Game source loaded ({} bytes)", src.len());
    } else {
        info!("Launcher mode — waiting for game selection");
    }

    let event_loop = EventLoop::new()
        .map_err(|e| anyhow::anyhow!("Event loop: {e}"))?;

    // Window setup — overlay mode uses borderless, centered window
    let window = Arc::new(
        event_loop.create_window(
            winit::window::WindowAttributes::default()
                .with_title("VibeGE")
                .with_inner_size(LogicalSize::new(cli.width as f64, cli.height as f64))
                .with_decorations(!cli.overlay)
        )
        .map_err(|e| anyhow::anyhow!("Window: {e}"))?,
    );

    if cli.overlay {
        // Make window topmost via Windows API
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
        info!("Overlay mode enabled — Ctrl+Shift+V to toggle");
    }

    // Start system tray (if available)
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
    let input = Arc::new(std::sync::Mutex::new(InputManager::new()));

    // Suspension engine — saves/resumes game state
    let snap_dir = project_base.join(".vibege").join("snapshots");
    std::fs::create_dir_all(&snap_dir).ok();
    let mut suspension = SuspensionEngine::with_config(SuspensionConfig {
        snapshot_dir: snap_dir,
        enable_compression: false,
        ..Default::default()
    })?;

    // Lua VM
    let lua = Lua::new();
    let vibege = lua.create_table().expect("create vibege table");

    // Input bindings
    let input_table = lua.create_table().expect("create input table");
    {
        let inp = Arc::clone(&input);
        input_table.set("is_key_down", lua.create_function(move |_, key: String| {
            Ok(inp.lock().unwrap().is_key_down(key_name_to_code(&key)))
        }).expect("create")).expect("set");
    }
    {
        let inp = Arc::clone(&input);
        input_table.set("is_key_pressed", lua.create_function(move |_, key: String| {
            Ok(inp.lock().unwrap().is_key_pressed(key_name_to_code(&key)))
        }).expect("create")).expect("set");
    }
    {
        let inp = Arc::clone(&input);
        input_table.set("mouse_position", lua.create_function(move |_, ()| {
            let p = inp.lock().unwrap().mouse_position();
            Ok((p.0, p.1))
        }).expect("create")).expect("set");
    }
    vibege.set("input", input_table).expect("set input");

    // Render bindings — deferred rendering
    let render_table = lua.create_table().expect("create render table");

    // draw_rect(x, y, w, h, r, g, b, a) — colored rectangle
    {
        let ren = Arc::clone(&renderer);
        render_table.set("draw_rect", lua.create_function(move |_, (x, y, w, h, r, g, b, a): (f32, f32, f32, f32, f32, f32, f32, f32)| {
            ren.draw_rect(x, y, w, h, r, g, b, a);
            Ok(())
        }).expect("create draw_rect")).expect("set draw_rect");
    }

    // load_texture(filepath) — load PNG from file, returns texture index
    {
        let ren = Arc::clone(&renderer);
        render_table.set("load_texture", lua.create_function(move |_, path: String| {
            match std::fs::read(&path) {
                Ok(data) => {
                    match ren.load_texture(&data) {
                        Ok(idx) => Ok(idx as i64),
                        Err(e) => Err(mlua::Error::RuntimeError(format!("Texture error: {e}"))),
                    }
                }
                Err(e) => Err(mlua::Error::RuntimeError(format!("File error: {e}"))),
            }
        }).expect("create load_texture")).expect("set load_texture");
    }

    // draw_sprite(tex_index, x, y, w, h) — draw textured sprite
    {
        let ren = Arc::clone(&renderer);
        render_table.set("draw_sprite", lua.create_function(move |_, (idx, x, y, w, h): (i64, f32, f32, f32, f32)| {
            ren.draw_sprite(idx as usize, x, y, w, h);
            Ok(())
        }).expect("create draw_sprite")).expect("set draw_sprite");
    }

    // clear(r, g, b, a) — set background color
    {
        let ren = Arc::clone(&renderer);
        render_table.set("clear", lua.create_function(move |_, (bg_r, bg_g, bg_b, bg_a): (f32, f32, f32, f32)| {
            ren.set_clear(bg_r, bg_g, bg_b, bg_a);
            Ok(())
        }).expect("create clear")).expect("set clear");
    }

    // draw_text(x, y, text, char_w, r, g, b) — bitmap text
    {
        let ren = Arc::clone(&renderer);
        render_table.set("draw_text", lua.create_function(move |_, (x, y, text, char_w, r, g, b): (f32, f32, String, f32, f32, f32, f32)| {
            ren.draw_text(x, y, &text, char_w, r, g, b);
            Ok(())
        }).expect("create draw_text")).expect("set draw_text");
    }

    vibege.set("render", render_table).expect("set render");

    // Audio bindings
    if let Some(ref audio_sys) = audio {
        let audio_table = lua.create_table().expect("create audio table");
        let hit = Arc::new(vibege_audio::generate_test_tone(220.0, 0.08));
        let score = Arc::new(vibege_audio::generate_test_tone(440.0, 0.15));
        let bounce = Arc::new(vibege_audio::generate_test_tone(330.0, 0.05));

        let sys = Arc::clone(audio_sys); let h = Arc::clone(&hit);
        audio_table.set("play_hit", lua.create_function(move |_, ()| { sys.play_sfx(&h); Ok(()) }).expect("")).expect("");

        let sys2 = Arc::clone(audio_sys); let s = Arc::clone(&score);
        audio_table.set("play_score", lua.create_function(move |_, ()| { sys2.play_sfx(&s); Ok(()) }).expect("")).expect("");

        let sys3 = Arc::clone(audio_sys); let b = Arc::clone(&bounce);
        audio_table.set("play_bounce", lua.create_function(move |_, ()| { sys3.play_sfx(&b); Ok(()) }).expect("")).expect("");

        vibege.set("audio", audio_table).expect("set audio");
    }

    // Time bindings
    let time_table = lua.create_table().expect("create time table");
    let start_time = std::time::Instant::now();
    time_table.set("delta_time", lua.create_function(move |_, ()| Ok(0.016)).expect("create delta")).expect("set delta");
    let st = start_time;
    time_table.set("elapsed", lua.create_function(move |_, ()| Ok(st.elapsed().as_secs_f64())).expect("create elapsed")).expect("set elapsed");
    vibege.set("time", time_table).expect("set time");

    // Runtime bindings — game switching
    let pending_switch: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let runtime_table = lua.create_table().expect("create runtime table");
    {
        let ps = Arc::clone(&pending_switch);
        runtime_table.set("switch_game", lua.create_function(move |_, name: String| {
            *ps.lock().unwrap() = Some(name);
            Ok(())
        }).expect("create switch_game")).expect("set switch_game");
    }
    vibege.set("runtime", runtime_table).expect("set runtime");

    lua.globals().set("vibege", vibege).expect("set vibege globals");

    // Load game or launcher
    let game_script = game_source.as_deref().unwrap_or(launcher_source);
    let is_launcher = game_source.is_none();

    info!(is_launcher = is_launcher, "Loading game script");
    if let Err(e) = lua.load(game_script).exec() {
        warn!("Script error: {e}");
    } else if let Ok(init_fn) = lua.globals().get::<Function>("init") {
        let _ = init_fn.call::<()>(());
    }

    // Main loop
    info!("Entering main loop");
    let mut last_frame = std::time::Instant::now();
    let has_lua_game = !is_launcher;

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { event, .. } => {
                input.lock().unwrap().handle_window_event(&event);

                match event {
                    WindowEvent::CloseRequested => {
                        if has_lua_game {
                            if let Ok(state_fn) = lua.globals().get::<Function>("get_state") {
                                if let Ok(state) = state_fn.call::<String>("") {
                                    let _ = suspension.suspend(state.as_bytes(), 0.0, "last-session");
                                    info!("Game state saved");
                                }
                            }
                        }
                        info!("Window closed");
                        elwt.exit();
                    }
                    WindowEvent::KeyboardInput { event: ke, .. } => {
                        if ke.physical_key == PhysicalKey::Code(KeyCode::Escape)
                            && ke.state == ElementState::Pressed
                        {
                            elwt.exit();
                        }
                    }
                    WindowEvent::Focused(false) => {
                        info!("Focus lost");
                        if has_lua_game {
                            if let Ok(suspend_fn) = lua.globals().get::<Function>("suspend") {
                                let _ = suspend_fn.call::<()>(());
                            }
                        }
                    }
                    WindowEvent::Focused(true) => {
                        info!("Focus gained");
                        if has_lua_game {
                            if let Ok(resume_fn) = lua.globals().get::<Function>("resume") {
                                let _ = resume_fn.call::<()>(());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                // Check for global hotkey Ctrl+Shift+V via platform APIs
                #[cfg(target_os = "windows")]
                {
                    // Use GetAsyncKeyState for global hotkey polling
                    // VK_CONTROL = 0x11, VK_SHIFT = 0x10, VK_V = 0x56
                    // Only check every 10 frames to reduce CPU usage
                    const VK_CONTROL: i32 = 0x11;
                    const VK_SHIFT: i32 = 0x10;
                    const VK_V: i32 = 0x56;
                    // Check with bit 15 (most significant bit) for current press state
                    // This is a simple polling approach that works across windows
                    unsafe {
                        let ctrl = windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(VK_CONTROL);
                        let shift = windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(VK_SHIFT);
                        let v = windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(VK_V);
                        if ctrl < 0 && shift < 0 && v < 0 {
                            // All three are pressed — toggle overlay once
                            // We check for the V key being pressed THIS frame
                            vibege_tray::request_toggle();
                        }
                    }
                }

                // Handle tray signals
                if vibege_tray::should_show_launcher() {
                    info!("Launcher requested — would show game store");
                    window.set_visible(true);
                }
                if vibege_tray::should_toggle_overlay() {
                    let visible = window.is_visible().unwrap_or(true);
                    window.set_visible(!visible);
                    info!(visible = !visible, "Overlay toggled via hotkey");
                }
                if vibege_tray::should_quit() {
                    info!("Quit requested from tray");
                    elwt.exit();
                    return;
                }
                // Restore saved state (game mode only)
                if has_lua_game {
                    let snap_id = suspension.list_snapshots().first().map(|s| s.id.clone());
                    if let Some(ref id) = snap_id {
                        if let Ok(snapshot) = suspension.resume(id) {
                            if let Ok(restore_fn) = lua.globals().get::<Function>("restore_state") {
                                let state_str = String::from_utf8_lossy(&snapshot.game_state).to_string();
                                let _ = restore_fn.call::<()>(state_str);
                            }
                        }
                    }
                }

                let now = std::time::Instant::now();
                let dt = now.duration_since(last_frame).as_secs_f64();
                last_frame = now;

                // Update time
                if let Ok(v) = lua.globals().get::<mlua::Table>("vibege") {
                    if let Ok(t) = v.get::<mlua::Table>("time") {
                        let stc = start_time;
                        let _ = t.set("delta_time", lua.create_function(move |_, ()| Ok(dt)).expect("dt"));
                        let _ = t.set("elapsed", lua.create_function(move |_, ()| Ok(stc.elapsed().as_secs_f64())).expect("elapsed"));
                    }
                }

                // Always call Lua update/render — the launcher IS a Lua game
                if let Ok(update_fn) = lua.globals().get::<Function>("update") {
                    if let Err(e) = update_fn.call::<()>(dt) {
                        error!("update(): {e}");
                        elwt.exit();
                        return;
                    }
                }
                if let Ok(render_fn) = lua.globals().get::<Function>("render") {
                    if let Err(e) = render_fn.call::<()>(()) {
                        error!("render(): {e}");
                        elwt.exit();
                        return;
                    }
                }

                // Check for game switch request from Lua
                if let Some(game_name) = pending_switch.lock().unwrap().take() {
                    let script = embedded_games.get(game_name.as_str()).copied();
                    if let Some(src) = script {
                        info!(game = %game_name, "Switching to embedded game");
                        if let Err(e) = lua.load(src).exec() {
                            warn!("Script error: {e}");
                        } else if let Ok(init_fn) = lua.globals().get::<Function>("init") {
                            let _ = init_fn.call::<()>(());
                        }
                    } else {
                        // Try loading from file
                        let path = PathBuf::from(&game_name);
                        if path.exists() {
                            info!(game = %game_name, "Loading game from file");
                            if let Ok(src) = std::fs::read_to_string(&path) {
                                if let Err(e) = lua.load(&src).exec() {
                                    warn!("Script error: {e}");
                                } else if let Ok(init_fn) = lua.globals().get::<Function>("init") {
                                    let _ = init_fn.call::<()>(());
                                }
                            }
                        } else {
                            warn!(game = %game_name, "Unknown game: not embedded and not a file");
                        }
                    }
                }

                // Present everything after render
                if let Err(e) = renderer.render() {
                    error!("GPU render: {e}");
                }

                input.lock().unwrap().end_frame();
                window.request_redraw();
            }
            _ => {}
        }
    }).map_err(|e| anyhow::anyhow!("Event loop: {e}"))?;

    info!("Runtime exited");
    Ok(())
}

fn key_name_to_code(name: &str) -> KeyCode {
    match name.to_lowercase().as_str() {
        // Letters
        "a" => KeyCode::KeyA, "b" => KeyCode::KeyB, "c" => KeyCode::KeyC,
        "d" => KeyCode::KeyD, "e" => KeyCode::KeyE, "f" => KeyCode::KeyF,
        "g" => KeyCode::KeyG, "h" => KeyCode::KeyH, "i" => KeyCode::KeyI,
        "j" => KeyCode::KeyJ, "k" => KeyCode::KeyK, "l" => KeyCode::KeyL,
        "m" => KeyCode::KeyM, "n" => KeyCode::KeyN, "o" => KeyCode::KeyO,
        "p" => KeyCode::KeyP, "q" => KeyCode::KeyQ, "r" => KeyCode::KeyR,
        "s" => KeyCode::KeyS, "t" => KeyCode::KeyT, "u" => KeyCode::KeyU,
        "v" => KeyCode::KeyV, "w" => KeyCode::KeyW, "x" => KeyCode::KeyX,
        "y" => KeyCode::KeyY, "z" => KeyCode::KeyZ,
        // Digits
        "0" => KeyCode::Digit0, "1" => KeyCode::Digit1, "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3, "4" => KeyCode::Digit4, "5" => KeyCode::Digit5,
        "6" => KeyCode::Digit6, "7" => KeyCode::Digit7, "8" => KeyCode::Digit8,
        "9" => KeyCode::Digit9,
        // Function keys
        "f1" => KeyCode::F1, "f2" => KeyCode::F2, "f3" => KeyCode::F3,
        "f4" => KeyCode::F4, "f5" => KeyCode::F5, "f6" => KeyCode::F6,
        "f7" => KeyCode::F7, "f8" => KeyCode::F8, "f9" => KeyCode::F9,
        "f10" => KeyCode::F10, "f11" => KeyCode::F11, "f12" => KeyCode::F12,
        // Navigation
        "up" => KeyCode::ArrowUp, "down" => KeyCode::ArrowDown,
        "left" => KeyCode::ArrowLeft, "right" => KeyCode::ArrowRight,
        // Modifiers
        "shift" => KeyCode::ShiftLeft, "ctrl" | "control" => KeyCode::ControlLeft,
        "alt" => KeyCode::AltLeft, "tab" => KeyCode::Tab,
        "space" => KeyCode::Space, "enter" | "return" => KeyCode::Enter,
        "escape" | "esc" => KeyCode::Escape, "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete, "home" => KeyCode::Home, "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp, "pagedown" => KeyCode::PageDown,
        // Default
        _ => KeyCode::Space,
    }
}
