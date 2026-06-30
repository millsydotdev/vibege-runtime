use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::info;

/// Native Rust implementation of the first-run wizard.
/// Replaces the Lua first-run.lua — 3 steps: hotkey, position, ready.
pub struct FirstRunScene {
    step: u32,
    hotkey_mod: String,
    hotkey_key: String,
    overlay_pos: String,
    started: bool,
}

impl FirstRunScene {
    pub fn new() -> Self {
        Self {
            step: 1,
            hotkey_mod: "ctrl+shift".into(),
            hotkey_key: "v".into(),
            overlay_pos: "center".into(),
            started: false,
        }
    }

    fn clear(&self, ctx: &mut SceneContext) {
        ctx.renderer.set_clear(0.05, 0.05, 0.15, 1.0);
    }

    fn rect(
        &self,
        ctx: &mut SceneContext,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) {
        ctx.renderer.draw_rect(x, y, w, h, r, g, b, a);
    }

    fn text(
        &self,
        ctx: &mut SceneContext,
        x: f32,
        y: f32,
        s: &str,
        size: f32,
        r: f32,
        g: f32,
        b: f32,
    ) {
        ctx.renderer.draw_text(x, y, s, size, r, g, b);
    }

    fn center_text(
        &self,
        ctx: &mut SceneContext,
        y: f32,
        s: &str,
        size: f32,
        r: f32,
        g: f32,
        b: f32,
    ) {
        let w = s.len() as f32 * size * 0.5;
        ctx.renderer.draw_text(400.0 - w, y, s, size, r, g, b);
    }

    fn input_pressed(&self, ctx: &SceneContext, key: &str) -> bool {
        ctx.input
            .lock()
            .unwrap()
            .is_key_pressed(vibege_input::key_name_to_code(key))
    }

    fn draw_radio(&self, ctx: &mut SceneContext, x: f32, y: f32, label: &str, selected: bool) {
        if selected {
            self.rect(ctx, x, y, 12.0, 12.0, 0.48, 0.23, 0.93, 1.0);
            self.rect(ctx, x + 3.0, y + 3.0, 6.0, 6.0, 1.0, 1.0, 1.0, 1.0);
        } else {
            self.rect(ctx, x, y, 12.0, 12.0, 0.5, 0.5, 0.6, 0.3);
        }
        self.text(ctx, x + 18.0, y, label, 8.0, 1.0, 1.0, 1.0);
    }
}

impl Scene for FirstRunScene {
    fn id(&self) -> SceneId {
        SceneId::FirstRun
    }

    fn on_create(&mut self, ctx: &mut SceneContext) -> SceneResult {
        info!("FirstRunScene: started");
        let cfg = ctx.config.get();
        // Pre-fill from any existing config
        self.hotkey_mod = cfg.overlay.hotkey_modifiers;
        self.hotkey_key = cfg.overlay.hotkey_key;
        self.overlay_pos = cfg.overlay.position;
        self.started = true;
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        let up = self.input_pressed(ctx, "up");
        let down = self.input_pressed(ctx, "down");
        let left = self.input_pressed(ctx, "left");
        let right = self.input_pressed(ctx, "right");
        let enter = self.input_pressed(ctx, "enter");

        if self.step == 1 {
            if up {
                self.hotkey_mod = match self.hotkey_mod.as_str() {
                    "ctrl+shift" => "ctrl+alt",
                    "ctrl+alt" => "alt+shift",
                    _ => "ctrl+shift",
                }
                .into();
            }
            if down {
                self.hotkey_mod = match self.hotkey_mod.as_str() {
                    "ctrl+shift" => "alt+shift",
                    "alt+shift" => "ctrl+alt",
                    _ => "ctrl+shift",
                }
                .into();
            }
            if left {
                self.hotkey_key = match self.hotkey_key.as_str() {
                    "v" => "tab",
                    "tab" => "space",
                    "space" => "h",
                    "h" => "b",
                    "b" => "g",
                    _ => "v",
                }
                .into();
            }
            if right {
                self.hotkey_key = match self.hotkey_key.as_str() {
                    "v" => "g",
                    "g" => "b",
                    "b" => "h",
                    "h" => "space",
                    "space" => "tab",
                    _ => "v",
                }
                .into();
            }
            if enter {
                self.step = 2;
            }
        } else if self.step == 2 {
            let positions = [
                "center",
                "top-left",
                "top-right",
                "bottom-left",
                "bottom-right",
            ];
            if up {
                let idx = positions
                    .iter()
                    .position(|&p| *p == self.overlay_pos)
                    .unwrap_or(0);
                self.overlay_pos = positions[(idx + 1) % positions.len()].into();
            }
            if down {
                let idx = positions
                    .iter()
                    .position(|&p| *p == self.overlay_pos)
                    .unwrap_or(0);
                self.overlay_pos = positions[(idx + positions.len() - 1) % positions.len()].into();
            }
            if enter {
                self.step = 3;
            }
        } else if self.step == 3 {
            if enter {
                // Save settings
                ctx.config.set(vibege_config::VibegeConfig {
                    overlay: vibege_config::OverlayConfig {
                        hotkey_modifiers: self.hotkey_mod.clone(),
                        hotkey_key: self.hotkey_key.clone(),
                        position: self.overlay_pos.clone(),
                        width: 800,
                        height: 600,
                    },
                    audio: vibege_config::AudioConfig { volume: 0.7 },
                    general: vibege_config::GeneralConfig {
                        startup_behavior: "hidden".into(),
                        performance_mode: "balanced".into(),
                        first_run_complete: true,
                    },
                });
                info!("FirstRunScene: settings saved, transitioning to Home");
                return Ok(SceneAction::Replace(Box::new(
                    super::home_scene::HomeScene::new(),
                )));
            }
        }

        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        self.clear(ctx);
        // Accent bar
        self.rect(ctx, 0.0, 0.0, 800.0, 3.0, 0.48, 0.23, 0.93, 1.0);

        if self.step == 1 {
            self.center_text(ctx, 60.0, "Welcome to VibeGE!", 16.0, 1.0, 1.0, 1.0);
            self.center_text(
                ctx,
                90.0,
                "The gaming overlay for AI-assisted development",
                8.0,
                0.5,
                0.5,
                0.6,
            );
            self.rect(ctx, 150.0, 130.0, 500.0, 1.0, 0.5, 0.5, 0.6, 0.3);
            self.center_text(
                ctx,
                160.0,
                "Choose your overlay hotkey",
                10.0,
                1.0,
                1.0,
                1.0,
            );
            self.center_text(
                ctx,
                185.0,
                "Up/Down to change modifier, Left/Right to change key",
                7.0,
                0.5,
                0.5,
                0.6,
            );

            let hk = format!("{} + {}", self.hotkey_mod, self.hotkey_key);
            self.center_text(ctx, 230.0, &hk, 20.0, 0.48, 0.23, 0.93);
            self.center_text(ctx, 260.0, "Press Enter to continue", 7.0, 0.5, 0.5, 0.6);
        } else if self.step == 2 {
            self.center_text(ctx, 60.0, "Overlay Position", 14.0, 1.0, 1.0, 1.0);
            self.center_text(
                ctx,
                85.0,
                "Where should the overlay appear?",
                8.0,
                0.5,
                0.5,
                0.6,
            );

            for (i, pos) in [
                "center",
                "top-left",
                "top-right",
                "bottom-left",
                "bottom-right",
            ]
            .iter()
            .enumerate()
            {
                self.draw_radio(
                    ctx,
                    250.0,
                    130.0 + i as f32 * 30.0,
                    pos,
                    self.overlay_pos == *pos,
                );
            }
            self.center_text(ctx, 320.0, "Press Enter to continue", 7.0, 0.5, 0.5, 0.6);
        } else if self.step == 3 {
            self.center_text(ctx, 80.0, "You're all set!", 16.0, 1.0, 1.0, 1.0);
            let hk = format!("{} + {}", self.hotkey_mod, self.hotkey_key);
            self.center_text(ctx, 110.0, &hk, 10.0, 0.48, 0.23, 0.93);
            self.center_text(ctx, 135.0, &self.overlay_pos, 10.0, 0.48, 0.23, 0.93);
            self.center_text(
                ctx,
                200.0,
                "Press hotkey at any time to open the overlay",
                8.0,
                0.5,
                0.5,
                0.6,
            );
            self.center_text(
                ctx,
                220.0,
                "and play your installed games while AI works.",
                8.0,
                0.5,
                0.5,
                0.6,
            );
            self.rect(ctx, 250.0, 270.0, 300.0, 40.0, 0.48, 0.23, 0.93, 1.0);
            self.center_text(ctx, 278.0, "Get Started", 10.0, 1.0, 1.0, 1.0);
        }

        // Step indicators
        for i in 1..=3 {
            if self.step == i {
                self.rect(
                    ctx,
                    380.0 + (i - 1) as f32 * 20.0,
                    360.0,
                    12.0,
                    12.0,
                    0.48,
                    0.23,
                    0.93,
                    1.0,
                );
            } else {
                self.rect(
                    ctx,
                    380.0 + (i - 1) as f32 * 20.0,
                    360.0,
                    12.0,
                    12.0,
                    0.5,
                    0.5,
                    0.6,
                    0.3,
                );
            }
        }

        Ok(SceneAction::Continue)
    }
}
