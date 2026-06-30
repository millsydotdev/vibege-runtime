/// Errors that can occur during game lifecycle operations.
#[derive(Debug)]
pub enum RuntimeError {
    PackageNotFound(String),
    InvalidPackage(String),
    ManifestMissing,
    ManifestParseFailed(String),
    EntryPointMissing(String),
    EntryPointNotFound(String),
    CorruptAsset(String),
    AssetPathTraversal(String),
    VersionMismatch { found: String, required: String },
    SdkIncompatible { reason: String },
    EngineIncompatible { found: String, required: String },
    PermissionDenied(String),
    IntegrityCheckFailed(String),
    LuaRuntimeError(String),
    LuaPanic(String),
    SdkRegistrationFailed(String),
    SessionAlreadyActive,
    SessionNotActive,
    SuspendFailed(String),
    ResumeFailed(String),
    CleanupFailed(String),
    ShutdownTimeout,
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::PackageNotFound(name) => write!(f, "Package not found: {name}"),
            RuntimeError::InvalidPackage(msg) => write!(f, "Invalid package: {msg}"),
            RuntimeError::ManifestMissing => write!(f, "Manifest is missing from package"),
            RuntimeError::ManifestParseFailed(msg) => write!(f, "Manifest parse failed: {msg}"),
            RuntimeError::EntryPointMissing(name) => {
                write!(f, "Entry point not specified in manifest for: {name}")
            }
            RuntimeError::EntryPointNotFound(path) => {
                write!(f, "Entry point not found in package: {path}")
            }
            RuntimeError::VersionMismatch { found, required } => {
                write!(f, "Version mismatch: found {found}, required {required}")
            }
            RuntimeError::SdkIncompatible { reason } => {
                write!(f, "SDK incompatible: {reason}")
            }
            RuntimeError::EngineIncompatible { found, required } => {
                write!(f, "Engine incompatible: found {found}, required {required}")
            }
            RuntimeError::PermissionDenied(perm) => {
                write!(f, "Permission denied: {perm}")
            }
            RuntimeError::IntegrityCheckFailed(msg) => {
                write!(f, "Integrity check failed: {msg}")
            }
            RuntimeError::CorruptAsset(msg) => write!(f, "Corrupt asset: {msg}"),
            RuntimeError::AssetPathTraversal(path) => {
                write!(f, "Asset path traversal detected: {path}")
            }
            RuntimeError::LuaRuntimeError(msg) => write!(f, "Lua error: {msg}"),
            RuntimeError::LuaPanic(msg) => write!(f, "Lua panic: {msg}"),
            RuntimeError::SdkRegistrationFailed(msg) => write!(f, "SDK registration failed: {msg}"),
            RuntimeError::SessionAlreadyActive => {
                write!(f, "A session is already active")
            }
            RuntimeError::SessionNotActive => write!(f, "No active session"),
            RuntimeError::SuspendFailed(msg) => write!(f, "Suspend failed: {msg}"),
            RuntimeError::ResumeFailed(msg) => write!(f, "Resume failed: {msg}"),
            RuntimeError::CleanupFailed(msg) => write!(f, "Cleanup failed: {msg}"),
            RuntimeError::ShutdownTimeout => write!(f, "Shutdown timed out"),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<String> for RuntimeError {
    fn from(msg: String) -> Self {
        RuntimeError::LuaRuntimeError(msg)
    }
}
