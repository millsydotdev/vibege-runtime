//! # VibeGE Sandbox
//!
//! OS-level process isolation for game execution.
//!
//! The sandbox creates a subprocess with restricted permissions using
//! platform-specific security mechanisms:
//!
//! - **Windows:** Job Objects (memory/process limits, kill-on-close)
//! - **macOS:** (planned) seatbelt sandbox profiles
//! - **Linux:** (planned) user namespaces + seccomp-bpf
//!
//! ## Current Status
//!
//! Windows: Real Job Object implementation with memory limits,
//! process count limits, and kill-on-close semantics.
//!
//! Unix: Environment-variable stub (full sandbox requires
//! platform-specific helper binary or LD_PRELOAD interposition).

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tracing::{debug, error, info, warn};
use vibege_core::{ErrorCode, Result, RuntimeError};

/// Declares the resource access permissions for a sandboxed game.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub name: String,
    pub allowed_read_paths: Vec<PathBuf>,
    pub allowed_write_paths: Vec<PathBuf>,
    pub network_access: NetworkAccess,
    pub max_memory_mb: u64,
    pub max_cpu_time_secs: u64,
    pub max_processes: u32,
    pub max_file_size_mb: u64,
    pub dev_mode: bool,
    pub game_path: PathBuf,
    pub game_args: Vec<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkAccess {
    None,
    Outbound,
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
    #[cfg(windows)]
    job_handle: Option<std::os::windows::io::OwnedHandle>,
}

impl Sandbox {
    pub fn with_config(config: SandboxConfig) -> Self {
        Self {
            config,
            child: None,
            start_time: std::time::Instant::now(),
            violations: 0,
            #[cfg(windows)]
            job_handle: None,
        }
    }

    /// Spawns the sandboxed game process with platform-specific restrictions.
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
            info!(pid = child.id(), "Sandboxed game process started");
        }
        Ok(())
    }

    // ─── Unix (stub — env vars only) ─────────────────────────────────

    /// On Unix, full sandboxing requires seccomp-bpf / user namespaces
    /// which need either a helper binary or LD_PRELOAD interposition.
    /// For now, mark the process with env vars so games can detect
    /// they are running sandboxed.
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
        cmd.env("VIBEGE_SANDBOXED", "1");
        cmd.env("VIBEGE_SANDBOX_NAME", &config.name);

        let max_proc = config.max_processes;
        let max_fsize_mb = config.max_file_size_mb;
        let max_mem_mb = config.max_memory_mb;

        unsafe {
            cmd.pre_exec(move || {
                libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
                let mem_bytes = (max_mem_mb as u64) * 1024 * 1024;
                let rlim = libc::rlimit {
                    rlim_cur: mem_bytes,
                    rlim_max: mem_bytes,
                };
                libc::setrlimit(libc::RLIMIT_AS, &rlim);
                libc::setrlimit(libc::RLIMIT_DATA, &rlim);
                let stack = mem_bytes.min(8 * 1024 * 1024);
                let rlim_s = libc::rlimit {
                    rlim_cur: stack,
                    rlim_max: stack,
                };
                libc::setrlimit(libc::RLIMIT_STACK, &rlim_s);
                let rlim_n = libc::rlimit {
                    rlim_cur: max_proc as u64,
                    rlim_max: max_proc as u64,
                };
                libc::setrlimit(libc::RLIMIT_NPROC, &rlim_n);
                let fsize = (max_fsize_mb as u64) * 1024 * 1024;
                let rlim_f = libc::rlimit {
                    rlim_cur: fsize,
                    rlim_max: fsize,
                };
                libc::setrlimit(libc::RLIMIT_FSIZE, &rlim_f);
                Ok(())
            })
        }

        let child = cmd.spawn().map_err(|e| {
            RuntimeError::with_cause(
                ErrorCode::INIT_FAILED,
                format!(
                    "Failed to spawn game process: {}",
                    config.game_path.display()
                ),
                e,
            )
        })?;
        self.child = Some(child);
        Ok(())
    }

    // ─── Windows (Job Object implementation) ─────────────────────────

    /// Spawns the game process on Windows with real Job Object isolation:
    ///
    /// 1. Creates a Job Object with kill-on-close, memory limit, process limit
    /// 2. Spawns the process
    /// 3. Assigns the process to the job
    ///
    /// Future: restricted token + AppContainer for stronger isolation.
    #[cfg(windows)]
    fn spawn_windows(&mut self) -> Result<()> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::*;

        let config = &self.config;

        // 1. Create Job Object
        let job_name: Vec<u16> = OsStr::new(&format!("vibege_{}", config.name))
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let job = unsafe { CreateJobObjectW(std::ptr::null(), job_name.as_ptr()) };
        if job == 0 {
            return Err(RuntimeError::new(
                ErrorCode::INIT_FAILED,
                "Failed to create Job Object for sandbox",
            ));
        }

        // 2. Set job limits
        let mem_limit = config.max_memory_mb * 1024 * 1024;

        let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                PerProcessUserTimeLimit: 0,
                PerJobUserTimeLimit: 0,
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                    | JOB_OBJECT_LIMIT_PROCESS_MEMORY
                    | JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
                MinimumWorkingSetSize: 0,
                MaximumWorkingSetSize: 0,
                ActiveProcessLimit: if config.max_processes > 0 {
                    config.max_processes
                } else {
                    1
                },
                Affinity: 0,
                PriorityClass: 0,
                SchedulingClass: 0,
            },
            IoInfo: unsafe { std::mem::zeroed() },
            ProcessMemoryLimit: mem_limit as usize,
            JobMemoryLimit: 0,
            PeakProcessMemoryUsed: 0,
            PeakJobMemoryUsed: 0,
        };

        let result = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if result == 0 {
            unsafe { CloseHandle(job) };
            return Err(RuntimeError::new(
                ErrorCode::INIT_FAILED,
                "Failed to set Job Object limits",
            ));
        }

        // 3. Spawn the process
        let mut cmd = Command::new(&config.game_path);
        cmd.args(&config.game_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, val) in &config.env_vars {
            cmd.env(key, val);
        }
        cmd.env("VIBEGE_SANDBOXED", "1");
        cmd.env("VIBEGE_SANDBOX_NAME", &config.name);

        let child = cmd.spawn().map_err(|e| {
            unsafe { CloseHandle(job) };
            RuntimeError::with_cause(
                ErrorCode::INIT_FAILED,
                format!(
                    "Failed to spawn game process: {}",
                    config.game_path.display()
                ),
                e,
            )
        })?;

        let child_pid = child.id();

        // 4. Assign process to job
        let raw_handle = child.as_raw_handle() as isize;
        let result = unsafe { AssignProcessToJobObject(job, raw_handle) };
        if result == 0 {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &child_pid.to_string(), "/F"])
                .output();
            unsafe { CloseHandle(job) };
            return Err(RuntimeError::new(
                ErrorCode::INIT_FAILED,
                "Failed to assign game process to Job Object — game will not be sandboxed",
            ));
        }

        // Store the job handle — when it drops, KillOnJobClose kills all processes
        let job_handle = unsafe {
            std::os::windows::io::OwnedHandle::from_raw_handle(job as *mut std::ffi::c_void)
        };

        self.child = Some(child);
        self.job_handle = Some(job_handle);

        info!(
            pid = child_pid,
            memory_mb = config.max_memory_mb,
            max_processes = config.max_processes,
            "Game process assigned to Job Object"
        );

        Ok(())
    }

    // ─── Common API ─────────────────────────────────────────────────

    pub fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    pub fn stats(&self) -> SandboxStats {
        SandboxStats {
            process_id: self.child.as_ref().map(|c| c.id()).unwrap_or(0),
            memory_kb: 0,
            cpu_time_ms: 0,
            uptime_secs: self.start_time.elapsed().as_secs_f64(),
            violations: self.violations,
        }
    }

    pub fn request_shutdown(&mut self) -> Result<()> {
        if let Some(ref mut child) = self.child {
            let pid = child.id();
            let _ = child.kill();
            info!(pid = pid, "Shutdown signal sent to sandboxed process");
        }
        Ok(())
    }

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
        // On Windows, the job handle drop triggers JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        // which kills all remaining processes in the job.
    }
}

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
    }

    #[test]
    fn test_validate_valid_config() {
        let config = SandboxConfig {
            name: "test".into(),
            game_path: std::env::current_exe().unwrap(),
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
            game_path: std::env::current_exe().unwrap(),
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
    }
}
