use vibege_config::config::{
    AudioConfig, DeveloperConfig, GeneralConfig, GraphicsConfig, InputConfig, OverlayConfig,
    VibegeConfig,
};
use vibege_config::{ConfigHandle, Validate, installed_games_dir};

fn default_config() -> VibegeConfig {
    VibegeConfig::default()
}

#[test]
fn test_default_config_created() {
    let cfg = default_config();
    assert_eq!(cfg.general.startup_behavior, "hidden");
    assert_eq!(cfg.general.performance_mode, "balanced");
}

#[test]
fn test_audio_config_defaults() {
    let cfg = AudioConfig::default();
    assert!((cfg.volume - 0.7).abs() < 1e-6);
    assert!(!cfg.muted);
    assert!((cfg.music_volume - 0.7).abs() < 1e-6);
    assert!((cfg.sfx_volume - 0.8).abs() < 1e-6);
}

#[test]
fn test_graphics_config_defaults() {
    let cfg = GraphicsConfig::default();
    assert_eq!(cfg.width, 1280);
    assert_eq!(cfg.height, 720);
    assert!(cfg.vsync);
    assert_eq!(cfg.dpi_scale, 1.0);
}

#[test]
fn test_input_config_defaults() {
    let cfg = InputConfig::default();
    assert!((cfg.mouse_sensitivity - 1.0).abs() < 1e-6);
}

#[test]
fn test_overlay_config_defaults() {
    let cfg = OverlayConfig::default();
    assert_eq!(cfg.hotkey_modifiers, "ctrl+shift");
    assert_eq!(cfg.hotkey_key, "v");
    assert_eq!(cfg.width, 800);
}

#[test]
fn test_developer_config_defaults() {
    let cfg = DeveloperConfig::default();
    assert!(!cfg.dev_mode);
}

#[test]
fn test_general_config_defaults() {
    let cfg = GeneralConfig::default();
    assert_eq!(cfg.startup_behavior, "hidden");
    assert_eq!(cfg.backend_url, "http://localhost:3000/api/v1");
    assert_eq!(cfg.theme, "dark");
}

#[test]
fn test_config_roundtrip_toml() {
    let cfg = default_config();
    let toml_str = toml::to_string_pretty(&cfg).expect("serialize");
    let deserialized: VibegeConfig = toml::from_str(&toml_str).expect("deserialize");
    assert_eq!(
        deserialized.general.startup_behavior,
        cfg.general.startup_behavior
    );
    assert_eq!(deserialized.graphics.width, cfg.graphics.width);
}

#[test]
fn test_config_validate_valid() {
    let cfg = default_config();
    assert!(cfg.validate().is_ok());
}

#[test]
fn test_config_validate_invalid_graphics() {
    let mut cfg = default_config();
    cfg.graphics.width = 0;
    assert!(cfg.validate().is_err());
}

#[test]
fn test_config_validate_invalid_overlay() {
    let mut cfg = default_config();
    cfg.overlay.hotkey_key = "invalid".into();
    assert!(cfg.validate().is_err());
}

#[test]
fn test_overlay_validate_hotkey_mod() {
    let cfg = OverlayConfig {
        hotkey_modifiers: "invalid".into(),
        ..OverlayConfig::default()
    };
    assert!(cfg.validate().is_err());
    let mut cfg2 = OverlayConfig {
        hotkey_modifiers: "invalid".into(),
        ..OverlayConfig::default()
    };
    cfg2.sanitize();
    assert_eq!(cfg2.hotkey_modifiers, "ctrl+shift");
}

#[test]
fn test_overlay_validate_dimensions() {
    let cfg = OverlayConfig {
        width: 100,
        ..OverlayConfig::default()
    };
    assert!(cfg.validate().is_err());
    let mut cfg2 = OverlayConfig {
        width: 100,
        ..OverlayConfig::default()
    };
    cfg2.sanitize();
    assert_eq!(cfg2.width, 200);
}

#[test]
fn test_general_validate_startup() {
    let cfg = GeneralConfig {
        startup_behavior: "invalid".into(),
        ..GeneralConfig::default()
    };
    assert!(cfg.validate().is_err());
    let mut cfg2 = GeneralConfig {
        startup_behavior: "invalid".into(),
        ..GeneralConfig::default()
    };
    cfg2.sanitize();
    assert_eq!(cfg2.startup_behavior, "hidden");
}

#[test]
fn test_general_validate_theme() {
    let cfg = GeneralConfig {
        theme: "neon".into(),
        ..GeneralConfig::default()
    };
    assert!(cfg.validate().is_err());
    let mut cfg2 = GeneralConfig {
        theme: "neon".into(),
        ..GeneralConfig::default()
    };
    cfg2.sanitize();
    assert_eq!(cfg2.theme, "dark");
}

#[test]
fn test_general_validate_backend_url() {
    let cfg = GeneralConfig {
        backend_url: "not-a-url".into(),
        ..GeneralConfig::default()
    };
    assert!(cfg.validate().is_err());
    let mut cfg2 = GeneralConfig {
        backend_url: "not-a-url".into(),
        ..GeneralConfig::default()
    };
    cfg2.sanitize();
    assert!(cfg2.backend_url.starts_with("http"));
}

#[test]
fn test_config_validate_and_fix() {
    let mut cfg = default_config();
    cfg.graphics.width = 99999;
    cfg.overlay.hotkey_key = "bad".into();
    let result = cfg.validate_and_fix();
    assert!(result.is_ok(), "should auto-fix: {:?}", result);
}

#[test]
fn test_active_profile_default() {
    let cfg = default_config();
    assert_eq!(cfg.active_profile, "Default");
}

#[test]
fn test_serde_json_compatibility() {
    let cfg = default_config();
    let json = serde_json::to_string(&cfg).expect("serialize");
    let deserialized: VibegeConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.graphics.width, 1280);
}

#[test]
fn test_config_handle_creation() {
    let handle = ConfigHandle::new();
    let cfg = handle.get();
    assert_eq!(cfg.general.startup_behavior, "hidden");
}

#[test]
fn test_installed_games_dir() {
    let dir = installed_games_dir();
    assert!(dir.to_string_lossy().contains("vibege"));
}
