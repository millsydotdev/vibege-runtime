use std::path::PathBuf;
use clap::Parser;
use serde::Deserialize;

use crate::error::{ErrorCode, Result, RuntimeError};

/// Log verbosity level.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = RuntimeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(RuntimeError::new(
                ErrorCode::CONFIG_INVALID_VALUE,
                format!("Invalid log level: '{s}'. Expected one of: trace, debug, info, warn, error"),
            )),
        }
    }
}

/// Window configuration settings.
#[derive(Debug, Clone, Deserialize)]
pub struct WindowConfig {
    /// Window title shown in the title bar.
    #[serde(default = "default_window_title")]
    pub title: String,

    /// Initial window width in pixels.
    #[serde(default = "default_window_width")]
    pub width: u32,

    /// Initial window height in pixels.
    #[serde(default = "default_window_height")]
    pub height: u32,

    /// Start in fullscreen mode.
    #[serde(default)]
    pub fullscreen: bool,

    /// Enable vertical sync.
    #[serde(default = "default_vsync")]
    pub vsync: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: default_window_title(),
            width: default_window_width(),
            height: default_window_height(),
            fullscreen: false,
            vsync: default_vsync(),
        }
    }
}

fn default_window_title() -> String { "VibeGE Runtime".to_string() }
const fn default_window_width() -> u32 { 1280 }
const fn default_window_height() -> u32 { 720 }
const fn default_vsync() -> bool { true }

/// Complete runtime configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    /// Log verbosity level.
    #[serde(default)]
    pub log_level: LogLevel,

    /// Maximum frames per second (0 = unlimited).
    #[serde(default = "default_fps_limit")]
    pub fps_limit: u32,

    /// Working directory for the runtime.
    pub working_dir: Option<PathBuf>,

    /// Enable developer mode features (overlay, console, profiling).
    #[serde(default)]
    pub dev_mode: bool,

    /// Window configuration.
    #[serde(default)]
    pub window: WindowConfig,
}

const fn default_fps_limit() -> u32 { 0 }

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            log_level: LogLevel::default(),
            fps_limit: default_fps_limit(),
            working_dir: None,
            dev_mode: false,
            window: WindowConfig::default(),
        }
    }
}

/// CLI arguments parsed from command-line invocation.
#[derive(Parser, Debug, Clone)]
#[command(name = "vibege-runtime", version, about = "VibeGE Game Runtime")]
pub struct CliArgs {
    /// Path to a configuration file.
    #[arg(short = 'c', long = "config", env = "VIBEGE_CONFIG")]
    pub config: Option<PathBuf>,

    /// Log level override.
    #[arg(short = 'l', long = "log-level", env = "VIBEGE_LOG_LEVEL")]
    pub log_level: Option<String>,

    /// Working directory.
    #[arg(short = 'C', long = "working-dir", env = "VIBEGE_WORKING_DIR")]
    pub working_dir: Option<PathBuf>,

    /// Enable developer mode.
    #[arg(long = "dev", env = "VIBEGE_DEV_MODE")]
    pub dev_mode: bool,

    /// FPS limit.
    #[arg(long = "fps-limit", env = "VIBEGE_FPS_LIMIT")]
    pub fps_limit: Option<u32>,
}

/// Merged result of loading configuration from all sources.
#[derive(Debug, Clone)]
pub struct MergedConfig {
    pub config: RuntimeConfig,
    pub config_file_path: Option<PathBuf>,
}

/// Loads and merges configuration from all available sources.
///
/// Priority order (highest to lowest):
/// 1. CLI arguments
/// 2. Environment variables
/// 3. Configuration file
/// 4. Default values
pub fn load_config() -> Result<MergedConfig> {
    let cli = CliArgs::parse();

    let mut config = RuntimeConfig::default();
    let mut config_file_path: Option<PathBuf> = None;

    // 1. Load from configuration file (lowest priority source)
    if let Some(cfg_path) = &cli.config {
        let content = std::fs::read_to_string(cfg_path)
            .map_err(|e| RuntimeError::with_cause(
                ErrorCode::CONFIG_FILE_NOT_FOUND,
                format!("Failed to read config file: {}", cfg_path.display()),
                e,
            ))?;
        let file_config: RuntimeConfig = toml::from_str(&content)?;
        config = merge_config(config, file_config);
        config_file_path = Some(cfg_path.clone());
    } else {
        // Try default config file paths
        for path in &[PathBuf::from("vibege.toml"), get_default_config_path()] {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| RuntimeError::with_cause(
                        ErrorCode::CONFIG_FILE_NOT_FOUND,
                        format!("Failed to read config file: {}", path.display()),
                        e,
                    ))?;
                let file_config: RuntimeConfig = toml::from_str(&content)?;
                config = merge_config(config, file_config);
                config_file_path = Some(path.clone());
                break;
            }
        }
    }

    // 2. Apply CLI/environment overrides on top of file config
    if let Some(level) = &cli.log_level {
        config.log_level = level.parse()?;
    }
    if let Some(dir) = &cli.working_dir {
        config.working_dir = Some(dir.clone());
    }
    if cli.dev_mode {
        config.dev_mode = true;
    }
    if let Some(fps) = cli.fps_limit {
        config.fps_limit = fps;
    }

    Ok(MergedConfig {
        config,
        config_file_path,
    })
}

fn merge_config(base: RuntimeConfig, override_cfg: RuntimeConfig) -> RuntimeConfig {
    RuntimeConfig {
        log_level: override_cfg.log_level,
        fps_limit: override_cfg.fps_limit,
        working_dir: override_cfg.working_dir.or(base.working_dir),
        dev_mode: override_cfg.dev_mode || base.dev_mode,
        window: WindowConfig {
            title: if override_cfg.window.title != default_window_title() {
                override_cfg.window.title
            } else {
                base.window.title
            },
            width: override_cfg.window.width,
            height: override_cfg.window.height,
            fullscreen: override_cfg.window.fullscreen || base.window.fullscreen,
            vsync: override_cfg.window.vsync,
        },
    }
}

fn get_default_config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".vibege").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_log_level_parsing() {
        assert_eq!("info".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("DEBUG".parse::<LogLevel>().unwrap(), LogLevel::Debug);
        assert_eq!("Trace".parse::<LogLevel>().unwrap(), LogLevel::Trace);
        assert!("invalid".parse::<LogLevel>().is_err());
    }

    #[test]
    fn test_default_config() {
        let config = RuntimeConfig::default();
        assert_eq!(config.log_level, LogLevel::Info);
        assert_eq!(config.fps_limit, 0);
        assert!(!config.dev_mode);
        assert_eq!(config.window.width, 1280);
        assert_eq!(config.window.height, 720);
        assert_eq!(config.window.title, "VibeGE Runtime");
    }

    #[test]
    fn test_config_from_toml() {
        let toml_str = r#"
log_level = "debug"
fps_limit = 144
dev_mode = true

[window]
title = "Test Game"
width = 1920
height = 1080
"#;
        let config: RuntimeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.log_level, LogLevel::Debug);
        assert_eq!(config.fps_limit, 144);
        assert!(config.dev_mode);
        assert_eq!(config.window.width, 1920);
        assert_eq!(config.window.height, 1080);
        assert_eq!(config.window.title, "Test Game");
    }

    #[test]
    fn test_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("vibege.toml");
        fs::write(&config_path, r#"
log_level = "warn"
fps_limit = 60
"#).unwrap();

        // Override config path via CLI (simulated)
        let content = fs::read_to_string(&config_path).unwrap();
        let file_config: RuntimeConfig = toml::from_str(&content).unwrap();
        let merged = merge_config(RuntimeConfig::default(), file_config);
        assert_eq!(merged.log_level, LogLevel::Warn);
        assert_eq!(merged.fps_limit, 60);
    }

    #[test]
    fn test_merge_priority() {
        let base = RuntimeConfig {
            log_level: LogLevel::Info,
            fps_limit: 60,
            ..Default::default()
        };
        let override_cfg = RuntimeConfig {
            log_level: LogLevel::Debug,
            ..Default::default()
        };
        let merged = merge_config(base, override_cfg);
        // log_level should be overridden, fps_limit should keep override default
        assert_eq!(merged.log_level, LogLevel::Debug);
    }
}
