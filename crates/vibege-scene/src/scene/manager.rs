use std::collections::VecDeque;

use super::kind::SceneKind;
use super::message::SceneMessage;
use super::state::{SceneSnapshot, SceneStateStore};
use super::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::{info, warn};

struct SceneNode {
    scene: Box<dyn Scene>,
}

impl SceneNode {
    fn new(scene: Box<dyn Scene>) -> Self {
        Self { scene }
    }

    fn id(&self) -> SceneId {
        self.scene.id()
    }

    fn kind(&self) -> SceneKind {
        self.scene.kind()
    }
}

/// Manages the lifecycle and navigation of all active scenes.
pub struct SceneManager {
    stack: Vec<SceneNode>,
    overlays: Vec<SceneNode>,
    persistent: Vec<SceneNode>,
    pending: VecDeque<SceneAction>,
    state_store: SceneStateStore,
    error_fallback: Option<Box<dyn Scene>>,
    frame: u64,
}

impl SceneManager {
    /// Create a new empty SceneManager.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            overlays: Vec::new(),
            persistent: Vec::new(),
            pending: VecDeque::new(),
            state_store: SceneStateStore::new(),
            error_fallback: None,
            frame: 0,
        }
    }

    /// Register a scene to show when another scene fails unexpectedly.
    pub fn set_error_fallback(&mut self, scene: Box<dyn Scene>) {
        self.error_fallback = Some(scene);
    }

    /// The current frame count (incremented each update).
    pub fn frame(&self) -> u64 {
        self.frame
    }

    // ─── Normal Stack Operations ─────────────────────────────────

    /// Push a Normal scene onto the main stack.
    pub fn push(&mut self, scene: Box<dyn Scene>, ctx: &mut SceneContext) {
        if let Some(top) = self.stack.last_mut() {
            call_scene(
                top.scene.as_mut(),
                |s, c| s.on_deactivate(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            save_scene_state(top.scene.as_mut(), &mut self.state_store, self.frame);
            call_scene(
                top.scene.as_mut(),
                |s, c| s.on_suspend(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
        }

        let mut node = SceneNode::new(scene);
        let id = node.id();
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_create(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_enter(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_activate(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );

        info!(?id, "Scene pushed");
        self.stack.push(node);
    }

    /// Pop the top Normal scene from the stack.
    pub fn pop(&mut self, ctx: &mut SceneContext) {
        if let Some(mut node) = self.stack.pop() {
            let id = node.id();
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_deactivate(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            save_scene_state(node.scene.as_mut(), &mut self.state_store, self.frame);
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_exit(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            node.scene.on_destroy(ctx);
            info!(?id, "Scene popped");

            broadcast_scenes(
                &SceneMessage::custom(&format!("{:?}_exited", id), ""),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
                &mut self.stack,
                &mut self.overlays,
                &mut self.persistent,
            );
        }

        if let Some(top) = self.stack.last_mut() {
            if let Some(data) = self.state_store.take(&top.id()) {
                let _ = top.scene.restore_state(&data.data);
            }
            call_scene(
                top.scene.as_mut(),
                |s, c| s.on_resume(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            call_scene(
                top.scene.as_mut(),
                |s, c| s.on_activate(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
        }
    }

    /// Replace the top Normal scene with a new one.
    pub fn replace(&mut self, scene: Box<dyn Scene>, ctx: &mut SceneContext) {
        if let Some(mut node) = self.stack.pop() {
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_deactivate(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_exit(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            node.scene.on_destroy(ctx);
        }

        let mut node = SceneNode::new(scene);
        let id = node.id();
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_create(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_enter(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_activate(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        info!(?id, "Scene replaced");
        self.stack.push(node);
    }

    /// Pop all scenes down to and including the root, then push a new root.
    pub fn pop_to_root(&mut self, scene: Box<dyn Scene>, ctx: &mut SceneContext) {
        while self.stack.len() > 1 {
            if let Some(mut node) = self.stack.pop() {
                call_scene(
                    node.scene.as_mut(),
                    |s, c| s.on_deactivate(c),
                    ctx,
                    &mut self.pending,
                    &mut self.error_fallback,
                );
                call_scene(
                    node.scene.as_mut(),
                    |s, c| s.on_exit(c),
                    ctx,
                    &mut self.pending,
                    &mut self.error_fallback,
                );
                node.scene.on_destroy(ctx);
            }
        }
        if let Some(mut node) = self.stack.pop() {
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_deactivate(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_exit(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            node.scene.on_destroy(ctx);
        }

        let mut node = SceneNode::new(scene);
        let id = node.id();
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_create(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_enter(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_activate(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        info!(?id, "Scene set as root");
        self.stack.push(node);
    }

    // ─── Overlay Operations ──────────────────────────────────────

    /// Push an Overlay scene on top of everything.
    pub fn push_overlay(&mut self, scene: Box<dyn Scene>, ctx: &mut SceneContext) {
        let mut node = SceneNode::new(scene);
        let id = node.id();
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_create(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_enter(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_activate(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        info!(?id, "Overlay pushed");
        self.overlays.push(node);
    }

    /// Pop the top Overlay scene.
    pub fn pop_overlay(&mut self, ctx: &mut SceneContext) {
        if let Some(mut node) = self.overlays.pop() {
            let id = node.id();
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_deactivate(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_exit(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            node.scene.on_destroy(ctx);
            info!(?id, "Overlay popped");
        }
    }

    // ─── Modal Operations ─────────────────────────────────────────

    /// Push a Modal scene on top of everything.
    pub fn push_modal(&mut self, scene: Box<dyn Scene>, ctx: &mut SceneContext) {
        let mut node = SceneNode::new(scene);
        let id = node.id();
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_create(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_enter(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_activate(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        info!(?id, "Modal pushed");
        self.overlays.push(node);
    }

    /// Pop the top Modal scene.
    pub fn pop_modal(&mut self, ctx: &mut SceneContext) {
        if let Some(mut node) = self.overlays.pop() {
            let id = node.id();
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_deactivate(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_exit(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            node.scene.on_destroy(ctx);
            info!(?id, "Modal popped");
        }
    }

    // ─── Persistent / Background Operations ──────────────────────

    /// Add a Persistent scene.
    pub fn push_persistent(&mut self, scene: Box<dyn Scene>, ctx: &mut SceneContext) {
        let mut node = SceneNode::new(scene);
        let id = node.id();
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_create(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_enter(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_activate(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        info!(?id, "Persistent scene added");
        self.persistent.push(node);
    }

    /// Remove a Persistent scene by its SceneId.
    pub fn remove_persistent(&mut self, id: &SceneId, ctx: &mut SceneContext) {
        if let Some(pos) = self.persistent.iter().position(|n| n.id() == *id) {
            let mut node = self.persistent.remove(pos);
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_deactivate(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_exit(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            node.scene.on_destroy(ctx);
            info!(?id, "Persistent scene removed");
        }
    }

    // ─── Per-frame Update & Render ───────────────────────────────

    /// Update all active scenes.
    pub fn update(&mut self, ctx: &mut SceneContext, dt: f64) -> SceneResult {
        self.frame += 1;

        for node in &mut self.persistent {
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_update(c, dt),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
        }

        if let Some(top) = self.stack.last_mut() {
            call_scene(
                top.scene.as_mut(),
                |s, c| s.on_update(c, dt),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
        }

        for node in self.overlays.iter_mut().rev() {
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_update(c, dt),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
        }

        Ok(SceneAction::Continue)
    }

    /// Render all visible scenes.
    pub fn render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        if let Some(top) = self.stack.last_mut() {
            call_scene(
                top.scene.as_mut(),
                |s, c| s.on_render(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
        }

        for node in &mut self.persistent {
            if node.kind() == SceneKind::Persistent {
                call_scene(
                    node.scene.as_mut(),
                    |s, c| s.on_render(c),
                    ctx,
                    &mut self.pending,
                    &mut self.error_fallback,
                );
            }
        }

        for node in &mut self.overlays {
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_render(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
        }

        Ok(SceneAction::Continue)
    }

    // ─── Navigation Action Processing ────────────────────────────

    /// Apply a navigation action immediately.
    pub fn apply(&mut self, action: SceneAction, ctx: &mut SceneContext) -> SceneResult {
        match action {
            SceneAction::Continue => {}
            SceneAction::Push(s) => self.push(s, ctx),
            SceneAction::Replace(s) => self.replace(s, ctx),
            SceneAction::Pop => self.pop(ctx),
            SceneAction::PopTo(depth) => self.pop_to_depth(depth, ctx),
            SceneAction::PopToRoot(s) => self.pop_to_root(s, ctx),
            SceneAction::Exit => {
                info!("Scene requested exit");
                self.shutdown_internal(ctx);
                return Ok(SceneAction::Exit);
            }
            SceneAction::PushOverlay(s) => self.push_overlay(s, ctx),
            SceneAction::PushModal(s) => self.push_modal(s, ctx),
            SceneAction::PopOverlay => self.pop_overlay(ctx),
            SceneAction::PopModal => self.pop_modal(ctx),
            SceneAction::PushPersistent(s) => self.push_persistent(s, ctx),
            SceneAction::PushBackground(s) => self.push_background_internal(s, ctx),
            SceneAction::Broadcast(msg) => {
                broadcast_scenes(
                    &msg,
                    ctx,
                    &mut self.pending,
                    &mut self.error_fallback,
                    &mut self.stack,
                    &mut self.overlays,
                    &mut self.persistent,
                );
            }
            SceneAction::SendMessage { index, msg } => {
                if let Some(node) = self.stack.get_mut(index) {
                    call_scene(
                        node.scene.as_mut(),
                        |s, c| s.on_message(c, &msg),
                        ctx,
                        &mut self.pending,
                        &mut self.error_fallback,
                    );
                }
            }
        }
        Ok(SceneAction::Continue)
    }

    /// Process any queued navigation actions from lifecycle callbacks.
    pub fn process_pending(&mut self, ctx: &mut SceneContext) -> SceneResult {
        while let Some(action) = self.pending.pop_front() {
            self.apply(action, ctx)?;
        }
        Ok(SceneAction::Continue)
    }

    // ─── Queries ──────────────────────────────────────────────────

    /// Returns a reference to the topmost active scene.
    pub fn active(&self) -> Option<&dyn Scene> {
        if let Some(top) = self.overlays.last() {
            return Some(top.scene.as_ref());
        }
        self.stack.last().map(|n| n.scene.as_ref())
    }

    /// Returns `true` if there are no scenes at all.
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty() && self.overlays.is_empty()
    }

    /// Depth of the main navigation stack.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Number of overlay/modal scenes.
    pub fn overlay_count(&self) -> usize {
        self.overlays.len()
    }

    /// Number of persistent scenes.
    pub fn persistent_count(&self) -> usize {
        self.persistent.len()
    }

    /// Broadcast a message to all active scenes.
    pub fn broadcast(&mut self, msg: &SceneMessage, ctx: &mut SceneContext) {
        broadcast_scenes(
            msg,
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
            &mut self.stack,
            &mut self.overlays,
            &mut self.persistent,
        );
    }

    /// Send a message to a specific scene by stack index (0 = root).
    pub fn send_to(&mut self, index: usize, msg: &SceneMessage, ctx: &mut SceneContext) {
        if let Some(node) = self.stack.get_mut(index) {
            call_scene(
                node.scene.as_mut(),
                |s, c| s.on_message(c, msg),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
        }
    }

    /// Check if there is a modal scene blocking input.
    pub fn has_modal(&self) -> bool {
        self.overlays
            .last()
            .is_some_and(|n| n.kind() == SceneKind::Modal)
    }

    // ─── Shutdown ─────────────────────────────────────────────────

    /// Perform a clean shutdown of all scenes.
    pub fn shutdown(&mut self, ctx: &mut SceneContext) {
        self.shutdown_internal(ctx);
    }

    fn shutdown_internal(&mut self, ctx: &mut SceneContext) {
        for mut node in self.overlays.drain(..).rev() {
            node.scene.on_destroy(ctx);
        }
        for mut node in self.stack.drain(..).rev() {
            node.scene.on_destroy(ctx);
        }
        for mut node in self.persistent.drain(..) {
            node.scene.on_destroy(ctx);
        }
        self.state_store.clear();
    }

    // ─── Helpers ──────────────────────────────────────────────────

    fn push_background_internal(&mut self, scene: Box<dyn Scene>, ctx: &mut SceneContext) {
        let mut node = SceneNode::new(scene);
        let id = node.id();
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_create(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_enter(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_activate(c),
            ctx,
            &mut self.pending,
            &mut self.error_fallback,
        );
        info!(?id, "Background scene added");
        self.persistent.push(node);
    }

    fn pop_to_depth(&mut self, depth: usize, ctx: &mut SceneContext) {
        while self.stack.len() > depth {
            if let Some(mut node) = self.stack.pop() {
                let id = node.id();
                call_scene(
                    node.scene.as_mut(),
                    |s, c| s.on_deactivate(c),
                    ctx,
                    &mut self.pending,
                    &mut self.error_fallback,
                );
                call_scene(
                    node.scene.as_mut(),
                    |s, c| s.on_exit(c),
                    ctx,
                    &mut self.pending,
                    &mut self.error_fallback,
                );
                node.scene.on_destroy(ctx);
                broadcast_scenes(
                    &SceneMessage::custom(&format!("{:?}_exited", id), ""),
                    ctx,
                    &mut self.pending,
                    &mut self.error_fallback,
                    &mut self.stack,
                    &mut self.overlays,
                    &mut self.persistent,
                );
            }
        }
        if let Some(top) = self.stack.last_mut() {
            if let Some(data) = self.state_store.take(&top.id()) {
                let _ = top.scene.restore_state(&data.data);
            }
            call_scene(
                top.scene.as_mut(),
                |s, c| s.on_resume(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
            call_scene(
                top.scene.as_mut(),
                |s, c| s.on_activate(c),
                ctx,
                &mut self.pending,
                &mut self.error_fallback,
            );
        }
    }
}

impl Default for SceneManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Free-standing helpers (avoid self-borrow conflicts) ──────────────

/// Call a lifecycle method on a scene, queuing navigation actions or handling errors.
#[allow(clippy::too_many_arguments)]
fn call_scene<F>(
    scene: &mut dyn Scene,
    f: F,
    ctx: &mut SceneContext,
    pending: &mut VecDeque<SceneAction>,
    error_fallback: &mut Option<Box<dyn Scene>>,
) where
    F: FnOnce(&mut dyn Scene, &mut SceneContext) -> SceneResult,
{
    match f(scene, ctx) {
        Ok(SceneAction::Continue) => {}
        Ok(action) => {
            pending.push_back(action);
        }
        Err(e) => {
            let id = scene.id();
            warn!(?id, error = %e, "Scene lifecycle error");
            if let Some(fallback) = error_fallback.take() {
                let mut node = SceneNode::new(fallback);
                let fid = node.id();
                let _ = node.scene.on_create(ctx);
                let _ = node.scene.on_enter(ctx);
                let _ = node.scene.on_activate(ctx);
                info!(?fid, "Error fallback displayed");
                // We can't push to overlays here without access to self.
                // Instead, queue a PushModal action.
                pending.push_back(SceneAction::PushModal(node.scene));
            } else {
                pending.push_back(SceneAction::Exit);
            }
        }
    }
}

/// Save scene state for persistence.
fn save_scene_state(scene: &mut dyn Scene, state_store: &mut SceneStateStore, frame: u64) {
    if let Some(data) = scene.save_state() {
        let snapshot = SceneSnapshot::new(scene.id(), data.clone(), frame);
        state_store.store(snapshot);
    }
}

/// Broadcast a message to all active scenes.
#[allow(clippy::too_many_arguments)]
fn broadcast_scenes(
    msg: &SceneMessage,
    ctx: &mut SceneContext,
    pending: &mut VecDeque<SceneAction>,
    error_fallback: &mut Option<Box<dyn Scene>>,
    stack: &mut [SceneNode],
    overlays: &mut [SceneNode],
    persistent: &mut [SceneNode],
) {
    for node in stack.iter_mut() {
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_message(c, msg),
            ctx,
            pending,
            error_fallback,
        );
    }
    for node in overlays.iter_mut() {
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_message(c, msg),
            ctx,
            pending,
            error_fallback,
        );
    }
    for node in persistent.iter_mut() {
        call_scene(
            node.scene.as_mut(),
            |s, c| s.on_message(c, msg),
            ctx,
            pending,
            error_fallback,
        );
    }
}
