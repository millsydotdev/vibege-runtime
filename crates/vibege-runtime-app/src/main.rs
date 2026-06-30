use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use clap::Parser;
use tracing::{error, info, warn};
use vibege_asset::AssetManager;
use vibege_audio::AudioSystem;
use vibege_core::{
    Diagnostics, EventBus, HealthStatus, LogLevel, RuntimeEvent, ServiceRegistry,
    SubscriberPriority, install_panic_hook, logging,
};
use vibege_input::InputManager;
use vibege_renderer::Renderer;
use vibege_scene::scene::{SceneContext, SceneManager};
use vibege_scene::scenes::boot_scene::BootScene;
use vibege_suspension::{SuspensionConfig, SuspensionEngine};
use vibege_window::display::DisplayManager;
use vibege_window::overlay::{OverlayManager, OverlayPersistentState, apply_overlay_attributes};
use winit::dpi::LogicalSize;
use winit::event::Event;
use winit::event_loop::EventLoop;

#[derive(Parser)]
#[command(
    name = "vibege-runtime",
    version,
    about = "VibeGE Game Runtime — AI-friendly overlay"
)]
struct RuntimeCli {
    #[arg(short = 'p', long = "project", default_value = "")]
    project_dir: String,
    #[arg(short = 'e', long = "entry", default_value = "")]
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
    let has_game = !cli.entry.is_empty() && !cli.project_dir.is_empty();
    logging::init_logging(LogLevel::Info);

    // ── Diagnostics ──
    let diagnostics = Arc::new(Diagnostics::new());

    // ── Configuration Manager ──
    let cfg = Arc::new(vibege_config::ConfigHandle::new());
    let _player_config = cfg.get();
    info!(first_run = cfg.is_first_run(), "Player config loaded");

    let start_visible = cli.overlay
        || cli.show
        || has_game
        || !cli.start_hidden
        || cfg.get().general.startup_behavior == "shown";

    // ── Service Registry ──
    let mut services = ServiceRegistry::new();
    diagnostics.register_simple("config", true, "loaded from file".into());

    // ── Event Loop & Window ──
    let event_loop = EventLoop::new().map_err(|e| anyhow::anyhow!("Event loop: {e}"))?;
    let window = Arc::new(
        event_loop
            .create_window(
                winit::window::WindowAttributes::default()
                    .with_title("VibeGE")
                    .with_inner_size(LogicalSize::new(cli.width as f64, cli.height as f64))
                    .with_decorations(!cli.overlay)
                    .with_visible(start_visible),
            )
            .map_err(|e| anyhow::anyhow!("Window: {e}"))?,
    );

    // ── Overlay & Display Managers ──
    let mut overlay_manager = if let Some(state) = load_overlay_state(&cfg) {
        OverlayManager::from_persistent(state)
    } else {
        OverlayManager::new()
    };
    let display_manager = DisplayManager::new(&window);
    overlay_manager.centre_on(&display_manager, None);
    if cli.overlay {
        apply_overlay_attributes(&window, overlay_manager.mode());
        info!("Overlay mode enabled");
    }
    diagnostics.register_simple("window", true, format!("{}x{}", cli.width, cli.height));

    // ── Event Bus ──
    let event_bus = Arc::new(EventBus::new());
    event_bus.subscribe_with_priority(SubscriberPriority::Monitor, move |ev| {
        info!("Event: {ev:?}");
    });
    event_bus.publish(&RuntimeEvent::WindowCreated);

    // ── Periodic diagnostics publishing ──
    let diag_bus = Arc::clone(&event_bus);
    let diag_thread = Arc::clone(&diagnostics);
    std::thread::Builder::new()
        .name("diagnostics".into())
        .spawn(move || {
            let mut last_publish = Instant::now();
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let elapsed = last_publish.elapsed();
                if elapsed >= Duration::from_secs(5) {
                    let health = diag_thread.report();
                    if !matches!(health.overall, HealthStatus::Healthy) {
                        diag_bus.publish(&RuntimeEvent::DiagnosticsReported);
                    }
                    last_publish = Instant::now();
                }
            }
        })
        .ok();

    // ── System Tray ──
    let _tray_handle = vibege_tray::start();
    if _tray_handle.is_some() {
        info!("System tray active");
        vibege_tray::set_overlay_label(overlay_manager.is_visible());
    }
    diagnostics.register_simple("tray", _tray_handle.is_some(), "system tray".into());

    // ── GPU Renderer ──
    let (w, h) = {
        let s = window.inner_size();
        (s.width, s.height)
    };
    info!("Initialising GPU...");
    let renderer = match pollster::block_on(Renderer::new(Arc::clone(&window), w, h)) {
        Ok(r) => {
            info!("Renderer ready");
            diagnostics.register_simple("renderer", true, format!("{}x{}", w, h));
            Arc::new(r)
        }
        Err(e) => {
            error!("GPU initialisation failed: {e}");
            diagnostics.register_simple("renderer", false, format!("init failed: {e}"));
            vibege_tray::show_notification(
                "GPU Error",
                "Renderer failed to initialise. Check your GPU drivers.",
            );
            return Err(e.into());
        }
    };

    // ── Asset Manager ──
    let asset_manager = Arc::new(AssetManager::new());
    asset_manager.set_texture_loader(renderer.create_asset_texture_loader());
    info!("Asset manager ready");
    diagnostics.register_simple("assets", true, "texture loader connected".into());

    // ── Audio System ──
    let audio = AudioSystem::new().map(Arc::new);
    diagnostics.register_simple(
        "audio",
        audio.is_some(),
        if audio.is_some() {
            "ready".into()
        } else {
            "device unavailable".into()
        },
    );

    // ── Input System ──
    let input = Arc::new(Mutex::new(InputManager::new()));
    diagnostics.register_simple("input", true, "ready".into());

    // ── Suspension Engine ──
    let snap_dir = PathBuf::from(".").join(".vibege").join("snapshots");
    std::fs::create_dir_all(&snap_dir).ok();
    let suspension: Option<Arc<Mutex<SuspensionEngine>>> =
        match SuspensionEngine::with_config(SuspensionConfig {
            snapshot_dir: snap_dir,
            enable_compression: false,
            ..Default::default()
        }) {
            Ok(s) => {
                diagnostics.register_simple("suspension", true, "ready".into());
                Some(Arc::new(Mutex::new(s)))
            }
            Err(e) => {
                warn!("Suspension engine failed: {e}");
                diagnostics.register_simple("suspension", false, format!("init failed: {e}"));
                None
            }
        };

    // ── Scene Manager ──
    let mut scene_ctx = SceneContext::new(
        w,
        h,
        Arc::clone(&renderer),
        Arc::clone(&input),
        Arc::clone(&cfg),
        Some(Arc::clone(&event_bus)),
        audio.clone(),
        Arc::clone(&asset_manager),
        suspension.clone(),
    );
    let mut scene_manager = SceneManager::new();
    scene_manager.push(Box::new(BootScene::new()), &mut scene_ctx);
    diagnostics.register_simple("scenes", true, "BootScene loaded".into());

    // ── Initialize Service Registry ──
    services.register("runtime", None, None);
    if let Err(e) = services.initialize(&diagnostics) {
        warn!("Service initialization failed: {e}");
    }

    // ── Main Loop ──
    info!("Entering main loop");
    let mut last_frame = Instant::now();

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { event: we, .. } => {
                input.lock().expect("Input lock").handle_window_event(&we);
                match &we {
                    winit::event::WindowEvent::CloseRequested => {
                        event_bus.publish(&RuntimeEvent::ShuttingDown);
                        info!("Window closed");
                        elwt.exit();
                    }
                    winit::event::WindowEvent::Moved(pos) => {
                        overlay_manager.set_position(pos.x, pos.y);
                        event_bus.publish(&RuntimeEvent::WindowMoved { x: pos.x, y: pos.y });
                    }
                    winit::event::WindowEvent::Focused(focused) => {
                        if *focused {
                            event_bus.publish(&RuntimeEvent::WindowRestored);
                        } else {
                            event_bus.publish(&RuntimeEvent::WindowMinimized);
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                poll_overlay_hotkey(&cfg, overlay_manager.is_visible());

                if vibege_tray::should_show_launcher() {
                    window.set_visible(true);
                    overlay_manager.show();
                    vibege_tray::set_overlay_label(true);
                    event_bus.publish(&RuntimeEvent::OverlayShown);
                }
                if vibege_tray::should_toggle_overlay() {
                    overlay_manager.toggle();
                    let visible = overlay_manager.is_visible();
                    window.set_visible(visible);
                    vibege_tray::set_overlay_label(visible);
                    event_bus.publish(if visible {
                        &RuntimeEvent::OverlayShown
                    } else {
                        &RuntimeEvent::OverlayHidden
                    });
                    save_overlay_state(&cfg, &overlay_manager);
                }
                if vibege_tray::should_restart() {
                    info!("Restart requested from tray");
                    if let Ok(exe_path) = std::env::current_exe() {
                        let args: Vec<String> = std::env::args().collect();
                        match std::process::Command::new(&exe_path)
                            .args(&args[1..])
                            .spawn()
                        {
                            Ok(_) => info!("Relaunch process spawned"),
                            Err(e) => warn!("Failed to spawn relaunch: {e}"),
                        }
                    }
                    elwt.exit();
                    return;
                }
                if vibege_tray::should_quit() {
                    event_bus.publish(&RuntimeEvent::ShuttingDown);
                    info!("Quit requested");
                    scene_manager.shutdown(&mut scene_ctx);
                    elwt.exit();
                    return;
                }

                let now = Instant::now();
                let dt = now.duration_since(last_frame).as_secs_f64();
                last_frame = now;

                let action = match scene_manager.update(&mut scene_ctx, dt) {
                    Ok(a) => a,
                    Err(e) => {
                        warn!("Scene update: {e}");
                        return;
                    }
                };
                if let Err(e) = scene_manager.apply(action, &mut scene_ctx) {
                    warn!("Navigation: {e}");
                }
                if let Err(e) = scene_manager.process_pending(&mut scene_ctx) {
                    warn!("Pending nav: {e}");
                }

                let action = match scene_manager.render(&mut scene_ctx) {
                    Ok(a) => a,
                    Err(e) => {
                        warn!("Scene render: {e}");
                        return;
                    }
                };
                if let Err(e) = scene_manager.apply(action, &mut scene_ctx) {
                    warn!("Navigation: {e}");
                }
                if let Err(e) = scene_manager.process_pending(&mut scene_ctx) {
                    warn!("Pending nav: {e}");
                }

                if let Err(e) = renderer.render() {
                    error!("GPU: {e}");
                }

                input.lock().expect("Input lock").end_frame();
                window.request_redraw();

                if scene_manager.is_empty() {
                    elwt.exit();
                }
            }
            _ => {}
        })
        .map_err(|e| anyhow::anyhow!("Event loop: {e}"))?;

    info!("Runtime exited");
    Ok(())
}

#[allow(unused_variables)]
fn poll_overlay_hotkey(cfg: &vibege_config::ConfigHandle, _overlay_visible: bool) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        let k_mod = cfg.get().overlay.hotkey_modifiers;
        let k_key = cfg.get().overlay.hotkey_key;
        let (mc, ms, ma) = (
            k_mod.contains("ctrl"),
            k_mod.contains("shift"),
            k_mod.contains("alt"),
        );
        unsafe {
            let ctrl_pressed = GetAsyncKeyState(0x11);
            let shift_pressed = GetAsyncKeyState(0x10);
            let alt_pressed = GetAsyncKeyState(0x12);
            let vk = match k_key.as_str() {
                "v" => 0x56,
                "g" => 0x47,
                "b" => 0x42,
                "h" => 0x48,
                "space" => 0x20,
                "tab" => 0x09,
                _ => 0x56,
            };
            let key_pressed = GetAsyncKeyState(vk);
            if (!mc || (ctrl_pressed as i16) < 0)
                && (!ms || (shift_pressed as i16) < 0)
                && (!ma || (alt_pressed as i16) < 0)
                && (key_pressed as i16) < 0
            {
                vibege_tray::request_toggle();
            }
        }
    }
}

fn load_overlay_state(cfg: &vibege_config::ConfigHandle) -> Option<OverlayPersistentState> {
    let config = cfg.get();
    if cfg.get().overlay.last_monitor.is_empty() {
        return None;
    }
    Some(OverlayPersistentState {
        x: config.overlay.last_x,
        y: config.overlay.last_y,
        width: config.overlay.width,
        height: config.overlay.height,
        monitor_name: config.overlay.last_monitor.clone(),
        was_visible: config.overlay.last_visible,
    })
}

fn save_overlay_state(cfg: &vibege_config::ConfigHandle, overlay: &OverlayManager) {
    let state = overlay.persistent_state();
    let mut config = cfg.get();
    config.overlay.last_x = state.x;
    config.overlay.last_y = state.y;
    config.overlay.width = state.width;
    config.overlay.height = state.height;
    config.overlay.last_monitor.clone_from(&state.monitor_name);
    config.overlay.last_visible = state.was_visible;
    cfg.set(config);
}
