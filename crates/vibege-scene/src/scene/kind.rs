/// Classification of a scene's role in the stack.
///
/// Determines how the SceneManager handles lifecycle, rendering,
/// and input routing for each scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneKind {
    /// Standard scene in the main navigation stack.
    /// Suspended when a Normal scene is pushed on top.
    /// Receives update/render when it is the topmost Normal scene.
    Normal,

    /// Rendered on top of the Normal stack without pausing the scene below.
    /// Useful for HUD elements, notifications, tooltips.
    Overlay,

    /// Blocks input to all scenes below while rendered on top.
    /// Useful for confirm dialogs, error modals, required actions.
    Modal,

    /// Survives stack operations. Not destroyed on pop or replace.
    /// Runs update every frame. May render on top if desired.
    /// Useful for download managers, music players, background sync.
    Persistent,

    /// Runs update but does not render.
    /// Useful for download managers, auto-save, network requests.
    Background,
}
