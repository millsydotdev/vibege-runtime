use std::sync::{Arc, Mutex};

use vibege_core::{App, AppState, LifecycleHandler, MergedConfig, Result};

struct RecordingHandler {
    calls: Arc<Mutex<Vec<String>>>,
    fail_on: Option<&'static str>,
}

impl RecordingHandler {
    fn new(calls: Arc<Mutex<Vec<String>>>) -> Self {
        Self { calls, fail_on: None }
    }

    fn with_failure(calls: Arc<Mutex<Vec<String>>>, fail_on: &'static str) -> Self {
        Self { calls, fail_on: Some(fail_on) }
    }

    fn record(&self, name: &str) -> Result<()> {
        self.calls.lock().unwrap().push(name.to_string());
        if self.fail_on == Some(name) {
            Err(vibege_core::RuntimeError::new(
                vibege_core::ErrorCode::INTERNAL,
                format!("Forced failure in {name}"),
            ))
        } else {
            Ok(())
        }
    }
}

impl LifecycleHandler for RecordingHandler {
    fn on_init(&mut self, _config: &MergedConfig) -> Result<()> {
        self.record("on_init")
    }
    fn on_update(&mut self, _dt: f64) -> Result<()> { self.record("on_update") }
    fn on_render(&mut self, _alpha: f64) -> Result<()> { self.record("on_render") }
    fn on_suspend(&mut self) -> Result<()> { self.record("on_suspend") }
    fn on_resume(&mut self) -> Result<()> { self.record("on_resume") }
    fn on_shutdown(&mut self) -> Result<()> { self.record("on_shutdown") }
}

#[test]
fn test_app_new() {
    let app = App::new();
    assert!(app.is_ok());
    assert_eq!(app.unwrap().state(), AppState::Initialising);
}

#[test]
fn test_config_accessible() {
    let app = App::new().unwrap();
    assert_eq!(app.config().config.log_level, vibege_core::LogLevel::Info);
}

#[test]
fn test_uptime_increases() {
    let app = App::new().unwrap();
    let t1 = app.uptime();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let t2 = app.uptime();
    assert!(t2 > t1);
}

#[test]
fn test_shutdown_request_stops_run() {
    let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handler = RecordingHandler::new(Arc::clone(&calls));
    let mut app = App::new().unwrap();
    app.request_shutdown();
    let result = app.run(&mut handler);
    assert!(result.is_ok());
    let recorded = calls.lock().unwrap();
    assert!(recorded.contains(&"on_init".to_string()));
    assert!(recorded.contains(&"on_shutdown".to_string()));
}

#[test]
fn test_init_failure_propagates() {
    let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handler = RecordingHandler::with_failure(Arc::clone(&calls), "on_init");
    let mut app = App::new().unwrap();
    assert!(app.run(&mut handler).is_err());
}

#[test]
fn test_error_code_categories() {
    assert_eq!(vibege_core::ErrorCode::CONFIG_FILE_NOT_FOUND.category(), "configuration");
    assert_eq!(vibege_core::ErrorCode::INIT_FAILED.category(), "initialisation");
    assert_eq!(vibege_core::ErrorCode::PANIC.category(), "internal");
}

#[test]
fn test_error_display() {
    let err = vibege_core::RuntimeError::new(
        vibege_core::ErrorCode::CONFIG_FILE_NOT_FOUND,
        "Config file missing",
    );
    let display = format!("{err}");
    assert!(display.contains("1001"));
    assert!(display.contains("Config file missing"));
}

#[test]
fn test_error_at_location() {
    let err = vibege_core::RuntimeError::new(
        vibege_core::ErrorCode::INIT_FAILED,
        "Window creation failed",
    ).at("src/window.rs:42");
    assert!(format!("{err}").contains("src/window.rs:42"));
}
