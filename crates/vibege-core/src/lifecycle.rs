use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::config::{load_config, MergedConfig};
use crate::error::Result;
use crate::logging;

/// Describes the current state of the runtime application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Runtime is starting up and initialising subsystems.
    Initialising,
    /// Runtime is executing the main game loop.
    Running,
    /// Runtime is suspending game state.
    Suspending,
    /// Runtime state has been suspended.
    Suspended,
    /// Runtime is shutting down.
    ShuttingDown,
    /// Runtime has exited.
    Exited,
}

/// Signals that the application can respond to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Graceful shutdown request (SIGTERM, CTRL+C).
    Shutdown,
    /// Suspend request (SIGTSTP, custom trigger).
    Suspend,
    /// Resume request (SIGCONT, custom trigger).
    Resume,
}

/// Callback invoked during each phase of the application lifecycle.
pub trait LifecycleHandler: Send {
    /// Called once during initialisation, after config is loaded.
    fn on_init(&mut self, config: &MergedConfig) -> Result<()>;

    /// Called once per frame during the update phase.
    fn on_update(&mut self, dt: f64) -> Result<()>;

    /// Called once per frame during the render phase.
    fn on_render(&mut self, alpha: f64) -> Result<()>;

    /// Called when a suspend signal is received.
    fn on_suspend(&mut self) -> Result<()>;

    /// Called when a resume signal is received.
    fn on_resume(&mut self) -> Result<()>;

    /// Called once during shutdown, after the game loop ends.
    fn on_shutdown(&mut self) -> Result<()>;
}

/// The core runtime application.
///
/// Manages the application lifecycle: configuration loading, subsystem initialisation,
/// the main loop, signal handling, and graceful shutdown.
pub struct App {
    /// Current application state.
    state: AppState,

    /// Merged runtime configuration.
    config: MergedConfig,

    /// Timestamp of when the application started.
    started_at: Instant,

    /// Flag set to true when a shutdown signal is received.
    shutdown_requested: Arc<AtomicBool>,

    /// Flag set to true when a suspend signal is received.
    suspend_requested: Arc<AtomicBool>,
}

impl App {
    /// Creates a new runtime application from the default configuration sources.
    ///
    /// This loads and merges configuration from CLI args, environment variables,
    /// config files, and defaults.
    pub fn new() -> Result<Self> {
        let config = load_config()?;
        Ok(Self {
            state: AppState::Initialising,
            config,
            started_at: Instant::now(),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            suspend_requested: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Returns a reference to the merged runtime configuration.
    pub fn config(&self) -> &MergedConfig {
        &self.config
    }

    /// Returns the current application state.
    pub fn state(&self) -> AppState {
        self.state
    }

    /// Returns the duration since the application started.
    pub fn uptime(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Runs the application with the given lifecycle handler.
    ///
    /// This method:
    /// 1. Initialises logging
    /// 2. Calls `handler.on_init()`
    /// 3. Installs signal handlers
    /// 4. Enters the main loop (update/render cycle)
    /// 5. Calls `handler.on_shutdown()` on exit
    ///
    /// Returns an error if initialisation fails. The main loop exits when
    /// a shutdown signal is received or the handler returns an error.
    pub fn run(&mut self, handler: &mut dyn LifecycleHandler) -> Result<()> {
        let span = tracing::info_span!("app_run", version = env!("CARGO_PKG_VERSION"));
        let _guard = span.enter();

        // Phase 1: Initialise logging
        logging::init_logging(self.config.config.log_level);
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            log_level = %self.config.config.log_level.as_str(),
            dev_mode = self.config.config.dev_mode,
            "Runtime initialising"
        );

        // Phase 2: Install signal handlers
        self.install_signal_handlers()?;

        // Phase 3: Call handler initialisation
        tracing::info!("Calling handler on_init");
        handler.on_init(&self.config)?;

        self.state = AppState::Running;
        tracing::info!("Runtime entered running state");

        // Phase 4: Main loop
        let mut last_frame = Instant::now();
        let mut frame_count: u64 = 0;
        let mut fps_timer = Instant::now();

        loop {
            // Check for signals
            if self.shutdown_requested.load(Ordering::SeqCst) {
                tracing::info!("Shutdown signal received");
                self.state = AppState::ShuttingDown;
                break;
            }

            if self.suspend_requested.load(Ordering::SeqCst) {
                tracing::info!("Suspend signal received");
                self.state = AppState::Suspending;
                handler.on_suspend()?;
                self.state = AppState::Suspended;
                self.suspend_requested.store(false, Ordering::SeqCst);
                tracing::info!("Runtime suspended");
                // Wait for resume signal
                while !self.shutdown_requested.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    if !self.suspend_requested.load(Ordering::SeqCst) {
                        self.state = AppState::Running;
                        handler.on_resume()?;
                        tracing::info!("Runtime resumed");
                        break;
                    }
                }
            }

            // Calculate delta time
            let now = Instant::now();
            let dt = now.duration_since(last_frame).as_secs_f64();
            last_frame = now;

            // Update
            handler.on_update(dt)?;

            // Render
            handler.on_render(dt)?;

            frame_count += 1;

            // FPS limiting
            let fps_limit = self.config.config.fps_limit;
            if fps_limit > 0 {
                let frame_time = 1.0 / fps_limit as f64;
                let elapsed = now.elapsed().as_secs_f64();
                if elapsed < frame_time {
                    std::thread::sleep(std::time::Duration::from_secs_f64(frame_time - elapsed));
                }
            }

            // Log FPS every second
            if fps_timer.elapsed().as_secs_f64() >= 1.0 {
                let fps = frame_count as f64 / fps_timer.elapsed().as_secs_f64();
                tracing::debug!(fps = fps, "Frame rate");
                frame_count = 0;
                fps_timer = Instant::now();
            }
        }

        // Phase 5: Shutdown
        self.shutdown(handler)?;

        Ok(())
    }

    /// Performs a graceful shutdown of the application.
    fn shutdown(&mut self, handler: &mut dyn LifecycleHandler) -> Result<()> {
        tracing::info!("Runtime shutting down");

        let result = handler.on_shutdown();

        match &result {
            Ok(()) => {
                tracing::info!("Handler shutdown completed successfully");
            }
            Err(e) => {
                tracing::error!(error = %e, "Handler shutdown returned error");
            }
        }

        logging::flush_logs();
        self.state = AppState::Exited;
        tracing::info!(uptime_secs = self.uptime().as_secs_f64(), "Runtime exited");

        result
    }

    /// Installs OS signal handlers for graceful shutdown and suspend/resume.
    fn install_signal_handlers(&self) -> Result<()> {
        let shutdown_flag = Arc::clone(&self.shutdown_requested);
        let suspend_flag = Arc::clone(&self.suspend_requested);

        #[cfg(unix)]
        {
            use signal_hook::consts::signal::*;
            use signal_hook::flag;

            flag::register(SIGTERM, Arc::clone(&shutdown_flag))
                .map_err(|e| RuntimeError::with_cause(
                    ErrorCode::SIGNAL_HANDLER_ERROR,
                    "Failed to register SIGTERM handler",
                    e,
                ))?;

            flag::register(SIGINT, Arc::clone(&shutdown_flag))
                .map_err(|e| RuntimeError::with_cause(
                    ErrorCode::SIGNAL_HANDLER_ERROR,
                    "Failed to register SIGINT handler",
                    e,
                ))?;

            flag::register(SIGTSTP, Arc::clone(&suspend_flag))
                .map_err(|e| RuntimeError::with_cause(
                    ErrorCode::SIGNAL_HANDLER_ERROR,
                    "Failed to register SIGTSTP handler",
                    e,
                ))?;
        }

        #[cfg(windows)]
        {
            // Windows uses SetConsoleCtrlHandler via a separate mechanism.
            // For v0.1, we use a simple polling approach.
            let _ = shutdown_flag;
            let _ = suspend_flag;
            tracing::warn!("Windows signal handling not yet implemented, using polling");
        }

        tracing::debug!("Signal handlers installed");
        Ok(())
    }

    /// Requests a graceful shutdown. Can be called from any thread.
    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
    }

    /// Requests a suspend. Can be called from any thread.
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
    fn test_app_state_transitions() {
        let mut app = App::new().unwrap();
        assert_eq!(app.state(), AppState::Initialising);
        app.state = AppState::Running;
        assert_eq!(app.state(), AppState::Running);
        app.state = AppState::ShuttingDown;
        assert_eq!(app.state(), AppState::ShuttingDown);
        app.state = AppState::Exited;
        assert_eq!(app.state(), AppState::Exited);
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

        let handle = std::thread::spawn(move || {
            app.run(&mut handler)
        });

        // Let it run briefly then request shutdown via the Arc flag
        std::thread::sleep(std::time::Duration::from_millis(50));
        shutdown.store(true, Ordering::SeqCst);

        // Wait for the thread to finish
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
}
