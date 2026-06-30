//! Mouse state — position, buttons, scroll, double-click, drag, cursor control.

/// Represents a mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

impl From<winit::event::MouseButton> for MouseButton {
    fn from(b: winit::event::MouseButton) -> Self {
        match b {
            winit::event::MouseButton::Left => Self::Left,
            winit::event::MouseButton::Right => Self::Right,
            winit::event::MouseButton::Middle => Self::Middle,
            winit::event::MouseButton::Back => Self::Back,
            winit::event::MouseButton::Forward => Self::Forward,
            winit::event::MouseButton::Other(v) => Self::Other(v),
        }
    }
}

/// Accumulated mouse state for one frame.
#[derive(Debug, Clone)]
pub(crate) struct MouseState {
    pub position: (f64, f64),
    pub delta: (f64, f64),
    pub button_states: std::collections::HashMap<MouseButton, super::ButtonState>,
    pub scroll_delta: (f64, f64),

    // Double-click tracking
    pub double_click_timer: f64,
    pub double_click_threshold: f64, // seconds
    pub double_click: std::collections::HashMap<MouseButton, bool>,

    // Drag tracking
    pub drag_start: std::collections::HashMap<MouseButton, Option<(f64, f64)>>,
    pub is_dragging: std::collections::HashMap<MouseButton, bool>,

    // Cursor
    pub cursor_visible: bool,
    pub cursor_locked: bool,
}

impl MouseState {
    pub fn new() -> Self {
        Self {
            position: (0.0, 0.0),
            delta: (0.0, 0.0),
            button_states: std::collections::HashMap::new(),
            scroll_delta: (0.0, 0.0),
            double_click_timer: 0.0,
            double_click_threshold: 0.3,
            double_click: std::collections::HashMap::new(),
            drag_start: std::collections::HashMap::new(),
            is_dragging: std::collections::HashMap::new(),
            cursor_visible: true,
            cursor_locked: false,
        }
    }

    /// Advance frame timer. Call once per frame with dt.
    #[allow(dead_code)]
    pub fn tick(&mut self, dt: f64) {
        self.double_click_timer = (self.double_click_timer - dt).max(0.0);
        // Clear double-click flags
        self.double_click.clear();
    }

    /// Called when a mouse button is pressed.
    pub fn on_button_down(&mut self, btn: MouseButton) {
        // Double-click detection
        if self.double_click_timer > 0.0 {
            self.double_click.insert(btn, true);
            self.double_click_timer = 0.0;
        } else {
            self.double_click_timer = self.double_click_threshold;
        }

        // Drag start
        self.drag_start.insert(btn, Some(self.position));
        // is_dragging stays false until movement
    }

    /// Called when a mouse button is released.
    pub fn on_button_up(&mut self, btn: MouseButton) {
        if self.is_dragging.get(&btn).copied().unwrap_or(false) {
            // Drag ended
        }
        self.is_dragging.insert(btn, false);
        self.drag_start.insert(btn, None);
    }

    /// Called on cursor move — updates drag state.
    pub fn on_move(&mut self, new_pos: (f64, f64)) {
        self.delta = (new_pos.0 - self.position.0, new_pos.1 - self.position.1);
        self.position = new_pos;

        // Check for drag
        for (btn, start) in self.drag_start.iter() {
            if let Some((sx, sy)) = start {
                let dx = (self.position.0 - sx).abs();
                let dy = (self.position.1 - sy).abs();
                if dx > 3.0 || dy > 3.0 {
                    // Minimum drag threshold
                    self.is_dragging.insert(*btn, true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_state_new() {
        let ms = MouseState::new();
        assert_eq!(ms.position, (0.0, 0.0));
        assert_eq!(ms.delta, (0.0, 0.0));
        assert_eq!(ms.scroll_delta, (0.0, 0.0));
        assert!(ms.cursor_visible);
        assert!(!ms.cursor_locked);
    }

    #[test]
    fn test_double_click_detection() {
        let mut ms = MouseState::new();
        ms.on_button_down(MouseButton::Left);
        assert!(!ms.double_click.contains_key(&MouseButton::Left));

        // Fast second click
        ms.on_button_down(MouseButton::Left);
        assert!(ms.double_click.contains_key(&MouseButton::Left));
    }

    #[test]
    fn test_double_click_expires() {
        let mut ms = MouseState::new();
        ms.double_click_threshold = 0.1;
        ms.on_button_down(MouseButton::Left);

        // Tick past threshold
        ms.tick(0.2);

        // Second click after threshold is a new click, not double
        ms.on_button_down(MouseButton::Left);
        // Wait — the timer was reset by the press, so this is still valid
        // Actually, the timer was set to 0.1 on first press, after 0.2s tick it's 0
        // Second press: timer is 0, so it sets timer and does NOT detect double
        assert!(!ms.double_click.contains_key(&MouseButton::Left));
    }

    #[test]
    fn test_mouse_delta_on_move() {
        let mut ms = MouseState::new();
        ms.on_move((100.0, 50.0));
        assert_eq!(ms.position, (100.0, 50.0));
        assert_eq!(ms.delta, (100.0, 50.0));

        ms.on_move((120.0, 60.0));
        assert_eq!(ms.position, (120.0, 60.0));
        assert_eq!(ms.delta, (20.0, 10.0));
    }

    #[test]
    fn test_cursor_visibility() {
        let mut ms = MouseState::new();
        assert!(ms.cursor_visible);
        ms.cursor_visible = false;
        assert!(!ms.cursor_visible);
    }

    #[test]
    fn test_drag_start_on_down() {
        let mut ms = MouseState::new();
        ms.on_button_down(MouseButton::Left);
        assert_eq!(
            ms.drag_start.get(&MouseButton::Left),
            Some(&Some((0.0, 0.0)))
        );
    }

    #[test]
    fn test_drag_detection() {
        let mut ms = MouseState::new();
        ms.on_button_down(MouseButton::Left);
        assert!(
            !ms.is_dragging
                .get(&MouseButton::Left)
                .copied()
                .unwrap_or(false)
        );

        // Move past threshold
        ms.on_move((10.0, 0.0));
        assert!(
            ms.is_dragging
                .get(&MouseButton::Left)
                .copied()
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_drag_cleared_on_release() {
        let mut ms = MouseState::new();
        ms.on_button_down(MouseButton::Left);
        ms.on_move((10.0, 0.0));
        assert!(
            ms.is_dragging
                .get(&MouseButton::Left)
                .copied()
                .unwrap_or(false)
        );

        ms.on_button_up(MouseButton::Left);
        assert!(
            !ms.is_dragging
                .get(&MouseButton::Left)
                .copied()
                .unwrap_or(false)
        );
    }
}
