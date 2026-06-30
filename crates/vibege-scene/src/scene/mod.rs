//! # Scene System
//!
//! The Scene System is the application-level navigation framework for VibeGE.
//! It manages a stack of independently lifecycle-controlled screens (scenes),
//! supports overlay, modal, persistent, and background scene types, provides
//! a typed message-passing system for decoupled communication, and offers
//! state persistence so scenes survive interruption cleanly.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                   SceneManager                    │
//! │  ┌──────────────┐  ┌────────────┐  ┌───────────┐ │
//! │  │   Main Stack │  │  Overlays  │  │ Persistent│ │
//! │  │ [Normal]     │  │  [Overlay] │  │ [Bg]      │ │
//! │  │ [Normal]     │  │  [Modal]   │  │ [Persist] │ │
//! │  └──────────────┘  └────────────┘  └───────────┘ │
//! │  SceneStateStore    ActionQueue    ErrorRecovery  │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! ## Lifecycle Ordering
//!
//! Each scene moves through a guaranteed lifecycle:
//!
//! ```text
//! Construct → on_create → on_enter → on_activate → (updates/renders)
//!                 ↓            ↓            ↓
//!          (on error)    (on error)   on_suspend → on_deactivate → on_exit → on_destroy
//!                                          ↓
//!                                     on_resume → on_activate
//! ```
//!
//! ## Scene Kinds
//!
//! | Kind        | Pauses Below? | Updates | Renders | Survives Stack Ops? |
//! |-------------|--------------|---------|---------|---------------------|
//! | Normal      | Yes          | Yes     | Yes     | No                  |
//! | Overlay     | No           | Yes     | Yes     | No                  |
//! | Modal       | Yes (input)  | Yes     | Yes     | No                  |
//! | Persistent  | No           | Yes     | Opt-in  | Yes                 |
//! | Background  | No           | Yes     | No      | Yes                 |

pub mod kind;
pub mod manager;
pub mod message;
pub mod state;

pub use kind::SceneKind;
pub use manager::SceneManager;
pub use message::SceneMessage;
pub use state::{SceneSnapshot, SceneStateStore};

use std::sync::Arc;

use vibege_asset::AssetManager;
use vibege_core::EventBus;
use vibege_suspension::SuspensionEngine;

/// Identifies a scene type for routing and state tracking.
///
/// Not all variants have implementations — some are reserved for future use.
/// - **Implemented**: Boot, FirstRun, Home, Library, Store, Settings, Game, Error
/// - **Future**: Splash, Downloads, Pause, Notification, Update
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SceneId {
    Boot,
    /// Reserved — launch splash / loading screen.
    Splash,
    FirstRun,
    Home,
    Library,
    Store,
    /// Reserved — download manager overlay.
    Downloads,
    Settings,
    Game,
    /// Reserved — pause menu overlay during gameplay.
    Pause,
    /// Reserved — toast notification overlay.
    Notification,
    /// Reserved — app update overlay.
    Update,
    Error,
}

/// Navigation action returned by lifecycle callbacks and processed by SceneManager.
pub enum SceneAction {
    /// Continue normally — no navigation change.
    Continue,

    /// Push a new Normal scene on top of the stack.
    Push(Box<dyn Scene>),

    /// Replace the top Normal scene with a new one.
    Replace(Box<dyn Scene>),

    /// Pop the top Normal scene and resume the one below.
    Pop,

    /// Pop all scenes down to the given stack depth (1-indexed).
    PopTo(usize),

    /// Pop all Normal scenes and push a new root.
    PopToRoot(Box<dyn Scene>),

    /// Exit the entire application.
    Exit,

    /// Push an Overlay scene (does not pause Normal stack).
    PushOverlay(Box<dyn Scene>),

    /// Push a Modal scene (blocks input to scenes below).
    PushModal(Box<dyn Scene>),

    /// Pop the top Overlay scene.
    PopOverlay,

    /// Pop the top Modal scene.
    PopModal,

    /// Push a Persistent scene (survives stack operations).
    PushPersistent(Box<dyn Scene>),

    /// Push a Background scene (update only, no render).
    PushBackground(Box<dyn Scene>),

    /// Broadcast a message to all active scenes.
    Broadcast(message::SceneMessage),

    /// Send a message to a specific scene by position in the stack.
    SendMessage {
        index: usize,
        msg: message::SceneMessage,
    },
}

/// Convenience result type for scene operations.
pub type SceneResult = Result<SceneAction, String>;

/// Shared context passed to every scene lifecycle method.
///
/// Holds references to engine services that scenes may need during
/// their lifecycle. All fields are read-only from the scene's perspective.
pub struct SceneContext {
    pub screen_width: u32,
    pub screen_height: u32,
    pub renderer: Arc<vibege_renderer::Renderer>,
    pub input: Arc<std::sync::Mutex<vibege_input::InputManager>>,
    pub config: Arc<vibege_config::ConfigHandle>,
    pub event_bus: Option<Arc<EventBus>>,
    pub audio: Option<Arc<vibege_audio::AudioSystem>>,
    pub assets: Arc<AssetManager>,
    pub suspension: Option<Arc<std::sync::Mutex<SuspensionEngine>>>,
}

impl SceneContext {
    /// Create a new scene context.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: u32,
        height: u32,
        renderer: Arc<vibege_renderer::Renderer>,
        input: Arc<std::sync::Mutex<vibege_input::InputManager>>,
        config: Arc<vibege_config::ConfigHandle>,
        event_bus: Option<Arc<EventBus>>,
        audio: Option<Arc<vibege_audio::AudioSystem>>,
        assets: Arc<AssetManager>,
        suspension: Option<Arc<std::sync::Mutex<SuspensionEngine>>>,
    ) -> Self {
        Self {
            screen_width: width,
            screen_height: height,
            renderer,
            input,
            config,
            event_bus,
            audio,
            assets,
            suspension,
        }
    }
}

/// The primary trait for all VibeGE scenes.
///
/// Every screen or panel in the application implements this trait.
/// The lifecycle is deterministic and managed entirely by `SceneManager`.
///
/// # Lifecycle Phases
///
/// 1. **Construction** — The scene is box-allocated. No resources yet.
/// 2. **`on_create`** — Allocate resources, load assets. Return `Err` to abort.
/// 3. **`on_enter`** — The scene is becoming visible. Set up transient state.
/// 4. **`on_activate`** — The scene is the active recipient of input/updates.
/// 5. **`on_update` / `on_render`** — Per-frame update and draw.
/// 6. **`on_suspend`** — Another scene is covering this one. Save transient state.
/// 7. **`on_resume`** — This scene is being uncovered. Restore transient state.
/// 8. **`on_deactivate`** — The scene is losing active status.
/// 9. **`on_exit`** — The scene is about to be removed. Release transient resources.
/// 10. **`on_destroy`** — Final cleanup. Release all remaining resources. No fallible.
///
/// # State Persistence
///
/// Implement `save_state` / `restore_state` to survive interruption.
/// The SceneManager calls `save_state` before `on_suspend` and
/// `restore_state` after `on_resume`.
pub trait Scene {
    /// Unique identifier for this scene type.
    fn id(&self) -> SceneId;

    /// The scene's role in the stack.
    fn kind(&self) -> SceneKind {
        SceneKind::Normal
    }

    // ── Lifecycle ─────────────────────────────────────────────────

    /// Called once when the scene is first created.
    /// Allocate heavyweight resources here (textures, audio, Lua VMs).
    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    /// Called when the scene becomes visible (pushed onto stack, or resumed).
    fn on_enter(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    /// Called when this scene becomes the active (topmost) scene.
    /// It now receives input and update priority.
    fn on_activate(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    /// Called when this scene is no longer the active scene
    /// (a scene was pushed on top, or it was popped).
    fn on_deactivate(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    /// Called when a scene above this one is popped, and this scene
    /// becomes visible again.
    fn on_resume(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    /// Called when a scene is pushed on top, covering this one.
    fn on_suspend(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    /// Called just before the scene is removed from the stack.
    fn on_exit(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    /// Called after `on_exit` to release remaining resources.
    /// Unlike other lifecycle methods, this is infallible.
    fn on_destroy(&mut self, _ctx: &mut SceneContext) {}

    // ── Per-frame ─────────────────────────────────────────────────

    /// Called once per frame with the delta time in seconds.
    fn on_update(&mut self, _ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    /// Called once per frame to render the scene.
    fn on_render(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    // ── Message Handling ─────────────────────────────────────────

    /// Called when a message is routed to this scene.
    fn on_message(&mut self, _ctx: &mut SceneContext, _msg: &message::SceneMessage) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    // ── State Persistence ─────────────────────────────────────────

    /// Serialize the scene's current state for later restoration.
    /// Return `None` if this scene has no savable state.
    fn save_state(&self) -> Option<String> {
        None
    }

    /// Restore a previously saved state.
    fn restore_state(&mut self, _data: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
