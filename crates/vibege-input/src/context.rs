//! Input contexts — stack-based action routing.
//!
//! Contexts allow different action mappings depending on the current game
//! state. For example, pressing "Escape" in a Gameplay context opens the
//! pause menu, but in a Menu context it goes back.
//!
//! # Stack Semantics
//!
//! Contexts are arranged in a stack. The topmost context that has a
//! binding for a given action wins. If no context has the action, it
//! falls through to a default (Idle).

use crate::action::ActionMap;
use crate::{ButtonState, InputManager};

/// A named input context containing an action mapping.
#[derive(Debug, Clone)]
pub struct InputContext {
    name: String,
    map: ActionMap,
}

impl InputContext {
    /// Create a new input context.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            map: ActionMap::new(),
        }
    }

    /// The context name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Mutable access to the action map.
    pub fn map_mut(&mut self) -> &mut ActionMap {
        &mut self.map
    }

    /// Immutable access to the action map.
    pub fn map(&self) -> &ActionMap {
        &self.map
    }
}

/// A stack of input contexts.
///
/// # Example
///
/// ```ignore
/// let mut stack = ContextStack::new();
/// stack.push(InputContext::new("gameplay"));
/// stack.push(InputContext::new("menu"));
///
/// // Menu bindings take priority over gameplay for overlapping actions
/// let state = stack.action_state(&input, "escape");
/// ```
#[derive(Debug, Clone)]
pub struct ContextStack {
    contexts: Vec<InputContext>,
}

impl ContextStack {
    /// Create an empty context stack.
    pub fn new() -> Self {
        Self {
            contexts: Vec::new(),
        }
    }

    /// Push a context onto the stack. It becomes the active context.
    pub fn push(&mut self, ctx: InputContext) {
        self.contexts.push(ctx);
    }

    /// Pop the top context. Returns `None` if the stack is empty.
    pub fn pop(&mut self) -> Option<InputContext> {
        self.contexts.pop()
    }

    /// Remove all contexts.
    pub fn clear(&mut self) {
        self.contexts.clear();
    }

    /// Number of contexts on the stack.
    pub fn depth(&self) -> usize {
        self.contexts.len()
    }

    /// Get the top context by name. Returns `None` if not found.
    pub fn find(&self, name: &str) -> Option<&InputContext> {
        self.contexts.iter().find(|c| c.name == name)
    }

    /// Get the top context by name (mutable).
    pub fn find_mut(&mut self, name: &str) -> Option<&mut InputContext> {
        self.contexts.iter_mut().find(|c| c.name == name)
    }

    /// True if the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }

    // ── Action resolution ─────────────────────────────────────────

    /// Resolve an action name using the context stack.
    /// Searches from top to bottom. The first context that has a binding
    /// for this action wins — even if its binding is currently idle.
    pub fn action_state(&self, input: &InputManager, name: &str) -> ButtonState {
        for ctx in self.contexts.iter().rev() {
            if ctx.map.has_action(name) {
                return ctx.map.action_state(input, name);
            }
        }
        ButtonState::Idle
    }

    /// Check if an action is active in any context (top-down).
    pub fn is_action_active(&self, input: &InputManager, name: &str) -> bool {
        let s = self.action_state(input, name);
        s == ButtonState::Pressed || s == ButtonState::Held
    }

    /// Resolve an axis using the context stack. Top-down.
    /// The first context that has a binding for this axis wins — even if its value is zero.
    pub fn axis_value(&self, input: &InputManager, name: &str) -> f32 {
        for ctx in self.contexts.iter().rev() {
            if ctx.map.has_axis(name) {
                return ctx.map.axis_value(input, name);
            }
        }
        0.0
    }
}

impl Default for ContextStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{ActionInput, AxisBinding, AxisSource};
    use winit::keyboard::KeyCode;

    fn make_input() -> InputManager {
        InputManager::new()
    }

    #[test]
    fn test_context_stack_empty() {
        let stack = ContextStack::new();
        let input = make_input();
        assert_eq!(stack.action_state(&input, "anything"), ButtonState::Idle);
    }

    #[test]
    fn test_context_push_pop() {
        let mut stack = ContextStack::new();
        assert_eq!(stack.depth(), 0);

        stack.push(InputContext::new("gameplay"));
        assert_eq!(stack.depth(), 1);

        let popped = stack.pop();
        assert!(popped.is_some());
        assert_eq!(popped.unwrap().name(), "gameplay");
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_context_top_priority() {
        let mut stack = ContextStack::new();
        let mut gameplay = InputContext::new("gameplay");
        gameplay
            .map_mut()
            .bind_action("escape", ActionInput::Key(KeyCode::Escape));
        stack.push(gameplay);

        // Escape is active in gameplay
        let mut es = make_input();
        es.set_key_state(KeyCode::Escape, ButtonState::Pressed);
        assert!(stack.is_action_active(&es, "escape"));

        // Push menu context that overrides escape
        let menu = InputContext::new("menu");
        // Menu doesn't bind escape, so it falls through
        stack.push(menu);
        assert!(stack.is_action_active(&es, "escape")); // still falls through
    }

    #[test]
    fn test_context_override() {
        let mut stack = ContextStack::new();

        let mut gameplay = InputContext::new("gameplay");
        gameplay
            .map_mut()
            .bind_action("action", ActionInput::Key(KeyCode::Space));
        stack.push(gameplay);

        let mut menu = InputContext::new("menu");
        menu.map_mut()
            .bind_action("action", ActionInput::Key(KeyCode::Enter));
        stack.push(menu);

        // Menu is top → Enter triggers action, Space does not
        let mut input = make_input();
        input.set_key_state(KeyCode::Space, ButtonState::Held);
        assert!(!stack.is_action_active(&input, "action"));

        input.end_frame();
        input.end_frame();
        input.set_key_state(KeyCode::Enter, ButtonState::Held);
        assert!(stack.is_action_active(&input, "action"));
    }

    #[test]
    fn test_context_find() {
        let mut stack = ContextStack::new();
        stack.push(InputContext::new("gameplay"));
        stack.push(InputContext::new("menu"));

        assert!(stack.find("gameplay").is_some());
        assert!(stack.find("menu").is_some());
        assert!(stack.find("nonexistent").is_none());
    }

    #[test]
    fn test_context_clear() {
        let mut stack = ContextStack::new();
        stack.push(InputContext::new("a"));
        stack.push(InputContext::new("b"));
        assert_eq!(stack.depth(), 2);
        stack.clear();
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_context_axis_fallthrough() {
        let mut stack = ContextStack::new();
        let mut gameplay = InputContext::new("gameplay");
        gameplay.map_mut().bind_axis(
            "move_x",
            AxisBinding::new(AxisSource::DigitalAxis {
                negative: KeyCode::KeyA,
                positive: KeyCode::KeyD,
            }),
        );
        stack.push(gameplay);

        let mut input = make_input();
        input.set_key_state(KeyCode::KeyD, ButtonState::Held);
        let val = stack.axis_value(&input, "move_x");
        assert!((val - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_context_pop_on_empty() {
        let mut stack = ContextStack::new();
        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_context_is_empty() {
        let mut stack = ContextStack::new();
        assert!(stack.is_empty());
        stack.push(InputContext::new("test"));
        assert!(!stack.is_empty());
    }
}
