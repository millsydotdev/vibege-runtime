use std::fmt;
use thiserror::Error;

/// Machine-readable error codes for the VibeGE Runtime.
///
/// Error codes follow the category ranges defined in the Runtime Lifecycle API Spec:
/// - Configuration: 1000–1999
/// - Initialisation: 2000–2999
/// - Runtime: 3000–3999
/// - Sandbox: 4000–4999
/// - Suspension: 5000–5999
/// - Internal: 9000–9999
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorCode(u32);

impl ErrorCode {
    pub const CONFIG_FILE_NOT_FOUND: Self = Self(1001);
    pub const CONFIG_PARSE_ERROR: Self = Self(1002);
    pub const CONFIG_INVALID_VALUE: Self = Self(1003);
    pub const CONFIG_MISSING_REQUIRED: Self = Self(1004);

    pub const INIT_FAILED: Self = Self(2001);
    pub const INIT_SUBSYSTEM_FAILED: Self = Self(2002);

    pub const SHUTDOWN_TIMEOUT: Self = Self(3001);
    pub const SIGNAL_HANDLER_ERROR: Self = Self(3002);

    pub const PANIC: Self = Self(9001);
    pub const INTERNAL: Self = Self(9002);

    pub fn category(self) -> &'static str {
        match self.0 {
            1000..=1999 => "configuration",
            2000..=2999 => "initialisation",
            3000..=3999 => "runtime",
            4000..=4999 => "sandbox",
            5000..=5999 => "suspension",
            _ => "internal",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The primary error type for the VibeGE Runtime.
///
/// All fallible operations return `Result<T, RuntimeError>`.
/// Every error carries a machine-readable code and a human-readable message.
#[derive(Error, Debug)]
pub struct RuntimeError {
    /// Machine-readable error code for tool consumption.
    pub code: ErrorCode,

    /// Human-readable error message suitable for end-user display.
    pub message: String,

    /// Source location (file:line) where the error originated.
    pub source_location: Option<String>,

    /// The underlying cause of this error, if any.
    #[source]
    pub cause: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl RuntimeError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source_location: None,
            cause: None,
        }
    }

    pub fn with_cause(
        code: ErrorCode,
        message: impl Into<String>,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            source_location: None,
            cause: Some(Box::new(cause)),
        }
    }

    pub fn at(mut self, location: &str) -> Self {
        self.source_location = Some(location.to_string());
        self
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {} (code: {})", self.code.category(), self.message, self.code)?;
        if let Some(loc) = &self.source_location {
            write!(f, " at {}", loc)?;
        }
        Ok(())
    }
}

impl From<std::io::Error> for RuntimeError {
    fn from(err: std::io::Error) -> Self {
        Self::with_cause(ErrorCode::INTERNAL, "I/O operation failed", err)
    }
}

impl From<toml::de::Error> for RuntimeError {
    fn from(err: toml::de::Error) -> Self {
        Self::with_cause(ErrorCode::CONFIG_PARSE_ERROR, "Failed to parse configuration file", err)
    }
}

impl From<serde_json::Error> for RuntimeError {
    fn from(err: serde_json::Error) -> Self {
        Self::with_cause(ErrorCode::CONFIG_PARSE_ERROR, "Failed to parse JSON configuration", err)
    }
}

/// Convenience type alias for runtime results.
pub type Result<T> = std::result::Result<T, RuntimeError>;
