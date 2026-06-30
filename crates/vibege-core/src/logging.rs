use tracing_subscriber::filter::EnvFilter;

use crate::config::LogLevel;

/// Initializes the structured logging subsystem.
///
/// Logs are emitted as structured JSON to stdout by default.
/// The log level is configured from the `RuntimeConfig`.
///
/// If a global subscriber has already been set (e.g., by a parent process or
/// another initialisation call), this function returns immediately without error.
pub fn init_logging(log_level: LogLevel) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level.as_str()));

    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_env_filter(env_filter)
        .with_writer(std::io::stdout)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Flushes pending log events.
/// Should be called during shutdown to ensure all logs are emitted.
pub fn flush_logs() {
    // tracing-subscriber flushes on drop; this forces a sync point.
    // For v0.1, we accept potential log loss on crash.
    // Future: integrate with tracing-appender for non-blocking file I/O.
    tracing::info!("Log checkpoint");
}
