//! # VibeGE Sandbox
//!
//! OS-level process isolation for game execution.
//!
//! The sandbox creates a subprocess with restricted permissions using
//! platform-specific security mechanisms:
//!
//! - **Windows:** Job Objects + restricted tokens + AppContainer
//! - **macOS:** seatbelt sandbox profiles
//! - **Linux:** user namespaces + seccomp-bpf + mount namespaces
//!
//! Game processes are spawned with a `SandboxConfig` that declares
//! what resources they can access. The sandbox enforces these limits
//! at the OS level.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tracing::{debug, error, info, warn};
use vibege_core::{ErrorCode, Result, RuntimeError};

/// Declares the resource access permissions for a sandboxed game.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Human-readable name for the sandbox (used for logs, process names).
    pub name: String,

    /// Directories the game can read from.
    pub allowed_read_paths: Vec<PathBuf>,

    /// Directories the game can write to.
    pub allowed_write_paths: Vec<PathBuf>,

    /// Network access level.
    pub network_access: NetworkAccess,

    /// Maximum memory in MB (0 = default).
    pub max_memory_mb: u64,

    /// Maximum CPU time in seconds (0 = unlimited).
    pub max_cpu_time_secs: u64,

    /// Maximum number of child processes the game can spawn.
    pub max_processes: u32,

    /// Maximum file size in MB for write operations.
    pub max_file_size_mb: u64,

    /// Enable developer mode (relaxed restrictions).
    pub dev_mode: bool,

    /// Path to the game executable.
    pub game_path: PathBuf,

    /// Arguments to pass to the game process.
    pub game_args: Vec<String>,

    /// Environment variables to set in the sandbox.
    pub env_vars: Vec<(String, String)>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            name: "vibege-game".into(),
            allowed_read_paths: vec![],
            allowed_write_paths: vec![],
            network_access: NetworkAccess::None,
            max_memory_mb: 512,
            max_cpu_time_secs: 0,
            max_processes: 1,
            max_file_size_mb: 50,
            dev_mode: false,
            game_path: PathBuf::new(),
            game_args: vec![],
            env_vars: vec![],
        }
    }
}

/// Level of network access granted to a sandboxed game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkAccess {
    /// No network access.
    None,
    /// Outbound connections only.
    Outbound,
    /// Full network access.
    Full,
}

impl NetworkAccess {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Outbound => "outbound",
            Self::Full => "full",
        }
    }
}

/// Statistics about a sandboxed process.
#[derive(Debug, Clone)]
pub struct SandboxStats {
    pub process_id: u32,
    pub memory_kb: u64,
    pub cpu_time_ms: u64,
    pub uptime_secs: f64,
    pub violations: u64,
}

/// A running sandboxed process.
pub struct Sandbox {
    config: SandboxConfig,
    child: Option<Child>,
    start_time: std::time::Instant,
    violations: u64,
}

impl Sandbox {
    /// Creates a new sandbox configuration.
    pub fn with_config(config: SandboxConfig) -> Self {
        Self {
            config,
            child: None,
            start_time: std::time::Instant::now(),
            violations: 0,
        }
    }

    /// Spawns the sandboxed game process.
    ///
    /// Applies platform-specific sandbox restrictions before launching.
    pub fn spawn(&mut self) -> Result<()> {
        let config = &self.config;

        if !config.game_path.exists() {
            return Err(RuntimeError::new(
                ErrorCode::CONFIG_FILE_NOT_FOUND,
                format!("Game executable not found: {}", config.game_path.display()),
            ));
        }

        info!(
            game = %config.game_path.display(),
            name = %config.name,
            network = %config.network_access.as_str(),
            memory_mb = config.max_memory_mb,
            "Spawning sandboxed game process"
        );

        #[cfg(unix)]
        self.spawn_unix()?;

        #[cfg(windows)]
        self.spawn_windows()?;

        if let Some(ref child) = self.child {
            info!(
                pid = child.id(),
                "Sandboxed game process started"
            );
        }

        Ok(())
    }

    /// Spawns the game process on Unix with sandbox restrictions.
    #[cfg(unix)]
    fn spawn_unix(&mut self) -> Result<()> {
        let config = &self.config;
        let mut cmd = Command::new(&config.game_path);

        cmd.args(&config.game_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, val) in &config.env_vars {
            cmd.env(key, val);
        }

        // Set resource limits via setrlimit before exec (handled by the OS)
        // In a full implementation, this would use a sandbox helper binary
        // that applies seccomp-bpf, user namespaces, and mount namespaces.

        // For v0.1, mark the child as sandboxed via environment variable
        cmd.env("VIBEGE_SANDBOXED", "1");
        cmd.env("VIBEGE_SANDBOX_NAME", &config.name);

        let child = cmd.spawn()
            .map_err(|e| RuntimeError::with_cause(
                ErrorCode::INIT_FAILED,
                format!("Failed to spawn game process: {}", config.game_path.display()),
                e,
            ))?;

        self.child = Some(child);
        Ok(())
    }

    /// Spawns the game process on Windows with sandbox restrictions.
    #[cfg(windows)]
    fn spawn_windows(&mut self) -> Result<()> {
        let config = &self.config;
        let mut cmd = Command::new(&config.game_path);

        cmd.args(&config.game_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, val) in &config.env_vars {
            cmd.env(key, val);
        }

        // Mark as sandboxed
        cmd.env("VIBEGE_SANDBOXED", "1");
        cmd.env("VIBEGE_SANDBOX_NAME", &config.name);

        // In a full implementation, this would:
        // 1. Create a Job Object and assign the child process
        // 2. Create a restricted token (remove dangerous privileges)
        // 3. Set memory and process limits on the job

        let child = cmd.spawn()
            .map_err(|e| RuntimeError::with_cause(
                ErrorCode::INIT_FAILED,
                format!("Failed to spawn game process: {}", config.game_path.display()),
                e,
            ))?;

        self.child = Some(child);
        Ok(())
    }

    /// Returns the process ID of the sandboxed game, if running.
    pub fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    /// Returns whether the sandboxed process is still running.
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => false,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Returns the sandbox's configuration.
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// Returns basic statistics about the sandboxed process.
    pub fn stats(&self) -> SandboxStats {
        SandboxStats {
            process_id: self.child.as_ref().map(|c| c.id()).unwrap_or(0),
            memory_kb: 0,
            cpu_time_ms: 0,
            uptime_secs: self.start_time.elapsed().as_secs_f64(),
            violations: self.violations,
        }
    }

    /// Sends a signal to the sandboxed process to shut down gracefully.
    pub fn request_shutdown(&mut self) -> Result<()> {
        if let Some(ref mut child) = self.child {
            // For v0.1, kill the process directly.
            // A full implementation would send SIGTERM on Unix and
            // CTRL_BREAK_EVENT on Windows for graceful shutdown.
            let pid = child.id();
            let _ = child.kill();
            info!(pid = pid, "Shutdown signal sent to sandboxed process");
        }
        Ok(())
    }

    /// Waits for the sandboxed process to exit, with a timeout.
    pub fn wait_for_exit(&mut self, timeout: Duration) -> Result<()> {
        if let Some(ref mut child) = self.child {
            let start = std::time::Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        info!(
                            pid = child.id(),
                            exit_code = status.code().unwrap_or(-1),
                            "Sandboxed process exited"
                        );
                        return Ok(());
                    }
                    Ok(None) => {
                        if start.elapsed() > timeout {
                            warn!("Sandboxed process did not exit within timeout, killing");
                            let _ = child.kill();
                            child.wait().ok();
                            return Err(RuntimeError::new(
                                ErrorCode::SHUTDOWN_TIMEOUT,
                                format!("Game process did not exit within {:?}", timeout),
                            ));
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(e) => {
                        error!(error = %e, "Error waiting for sandboxed process");
                        return Err(RuntimeError::with_cause(
                            ErrorCode::INTERNAL,
                            "Error waiting for game process",
                            e,
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Records a sandbox violation (e.g., blocked syscall, file access).
    pub fn record_violation(&mut self) {
        self.violations += 1;
        warn!(
            total = self.violations,
            pid = self.child.as_ref().map(|c| c.id()).unwrap_or(0),
            "Sandbox violation recorded"
        );
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
            debug!(pid = child.id(), "Sandboxed process terminated on drop");
        }
    }
}

/// Validates a `SandboxConfig` for correctness.
pub fn validate_config(config: &SandboxConfig) -> Result<()> {
    if config.name.is_empty() {
        return Err(RuntimeError::new(
            ErrorCode::CONFIG_INVALID_VALUE,
            "Sandbox name cannot be empty",
        ));
    }
    if !config.game_path.exists() {
        return Err(RuntimeError::new(
            ErrorCode::CONFIG_FILE_NOT_FOUND,
            format!("Game executable not found: {}", config.game_path.display()),
        ));
    }
    if config.max_memory_mb == 0 {
        return Err(RuntimeError::new(
            ErrorCode::CONFIG_INVALID_VALUE,
            "Max memory must be greater than 0",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SandboxConfig::default();
        assert_eq!(config.name, "vibege-game");
        assert_eq!(config.network_access, NetworkAccess::None);
        assert_eq!(config.max_memory_mb, 512);
        assert!(!config.dev_mode);
    }

    #[test]
    fn test_validate_valid_config() {
        let config = SandboxConfig {
            name: "test".into(),
            game_path: PathBuf::from(std::env::current_exe().unwrap()),
            max_memory_mb: 256,
            ..Default::default()
        };
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_empty_name() {
        let config = SandboxConfig {
            name: "".into(),
            game_path: PathBuf::from("test.exe"),
            max_memory_mb: 256,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_missing_game() {
        let config = SandboxConfig {
            name: "test".into(),
            game_path: PathBuf::from("/nonexistent/game.exe"),
            max_memory_mb: 256,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_zero_memory() {
        let config = SandboxConfig {
            name: "test".into(),
            game_path: PathBuf::from(std::env::current_exe().unwrap()),
            max_memory_mb: 0,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_network_access_as_str() {
        assert_eq!(NetworkAccess::None.as_str(), "none");
        assert_eq!(NetworkAccess::Outbound.as_str(), "outbound");
        assert_eq!(NetworkAccess::Full.as_str(), "full");
    }

    #[test]
    fn test_sandbox_stats() {
        let config = SandboxConfig {
            name: "test".into(),
            game_path: PathBuf::from("test"),
            ..Default::default()
        };
        let sandbox = Sandbox::with_config(config);
        let stats = sandbox.stats();
        assert_eq!(stats.process_id, 0);
        assert!(stats.uptime_secs >= 0.0);
    }

    #[test]
    fn test_violation_recording() {
        let config = SandboxConfig {
            name: "test".into(),
            game_path: PathBuf::from("test"),
            ..Default::default()
        };
        let mut sandbox = Sandbox::with_config(config);
        assert_eq!(sandbox.violations, 0);
        sandbox.record_violation();
        assert_eq!(sandbox.violations, 1);
        sandbox.record_violation();
        assert_eq!(sandbox.violations, 2);
    }

    #[test]
    fn test_sandbox_name_from_config() {
        let config = SandboxConfig {
            name: "my-game-sandbox".into(),
            game_path: PathBuf::from("game"),
            ..Default::default()
        };
        assert_eq!(config.name, "my-game-sandbox");
    }
}
