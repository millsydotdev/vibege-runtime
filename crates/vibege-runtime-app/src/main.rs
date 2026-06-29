use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use mlua::{Function, Lua};
use tracing::{error, info};
use vibege_core::{install_panic_hook, logging, LogLevel};
use vibege_input::InputManager;
use vibege_renderer::Renderer;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};


#[derive(Parser)]
#[command(name = "vibege-runtime", version)]
struct RuntimeCli {
    #[arg(short = 'p', long = "project", default_value = ".")]
    project_dir: String,
    #[arg(short = 'e', long = "entry", default_value = "src/main.lua")]
    entry: String,
    #[arg(long = "width", default_value = "1280")]
    width: u32,
    #[arg(long = "height", default_value = "720")]
    height: u32,
}

fn main() -> anyhow::Result<()> {
    install_panic_hook();
    let cli = RuntimeCli::parse();

    let game_path = PathBuf::from(&cli.project_dir).join(&cli.entry);
    if !game_path.exists() {
        anyhow::bail!("Game entry not found: {}", game_path.display());
    }
    let game_source = std::fs::read_to_string(&game_path)?;

    logging::init_logging(LogLevel::Info);
    info!(entry = %game_path.display(), "VibeGE Runtime starting");

    let event_loop = EventLoop::new()
        .map_err(|e| anyhow::anyhow!("Event loop: {e}"))?;
    let window = Arc::new(
        event_loop.create_window(
            winit::window::WindowAttributes::default()
                .with_title("VibeGE")
                .with_inner_size(LogicalSize::new(cli.width as f64, cli.height as f64)),
        )
        .map_err(|e| anyhow::anyhow!("Window: {e}"))?,
    );

    info!("Initialising GPU...");
    let renderer = Arc::new(pollster::block_on(Renderer::new(
        Arc::clone(&window), cli.width, cli.height,
    ))?);
    info!("Renderer ready");

    let input_mgr = InputManager::new();
    let input = Arc::new(std::sync::Mutex::new(input_mgr));

    let lua = Lua::new();

    // Build vibege API
    let vibege = lua.create_table().expect("create vibege table");

    // Input bindings
    let input_table = lua.create_table().expect("create input table");
    let inp = Arc::clone(&input);
    input_table
        .set("is_key_down", lua.create_function(move |_, key: String| {
            Ok(inp.lock().unwrap().is_key_down(key_name_to_code(&key)))
        }).expect("create is_key_down"))
        .expect("set is_key_down");
    let inp2 = Arc::clone(&input);
    input_table
        .set("is_key_pressed", lua.create_function(move |_, key: String| {
            Ok(inp2.lock().unwrap().is_key_pressed(key_name_to_code(&key)))
        }).expect("create is_key_pressed"))
        .expect("set is_key_pressed");
    let inp3 = Arc::clone(&input);
    input_table
        .set("mouse_position", lua.create_function(move |_, ()| {
            let p = inp3.lock().unwrap().mouse_position();
            Ok((p.0, p.1))
        }).expect("create mouse_position"))
        .expect("set mouse_position");
    vibege.set("input", input_table).expect("set input");

    // Render bindings
    let render_table = lua.create_table().expect("create render table");
    let ren = Arc::clone(&renderer);
    render_table
        .set("clear", lua.create_function(move |_, (r, g, b, a): (f32, f32, f32, f32)| {
            let _ = ren.clear(r, g, b, a);
            Ok(())
        }).expect("create clear"))
        .expect("set clear");
    vibege.set("render", render_table).expect("set render");

    // Time bindings
    let time_table = lua.create_table().expect("create time table");
    let start_time = std::time::Instant::now();
    time_table
        .set("delta_time", lua.create_function(move |_, ()| Ok(0.016)).expect("create delta_time"))
        .expect("set delta_time");
    let st2 = start_time.clone();
    time_table
        .set("elapsed", lua.create_function(move |_, ()| Ok(st2.elapsed().as_secs_f64())).expect("create elapsed"))
        .expect("set elapsed");
    vibege.set("time", time_table).expect("set time");

    lua.globals().set("vibege", vibege).expect("set vibege globals");

    // Load game
    info!("Loading game");
    if let Err(e) = lua.load(&game_source).exec() {
        anyhow::bail!("Lua error: {e}");
    }

    // Call init()
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
                        info!("Window closed");
                        elwt.exit();
                    }
                    WindowEvent::KeyboardInput { event: ke, .. } => {
                        if ke.physical_key == PhysicalKey::Code(KeyCode::Escape)
                            && ke.state == ElementState::Pressed
                        {
                            info!("Escape — exiting");
                            elwt.exit();
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                let now = std::time::Instant::now();
                let dt = now.duration_since(last_frame).as_secs_f64();
                last_frame = now;

                // Update delta_time per frame
                if let Ok(v) = lua.globals().get::<mlua::Table>("vibege") {
                    if let Ok(t) = v.get::<mlua::Table>("time") {
                        let stc = start_time;
                        let _ = t.set("delta_time", lua.create_function(move |_, ()| Ok(dt)).expect("dt"));
                        let _ = t.set("elapsed", lua.create_function(move |_, ()| Ok(stc.elapsed().as_secs_f64())).expect("elapsed"));
                    }
                }

                // game.update(dt)
                if let Ok(update_fn) = lua.globals().get::<Function>("update") {
                    if let Err(e) = update_fn.call::<()>(dt) {
                        error!("update(): {e}");
                        elwt.exit();
                        return;
                    }
                }

                // game.render()
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
    }).map_err(|e| anyhow::anyhow!("Event loop error: {e}"))?;

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
