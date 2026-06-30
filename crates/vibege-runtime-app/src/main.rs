use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use clap::Parser;
use tracing::{error, info, warn};
use vibege_audio::AudioSystem;
use vibege_core::{LogLevel, install_panic_hook, logging};
use vibege_input::InputManager;
use vibege_renderer::Renderer;
use vibege_scene::scene::{SceneContext, SceneManager};
use vibege_scene::scenes::boot_scene::BootScene;
use vibege_suspension::{SuspensionConfig, SuspensionEngine};
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

    let has_game = !cli.entry.is_empty() && !cli.project_dir.is_empty();

    logging::init_logging(LogLevel::Info);

    // ── Configuration Manager ──
    let cfg = Arc::new(vibege_config::ConfigHandle::new());
    let player_config = cfg.get();
    info!(first_run = cfg.is_first_run(), "Player config loaded");

    let start_visible = cli.overlay
        || cli.show
        || has_game
        || !cli.start_hidden
        || player_config.general.startup_behavior == "shown";
    if start_visible {
        info!("Window visible on startup");
    } else {
        info!("Runtime started in background");
    }
    if cfg.is_first_run() {
        info!("First run detected");
    }

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

    if cli.overlay {
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
            };
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(w32) = handle.as_ref() {
                    let hwnd = w32.hwnd.get() as HWND;
                    unsafe {
                        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE);
                    }
                }
            }
        }
        info!("Overlay mode enabled");
    }

    // ── System Tray ──
    let _tray_handle = vibege_tray::start();
    if _tray_handle.is_some() {
        info!("System tray active");
    }

    // ── GPU Renderer ──
    let (w, h) = {
        let s = window.inner_size();
        (s.width, s.height)
    };
    info!("Initialising GPU...");
    let renderer = Arc::new(pollster::block_on(Renderer::new(
        Arc::clone(&window),
        w,
        h,
    ))?);
    info!("Renderer ready");

    // ── Audio System ──
    let audio = AudioSystem::new().map(Arc::new);
    if audio.is_some() {
        info!("Audio system ready");
    }

    // ── Input System ──
    let input = Arc::new(Mutex::new(InputManager::new()));

    // ── Suspension Engine ──
    let snap_dir = PathBuf::from(".").join(".vibege").join("snapshots");
    std::fs::create_dir_all(&snap_dir).ok();
    let mut _suspension = SuspensionEngine::with_config(SuspensionConfig {
        snapshot_dir: snap_dir,
        enable_compression: false,
        ..Default::default()
    })?;

    // ── Runtime Event Bus ──
    let event_bus = Arc::new(vibege_core::EventBus::new());
    let eb_log = Arc::clone(&event_bus);
    eb_log.subscribe(move |ev| info!("Event: {ev:?}"));

    // ── Scene Manager ──
    let mut scene_ctx = SceneContext::new(
        w,
        h,
        Arc::clone(&renderer),
        Arc::clone(&input),
        Arc::clone(&cfg),
        Some(Arc::clone(&event_bus)),
    );
    let mut scene_manager = SceneManager::new();
    scene_manager
        .push(Box::new(BootScene::new()), &mut scene_ctx)
        .map_err(|e| anyhow::anyhow!("Scene push: {e}"))?;

    // ── Main Loop ──
    info!("Entering main loop");
    let mut last_frame = std::time::Instant::now();

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::WindowEvent { event: we, .. } => {
                    input.lock().unwrap().handle_window_event(&we);
                    if matches!(we, winit::event::WindowEvent::CloseRequested) {
                        event_bus.publish(&vibege_core::RuntimeEvent::ShuttingDown);
                        info!("Window closed");
                        elwt.exit();
                    }
                }
                Event::AboutToWait => {
                    // Hotkey polling
                    #[cfg(target_os = "windows")]
                    {
                        let k_mod = cfg.get().overlay.hotkey_modifiers;
                        let k_key = cfg.get().overlay.hotkey_key;
                        let (mc, ms, ma) = (
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
                                "v" => 0x56,
                                "g" => 0x47,
                                "b" => 0x42,
                                "h" => 0x48,
                                "space" => 0x20,
                                "tab" => 0x09,
                                _ => 0x56,
                            };
                            let key = GetAsyncKeyState(vk);
                            if (!mc || ctrl < 0)
                                && (!ms || shift < 0)
                                && (!ma || alt < 0)
                                && key < 0
                            {
                                vibege_tray::request_toggle();
                            }
                        }
                    }

                    // Tray signals
                    if vibege_tray::should_show_launcher() {
                        window.set_visible(true);
                        event_bus.publish(&vibege_core::RuntimeEvent::OverlayShown);
                    }
                    if vibege_tray::should_toggle_overlay() {
                        let visible = window.is_visible().unwrap_or(true);
                        window.set_visible(!visible);
                        event_bus.publish(if visible {
                            &vibege_core::RuntimeEvent::OverlayHidden
                        } else {
                            &vibege_core::RuntimeEvent::OverlayShown
                        });
                    }
                    if vibege_tray::should_quit() {
                        event_bus.publish(&vibege_core::RuntimeEvent::ShuttingDown);
                        info!("Quit requested");
                        scene_manager.shutdown(&mut scene_ctx);
                        elwt.exit();
                        return;
                    }

                    // Frame timing
                    let now = std::time::Instant::now();
                    let dt = now.duration_since(last_frame).as_secs_f64();
                    last_frame = now;

                    // Update and render scene
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

                    // Present
                    if let Err(e) = renderer.render() {
                        error!("GPU: {e}");
                    }

                    input.lock().unwrap().end_frame();
                    window.request_redraw();

                    if scene_manager.is_empty() {
                        elwt.exit();
                    }
                }
                _ => {}
            }
        })
        .map_err(|e| anyhow::anyhow!("Event loop: {e}"))?;

    info!("Runtime exited");
    Ok(())
}
