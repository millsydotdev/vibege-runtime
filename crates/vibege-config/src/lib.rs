use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OverlayConfig {
    #[serde(default = "str_ctrl_shift")]
    pub hotkey_modifiers: String,
    #[serde(default = "str_v")]
    pub hotkey_key: String,
    #[serde(default = "str_center")]
    pub position: String,
    #[serde(default = "u800")]
    pub width: u32,
    #[serde(default = "u600")]
    pub height: u32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioConfig {
    #[serde(default = "f07")]
    pub volume: f32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "str_hidden")]
    pub startup_behavior: String,
    #[serde(default = "str_balanced")]
    pub performance_mode: String,
    #[serde(default)]
    pub first_run_complete: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VibegeConfig {
    #[serde(default)]
    pub overlay: OverlayConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub general: GeneralConfig,
}

fn str_ctrl_shift() -> String { "ctrl+shift".into() }
fn str_v() -> String { "v".into() }
fn str_center() -> String { "center".into() }
fn u800() -> u32 { 800 }
fn u600() -> u32 { 600 }
fn f07() -> f32 { 0.7 }
fn str_hidden() -> String { "hidden".into() }
fn str_balanced() -> String { "balanced".into() }

impl Default for VibegeConfig {
    fn default() -> Self {
        Self {
            overlay: OverlayConfig {
                hotkey_modifiers: str_ctrl_shift(),
                hotkey_key: str_v(),
                position: str_center(),
                width: u800(),
                height: u600(),
            },
            audio: AudioConfig { volume: f07() },
            general: GeneralConfig {
                startup_behavior: str_hidden(),
                performance_mode: str_balanced(),
                first_run_complete: false,
            },
        }
    }
}

pub fn installed_games_dir() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("vibege").join("games")
    } else {
        PathBuf::from(".vibege/installed-games")
    }
}

pub fn config_path() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("vibege").join("config.toml")
    } else {
        PathBuf::from(".vibege/config.toml")
    }
}

pub fn load_config() -> VibegeConfig {
    let path = config_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => VibegeConfig::default(),
        }
    } else {
        VibegeConfig::default()
    }
}

pub fn save_config(config: &VibegeConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = toml::to_string_pretty(config) {
        let _ = fs::write(&path, content);
    }
}

pub struct ConfigHandle {
    inner: Mutex<VibegeConfig>,
}

impl ConfigHandle {
    pub fn new() -> Self {
        Self { inner: Mutex::new(load_config()) }
    }

    pub fn get(&self) -> VibegeConfig {
        self.inner.lock().unwrap().clone()
    }

    pub fn set(&self, config: VibegeConfig) {
        *self.inner.lock().unwrap() = config.clone();
        save_config(&config);
    }

    pub fn is_first_run(&self) -> bool {
        !self.inner.lock().unwrap().general.first_run_complete
    }

    pub fn complete_first_run(&self) {
        let mut c = self.inner.lock().unwrap();
        c.general.first_run_complete = true;
        save_config(&c);
    }
}
