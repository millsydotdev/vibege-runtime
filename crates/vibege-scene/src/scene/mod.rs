//! Scene graph types for the VibeGE platform.

use std::sync::Arc;

pub mod manager;

use vibege_core::EventBus;

pub use manager::SceneManager;

/// Identifies a scene type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SceneId {
    Boot,
    Splash,
    FirstRun,
    Home,
    Library,
    Store,
    Downloads,
    Settings,
    Game,
    Pause,
    Notification,
    Update,
}

/// Navigation actions a scene can return from lifecycle methods.
pub enum SceneAction {
    Continue,
    Push(Box<dyn Scene>),
    Replace(Box<dyn Scene>),
    Pop,
    PopToRoot(Box<dyn Scene>),
    Exit,
}

pub type SceneResult = Result<SceneAction, String>;

/// Context passed to every scene lifecycle method.
pub struct SceneContext {
    pub screen_width: u32,
    pub screen_height: u32,

    /// Shared engine services.
    pub renderer: Arc<vibege_renderer::Renderer>,
    pub input: Arc<std::sync::Mutex<vibege_input::InputManager>>,
    pub config: Arc<vibege_config::ConfigHandle>,
    /// Event bus for inter-subsystem communication.
    pub event_bus: Option<Arc<EventBus>>,
}

impl SceneContext {
    pub fn new(
        width: u32, height: u32,
        renderer: Arc<vibege_renderer::Renderer>,
        input: Arc<std::sync::Mutex<vibege_input::InputManager>>,
        config: Arc<vibege_config::ConfigHandle>,
        event_bus: Option<Arc<EventBus>>,
    ) -> Self {
        Self { screen_width: width, screen_height: height, renderer, input, config, event_bus }
    }
}

/// Lifecycle for a single platform scene.
///
/// Scenes run on the main thread. The event loop closure is FnMut + 'static,
/// NOT Send — so scenes can hold non-Send types like Rc<Lua>.
pub trait Scene {
    fn id(&self) -> SceneId;

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    fn on_enter(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    fn on_suspend(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    fn on_resume(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, _ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    fn on_exit(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        Ok(SceneAction::Continue)
    }

    fn on_destroy(&mut self, _ctx: &mut SceneContext) {}
}
