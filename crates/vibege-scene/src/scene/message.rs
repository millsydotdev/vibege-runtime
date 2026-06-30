/// A structured message sent between scenes or broadcast by the SceneManager.
///
/// Messages are the primary mechanism for decoupled scene communication.
/// Rather than holding direct references, scenes send typed messages
/// that the SceneManager routes to the appropriate recipients.
#[derive(Debug, Clone)]
pub enum SceneMessage {
    /// A custom message with a name and JSON payload.
    Custom { name: String, payload: String },

    /// Request that the manager pop the current scene.
    RequestPop,

    /// Request that the manager push a scene of the given type.
    RequestPush(SceneId),

    /// A game was launched.
    GameLaunched { name: String },

    /// A game exited.
    GameExited { name: String, reason: String },

    /// The overlay was toggled hidden/shown.
    OverlayToggled { visible: bool },

    /// Settings were modified.
    SettingsChanged,

    /// Scene state was saved.
    StateSaved { scene_id: SceneId },

    /// Scene state was restored.
    StateRestored { scene_id: SceneId },

    /// An error occurred in a scene.
    Error { scene_id: SceneId, message: String },

    /// Focus changed (window gained/lost focus).
    FocusChanged { focused: bool },

    /// Window resize event.
    Resized { width: u32, height: u32 },
}

use crate::scene::SceneId;

impl SceneMessage {
    /// Create a custom message.
    pub fn custom(name: &str, payload: &str) -> Self {
        Self::Custom {
            name: name.to_string(),
            payload: payload.to_string(),
        }
    }

    /// Create an error message for a scene.
    pub fn error(scene_id: SceneId, message: &str) -> Self {
        Self::Error {
            scene_id,
            message: message.to_string(),
        }
    }

    /// Returns a human-readable summary of the message.
    pub fn summary(&self) -> &str {
        match self {
            Self::Custom { name, .. } => name.as_str(),
            Self::RequestPop => "request_pop",
            Self::RequestPush(_) => "request_push",
            Self::GameLaunched { .. } => "game_launched",
            Self::GameExited { .. } => "game_exited",
            Self::OverlayToggled { .. } => "overlay_toggled",
            Self::SettingsChanged => "settings_changed",
            Self::StateSaved { .. } => "state_saved",
            Self::StateRestored { .. } => "state_restored",
            Self::Error { .. } => "error",
            Self::FocusChanged { .. } => "focus_changed",
            Self::Resized { .. } => "resized",
        }
    }
}
