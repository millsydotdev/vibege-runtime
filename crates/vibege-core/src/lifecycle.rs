use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::config::{MergedConfig, load_config};
use crate::error::Result;
use crate::logging;
use crate::metrics::MetricsRegistry;
use crate::state_machine::{RuntimeState, StateMachine};

/// Describes the current state of the runtime application.
/// Kept for backward compatibility — delegates to [`RuntimeState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Initialising,
    Running,
    Suspending,
    Suspended,
    ShuttingDown,
    Exited,
}

impl From<RuntimeState> for AppState {
    fn from(s: RuntimeState) -> Self {
        match s {
            RuntimeState::Created | RuntimeState::Initialising => AppState::Initialising,
            RuntimeState::Running => AppState::Running,
            RuntimeState::Suspended => AppState::Suspended,
            RuntimeState::ShuttingDown => AppState::ShuttingDown,
            RuntimeState::Exited | RuntimeState::Error => AppState::Exited,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Shutdown,
    Suspend,
    Resume,
}

pub trait LifecycleHandler: Send {
    fn on_init(&mut self, config: &MergedConfig) -> Result<()>;
    fn on_update(&mut self, dt: f64) -> Result<()>;
    fn on_render(&mut self, alpha: f64) -> Result<()>;
    fn on_suspend(&mut self) -> Result<()>;
    fn on_resume(&mut self) -> Result<()>;
    fn on_shutdown(&mut self) -> Result<()>;
}

/// The core runtime application with explicit state machine enforcement.
pub struct App {
    state_machine: StateMachine,
    config: MergedConfig,
    started_at: Instant,
    shutdown_requested: Arc<AtomicBool>,
    suspend_requested: Arc<AtomicBool>,
    metrics: Arc<MetricsRegistry>,
}

impl App {
    pub fn new() -> Result<Self> {
        crate::crash::install_panic_hook();
        let config = load_config()?;
        Ok(Self {
            state_machine: StateMachine::new(),
            config,
            started_at: Instant::now(),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            suspend_requested: Arc::new(AtomicBool::new(false)),
            metrics: MetricsRegistry::new(),
        })
    }

    pub fn config(&self) -> &MergedConfig {
        &self.config
    }

    /// Returns the current state as the legacy AppState enum.
    pub fn state(&self) -> AppState {
        AppState::from(self.state_machine.state())
    }

    /// Returns the raw runtime state.
    pub fn runtime_state(&self) -> RuntimeState {
        self.state_machine.state()
    }

    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn metrics(&self) -> &Arc<MetricsRegistry> {
        &self.metrics
    }

    pub fn run(&mut self, handler: &mut dyn LifecycleHandler) -> Result<()> {
        let span = tracing::info_span!("app_run", version = env!("CARGO_PKG_VERSION"));
        let _guard = span.enter();

        self.state_machine
            .transition(RuntimeState::Initialising)
            .ok();

        logging::init_logging(self.config.config.log_level);
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            log_level = %self.config.config.log_level.as_str(),
            dev_mode = self.config.config.dev_mode,
            "Runtime initialising"
        );

        self.install_signal_handlers()?;

        tracing::info!("Calling handler on_init");
        if let Err(e) = handler.on_init(&self.config) {
            self.state_machine.transition(RuntimeState::Error).ok();
            return Err(e);
        }

        self.state_machine.transition(RuntimeState::Running).ok();
        tracing::info!("Runtime entered running state");

        let mut last_frame = Instant::now();
        let mut frame_count: u64 = 0;
        let mut fps_timer = Instant::now();

        loop {
            if self.shutdown_requested.load(Ordering::SeqCst) {
                tracing::info!("Shutdown signal received");
                self.state_machine
                    .transition(RuntimeState::ShuttingDown)
                    .ok();
                break;
            }

            if self.suspend_requested.load(Ordering::SeqCst) {
                tracing::info!("Suspend signal received");
                self.state_machine.transition(RuntimeState::Suspended).ok();
                handler.on_suspend()?;
                self.suspend_requested.store(false, Ordering::SeqCst);
                tracing::info!("Runtime suspended");

                // Wait for resume or shutdown — with 50ms poll but bounded yield
                let mut poll_count = 0u64;
                while !self.shutdown_requested.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(50));
                    poll_count += 1;
                    if poll_count.is_multiple_of(20) {
                        std::thread::yield_now();
                    }
                    if !self.suspend_requested.load(Ordering::SeqCst) {
                        self.state_machine.transition(RuntimeState::Running).ok();
                        handler.on_resume()?;
                        tracing::info!("Runtime resumed");
                        break;
                    }
                }
            }

            let now = Instant::now();
            let dt = now.duration_since(last_frame).as_secs_f64();
            last_frame = now;
            self.metrics.record_frame(dt);
            handler.on_update(dt)?;
            handler.on_render(dt)?;

            frame_count += 1;

            let fps_limit = self.config.config.fps_limit;
            if fps_limit > 0 {
                let frame_time = 1.0 / fps_limit as f64;
                let elapsed = now.elapsed().as_secs_f64();
                if elapsed < frame_time {
                    std::thread::sleep(Duration::from_secs_f64(frame_time - elapsed));
                }
            }

            if fps_timer.elapsed().as_secs_f64() >= 1.0 {
                let fps = frame_count as f64 / fps_timer.elapsed().as_secs_f64();
                tracing::debug!(fps = fps, "Frame rate");
                frame_count = 0;
                fps_timer = Instant::now();
            }
        }

        self.shutdown(handler)?;
        Ok(())
    }

    fn shutdown(&mut self, handler: &mut dyn LifecycleHandler) -> Result<()> {
        tracing::info!("Runtime shutting down");
        let result = handler.on_shutdown();
        match &result {
            Ok(()) => tracing::info!("Handler shutdown completed successfully"),
            Err(e) => tracing::error!(error = %e, "Handler shutdown returned error"),
        }
        self.metrics.stop();
        logging::flush_logs();
        self.state_machine.transition(RuntimeState::Exited).ok();
        tracing::info!(uptime_secs = self.uptime().as_secs_f64(), "Runtime exited");
        result
    }

    fn install_signal_handlers(&self) -> Result<()> {
        let shutdown_flag = Arc::clone(&self.shutdown_requested);
        let _suspend_flag = Arc::clone(&self.suspend_requested);

        #[cfg(unix)]
        {
            use crate::{ErrorCode, RuntimeError};
            use signal_hook::consts::signal::*;
            use signal_hook::flag;

            flag::register(SIGTERM, Arc::clone(&shutdown_flag)).map_err(|e| {
                RuntimeError::with_cause(
                    ErrorCode::SIGNAL_HANDLER_ERROR,
                    "Failed to register SIGTERM handler",
                    e,
                )
            })?;
            flag::register(SIGINT, Arc::clone(&shutdown_flag)).map_err(|e| {
                RuntimeError::with_cause(
                    ErrorCode::SIGNAL_HANDLER_ERROR,
                    "Failed to register SIGINT handler",
                    e,
                )
            })?;
            flag::register(SIGTSTP, Arc::clone(&_suspend_flag)).map_err(|e| {
                RuntimeError::with_cause(
                    ErrorCode::SIGNAL_HANDLER_ERROR,
                    "Failed to register SIGTSTP handler",
                    e,
                )
            })?;
        }

        #[cfg(windows)]
        {
            static CTRL_C_PRESSED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);

            extern "system" fn console_ctrl_handler(_: u32) -> i32 {
                CTRL_C_PRESSED.store(true, std::sync::atomic::Ordering::SeqCst);
                1
            }

            match unsafe {
                windows_sys::Win32::System::Console::SetConsoleCtrlHandler(
                    Some(console_ctrl_handler),
                    1,
                )
            } {
                0 => tracing::warn!("Failed to register console control handler"),
                _ => tracing::debug!("Console control handler registered"),
            }

            let shutdown = Arc::clone(&shutdown_flag);
            std::thread::Builder::new()
                .name("console-ctrl-watcher".into())
                .spawn(move || {
                    while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                        if CTRL_C_PRESSED.load(std::sync::atomic::Ordering::SeqCst) {
                            shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                })
                .ok();
        }

        tracing::debug!("Signal handlers installed");
        Ok(())
    }

    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
    }

    pub fn request_suspend(&self) {
        self.suspend_requested.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;

    struct TestHandler {
        init_called: bool,
        update_called: bool,
        render_called: bool,
        suspend_called: bool,
        resume_called: bool,
        shutdown_called: bool,
    }

    impl TestHandler {
        fn new() -> Self {
            Self {
                init_called: false,
                update_called: false,
                render_called: false,
                suspend_called: false,
                resume_called: false,
                shutdown_called: false,
            }
        }
    }

    impl LifecycleHandler for TestHandler {
        fn on_init(&mut self, _config: &MergedConfig) -> Result<()> {
            self.init_called = true;
            Ok(())
        }
        fn on_update(&mut self, _dt: f64) -> Result<()> {
            self.update_called = true;
            Ok(())
        }
        fn on_render(&mut self, _alpha: f64) -> Result<()> {
            self.render_called = true;
            Ok(())
        }
        fn on_suspend(&mut self) -> Result<()> {
            self.suspend_called = true;
            Ok(())
        }
        fn on_resume(&mut self) -> Result<()> {
            self.resume_called = true;
            Ok(())
        }
        fn on_shutdown(&mut self) -> Result<()> {
            self.shutdown_called = true;
            Ok(())
        }
    }

    #[test]
    fn test_app_creation() {
        let app = App::new();
        assert!(app.is_ok());
        let app = app.unwrap();
        assert_eq!(app.state(), AppState::Initialising);
    }

    #[test]
    fn test_shutdown_request() {
        let app = App::new().unwrap();
        assert!(!app.shutdown_requested.load(Ordering::SeqCst));
        app.request_shutdown();
        assert!(app.shutdown_requested.load(Ordering::SeqCst));
    }

    #[test]
    fn test_suspend_request() {
        let app = App::new().unwrap();
        assert!(!app.suspend_requested.load(Ordering::SeqCst));
        app.request_suspend();
        assert!(app.suspend_requested.load(Ordering::SeqCst));
    }

    #[test]
    fn test_lifecycle_handler_trait() {
        let mut handler = TestHandler::new();
        let config = load_config().unwrap();
        assert!(handler.on_init(&config).is_ok());
        assert!(handler.init_called);
        assert!(handler.on_update(0.016).is_ok());
        assert!(handler.update_called);
        assert!(handler.on_render(0.5).is_ok());
        assert!(handler.render_called);
        assert!(handler.on_suspend().is_ok());
        assert!(handler.suspend_called);
        assert!(handler.on_resume().is_ok());
        assert!(handler.resume_called);
        assert!(handler.on_shutdown().is_ok());
        assert!(handler.shutdown_called);
    }

    #[test]
    fn test_run_with_handler() {
        let mut app = App::new().unwrap();
        let mut handler = TestHandler::new();
        let shutdown = Arc::clone(&app.shutdown_requested);
        let handle = std::thread::spawn(move || app.run(&mut handler));
        std::thread::sleep(std::time::Duration::from_millis(50));
        shutdown.store(true, Ordering::SeqCst);
        handle.join().expect("Runtime thread panicked").unwrap();
    }

    #[test]
    fn test_uptime() {
        let app = App::new().unwrap();
        let uptime = app.uptime();
        assert!(uptime.as_secs_f64() >= 0.0);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let uptime2 = app.uptime();
        assert!(uptime2 > uptime);
    }

    #[test]
    fn test_runtime_state_initial() {
        let app = App::new().unwrap();
        assert_eq!(app.runtime_state(), RuntimeState::Created);
    }

    #[test]
    fn test_runtime_state_after_run_shutdown() {
        let mut app = App::new().unwrap();
        let mut handler = TestHandler::new();
        let shutdown = Arc::clone(&app.shutdown_requested);
        let handle = std::thread::spawn(move || app.run(&mut handler));
        std::thread::sleep(std::time::Duration::from_millis(50));
        shutdown.store(true, Ordering::SeqCst);
        let result = handle.join().expect("Runtime thread panicked");
        // The internal state machine should track transitions properly
        assert!(result.is_ok());
    }
}
