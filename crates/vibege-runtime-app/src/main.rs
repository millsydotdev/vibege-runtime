use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use mlua::{Function, Lua};
use tracing::{error, info};
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
    #[arg(short = 'p', long = "project", default_value = ".")]
    project_dir: String,

    #[arg(short = 'e', long = "entry", default_value = "src/main.lua")]
    entry: String,

    #[arg(long = "width", default_value = "800")]
    width: u32,

    #[arg(long = "height", default_value = "600")]
    height: u32,

    #[arg(long = "overlay")]
    overlay: bool,
}

fn main() -> anyhow::Result<()> {
    install_panic_hook();
    let cli = RuntimeCli::parse();

    let project_dir = PathBuf::from(&cli.project_dir);
    let game_path = project_dir.join(&cli.entry);
    if !game_path.exists() {
        anyhow::bail!("Game entry not found: {}", game_path.display());
    }
    let game_source = std::fs::read_to_string(&game_path)?;

    logging::init_logging(LogLevel::Info);
    info!(entry = %game_path.display(), overlay = cli.overlay, "VibeGE Runtime");

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
        info!("Overlay mode enabled — game stays on top of editor");
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
    let snap_dir = project_dir.join(".vibege").join("snapshots");
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

    // Render bindings
    let render_table = lua.create_table().expect("create render table");
    let ren = Arc::clone(&renderer);
    render_table.set("draw_rect", lua.create_function(move |_, (x, y, w, h, r, g, b, a): (f32, f32, f32, f32, f32, f32, f32, f32)| {
        ren.draw_rect(x, y, w, h, r, g, b, a);
        Ok(())
    }).expect("create draw_rect")).expect("set draw_rect");
    let ren2 = Arc::clone(&renderer);
    render_table.set("clear", lua.create_function(move |_, (bg_r, bg_g, bg_b, bg_a): (f32, f32, f32, f32)| {
        let _ = ren2.present(bg_r, bg_g, bg_b, bg_a);
        Ok(())
    }).expect("create clear")).expect("set clear");
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

    lua.globals().set("vibege", vibege).expect("set vibege globals");

    // Load game
    info!("Loading game");
    lua.load(&game_source).exec().map_err(|e| anyhow::anyhow!("Lua: {e}"))?;

    if let Ok(init_fn) = lua.globals().get::<Function>("init") {
        let _ = init_fn.call::<()>(());
    }

    // Main loop
    info!("Entering game loop");
    let mut last_frame = std::time::Instant::now();

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { event, .. } => {
                input.lock().unwrap().handle_window_event(&event);

                match event {
                    WindowEvent::CloseRequested => {
                        // Save state on close
                        if let Ok(state_fn) = lua.globals().get::<Function>("get_state") {
                            if let Ok(state) = state_fn.call::<String>("") {
                                let _ = suspension.suspend(state.as_bytes(), 0.0, "last-session");
                                info!("Game state saved");
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
                        // Auto-suspend on focus loss (overlay hidden)
                        info!("Focus lost — suspending");
                        if let Ok(suspend_fn) = lua.globals().get::<Function>("suspend") {
                            let _ = suspend_fn.call::<()>(());
                        }
                    }
                    WindowEvent::Focused(true) => {
                        // Auto-resume on focus regain
                        info!("Focus gained — resuming");
                        if let Ok(resume_fn) = lua.globals().get::<Function>("resume") {
                            let _ = resume_fn.call::<()>(());
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                // Check if we have a saved state to restore
                let snap_id = suspension.list_snapshots().first().map(|s| s.id.clone());
                if let Some(ref id) = snap_id {
                    if let Ok(snapshot) = suspension.resume(id) {
                        if let Ok(restore_fn) = lua.globals().get::<Function>("restore_state") {
                            let state_str = String::from_utf8_lossy(&snapshot.game_state).to_string();
                            let _ = restore_fn.call::<()>(state_str);
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

                // update(dt)
                if let Ok(update_fn) = lua.globals().get::<Function>("update") {
                    if let Err(e) = update_fn.call::<()>(dt) {
                        error!("update(): {e}");
                        elwt.exit();
                        return;
                    }
                }

                // render()
                if let Ok(render_fn) = lua.globals().get::<Function>("render") {
                    if let Err(e) = render_fn.call::<()>(()) {
                        error!("render(): {e}");
                        elwt.exit();
                        return;
                    }
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
        "space" => KeyCode::Space, "enter" => KeyCode::Enter,
        "escape" => KeyCode::Escape, "up" => KeyCode::ArrowUp,
        "down" => KeyCode::ArrowDown, "left" => KeyCode::ArrowLeft,
        "right" => KeyCode::ArrowRight, "w" => KeyCode::KeyW,
        "a" => KeyCode::KeyA, "s" => KeyCode::KeyS,
        "d" => KeyCode::KeyD, _ => KeyCode::Space,
    }
}
