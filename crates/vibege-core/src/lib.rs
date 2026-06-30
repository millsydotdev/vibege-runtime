//! # VibeGE Core
//!
//! The core runtime library for the VibeGE game engine.
//!
//! This crate provides the foundational runtime services:
//! - **Application lifecycle** — startup, main loop, shutdown
//! - **Configuration** — loading from CLI args, environment, and config files
//! - **Error handling** — typed errors with machine-readable codes
//! - **Logging** — structured JSON logging via `tracing`
//! - **Metrics** — counters, gauges, FPS tracking
//! - **Crash reporting** — panic hook, crash dump generation
//!
//! ## Architecture
//!
//! The runtime is structured as a set of independent subsystems that
//! communicate through well-defined interfaces. The `App` struct is the
//! top-level entry point that orchestrates the lifecycle.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vibege_core::{App, LifecycleHandler, MergedConfig, Result};
//!
//! struct MyGame;
//!
//! impl LifecycleHandler for MyGame {
//!     fn on_init(&mut self, config: &MergedConfig) -> Result<()> { Ok(()) }
//!     fn on_update(&mut self, dt: f64) -> Result<()> { Ok(()) }
//!     fn on_render(&mut self, alpha: f64) -> Result<()> { Ok(()) }
//!     fn on_suspend(&mut self) -> Result<()> { Ok(()) }
//!     fn on_resume(&mut self) -> Result<()> { Ok(()) }
//!     fn on_shutdown(&mut self) -> Result<()> { Ok(()) }
//! }
//!
//! fn main() -> Result<()> {
//!     let mut app = App::new()?;
//!     let mut game = MyGame;
//!     app.run(&mut game)
//! }
//! ```

pub mod config;
pub mod crash;
pub mod error;
pub mod event;
pub mod lifecycle;
pub mod logging;
pub mod metrics;

pub use config::{LogLevel, MergedConfig, RuntimeConfig, WindowConfig, load_config};
pub use crash::install_panic_hook;
pub use error::{ErrorCode, Result, RuntimeError};
pub use event::{EventBus, RuntimeEvent};
pub use lifecycle::{App, AppState, LifecycleHandler, Signal};
pub use metrics::{MetricsRegistry, MetricsSnapshot};
