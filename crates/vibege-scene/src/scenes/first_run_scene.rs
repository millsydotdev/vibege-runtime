use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::info;

const TOTAL_STEPS: u32 = 7;

pub struct FirstRunScene {
    step: u32,
    hotkey_mod: String,
    hotkey_key: String,
    position: String,
    startup: String,
    perf: String,
    volume: f32,
    theme: String,
}

impl FirstRunScene {
    pub fn new() -> Self {
        Self {
            step: 1,
            hotkey_mod: "ctrl+shift".into(),
            hotkey_key: "v".into(),
            position: "center".into(),
            startup: "hidden".into(),
            perf: "balanced".into(),
            volume: 0.7,
            theme: "dark".into(),
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
        sz: f32,
        r: f32,
        g: f32,
        b: f32,
    ) {
        ctx.renderer.draw_text(x, y, s, sz, r, g, b);
    }
    fn center_text(
        &self,
        ctx: &mut SceneContext,
        y: f32,
        s: &str,
        sz: f32,
        r: f32,
        g: f32,
        b: f32,
    ) {
        let w = s.len() as f32 * sz * 0.5;
        ctx.renderer.draw_text(400.0 - w, y, s, sz, r, g, b);
    }

    fn cycle_str(current: &str, options: &[&str], dir: i32) -> String {
        let idx = options.iter().position(|o| *o == current);
        let n = match idx {
            Some(i) => (i as i32 + dir).rem_euclid(options.len() as i32) as usize,
            None => 0,
        };
        options[n].to_string()
    }

    fn save(&self, ctx: &mut SceneContext) {
        ctx.config.set(vibege_config::VibegeConfig {
            overlay: vibege_config::OverlayConfig {
                hotkey_modifiers: self.hotkey_mod.clone(),
                hotkey_key: self.hotkey_key.clone(),
                position: self.position.clone(),
                width: 800,
                height: 600,
            },
            audio: vibege_config::AudioConfig {
                volume: self.volume,
            },
            general: vibege_config::GeneralConfig {
                startup_behavior: self.startup.clone(),
                performance_mode: self.perf.clone(),
                first_run_complete: true,
                backend_url: "http://localhost:3000/api/v1".into(),
            },
        });
    }

    fn draw_step_dots(&self, ctx: &mut SceneContext) {
        for i in 1..=TOTAL_STEPS {
            let x = 360.0 + (i - 1) as f32 * 14.0;
            if self.step == i {
                self.rect(ctx, x, 380.0, 10.0, 10.0, 0.48, 0.23, 0.93, 1.0);
            } else {
                self.rect(ctx, x, 380.0, 10.0, 10.0, 0.5, 0.5, 0.6, 0.3);
            }
        }
    }
}

impl Scene for FirstRunScene {
    fn id(&self) -> SceneId {
        SceneId::FirstRun
    }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        info!("FirstRunScene: 7-step wizard started");
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        let up = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("up"));
        let down = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("down"));
        let left = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("left"));
        let right = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("right"));
        let enter = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("enter"));

        match self.step {
            1 => {
                if enter {
                    self.step = 2;
                }
            }
            2 => {
                if up {
                    self.hotkey_mod = Self::cycle_str(
                        &self.hotkey_mod,
                        &["ctrl+shift", "ctrl+alt", "alt+shift"],
                        -1,
                    );
                }
                if down {
                    self.hotkey_mod = Self::cycle_str(
                        &self.hotkey_mod,
                        &["ctrl+shift", "ctrl+alt", "alt+shift"],
                        1,
                    );
                }
                if left {
                    self.hotkey_key = Self::cycle_str(
                        &self.hotkey_key,
                        &["v", "g", "b", "h", "space", "tab"],
                        -1,
                    );
                }
                if right {
                    self.hotkey_key =
                        Self::cycle_str(&self.hotkey_key, &["v", "g", "b", "h", "space", "tab"], 1);
                }
                if enter {
                    self.step = 3;
                }
            }
            3 => {
                if up || down {
                    self.position = Self::cycle_str(
                        &self.position,
                        &[
                            "center",
                            "top-left",
                            "top-right",
                            "bottom-left",
                            "bottom-right",
                        ],
                        if up { -1 } else { 1 },
                    );
                }
                if enter {
                    self.step = 4;
                }
            }
            4 => {
                if up || down {
                    self.startup = Self::cycle_str(
                        &self.startup,
                        &["hidden", "shown", "minimized"],
                        if up { -1 } else { 1 },
                    );
                }
                if enter {
                    self.step = 5;
                }
            }
            5 => {
                if up || down {
                    self.perf = Self::cycle_str(
                        &self.perf,
                        &["battery", "balanced", "performance"],
                        if up { -1 } else { 1 },
                    );
                }
                if enter {
                    self.step = 6;
                }
            }
            6 => {
                if left {
                    self.volume = (self.volume - 0.1).clamp(0.0, 1.0);
                }
                if right {
                    self.volume = (self.volume + 0.1).clamp(0.0, 1.0);
                }
                if enter {
                    self.step = 7;
                }
            }
            7 => {
                if enter {
                    self.save(ctx);
                    info!("FirstRun complete, transitioning to Home");
                    return Ok(SceneAction::Replace(Box::new(
                        super::home_scene::HomeScene::new(),
                    )));
                }
            }
            _ => {}
        }

        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        self.clear(ctx);
        self.rect(ctx, 0.0, 0.0, 800.0, 3.0, 0.48, 0.23, 0.93, 1.0);

        match self.step {
            1 => {
                self.center_text(ctx, 60.0, "Welcome to VibeGE!", 18.0, 1.0, 1.0, 1.0);
                self.center_text(
                    ctx,
                    90.0,
                    "The gaming overlay for AI-assisted development",
                    9.0,
                    0.5,
                    0.5,
                    0.6,
                );
                self.rect(ctx, 150.0, 130.0, 500.0, 1.0, 0.5, 0.5, 0.6, 0.3);
                self.center_text(
                    ctx,
                    170.0,
                    "Configure your overlay in the next few steps.",
                    8.0,
                    1.0,
                    1.0,
                    1.0,
                );
                self.center_text(
                    ctx,
                    200.0,
                    "You can change everything later in Settings.",
                    8.0,
                    0.5,
                    0.5,
                    0.6,
                );
                self.rect(ctx, 280.0, 270.0, 240.0, 40.0, 0.48, 0.23, 0.93, 1.0);
                self.center_text(ctx, 278.0, "  Get Started", 12.0, 1.0, 1.0, 1.0);
            }
            2 => {
                self.center_text(ctx, 60.0, "Overlay Hotkey", 14.0, 1.0, 1.0, 1.0);
                self.center_text(
                    ctx,
                    85.0,
                    "Press Up/Down to change modifier, Left/Right to change key",
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
                let hk = format!("{} + {}", self.hotkey_mod, self.hotkey_key);
                self.center_text(ctx, 160.0, &hk, 24.0, 0.48, 0.23, 0.93);
                self.center_text(ctx, 200.0, "Press Enter to continue", 7.0, 0.5, 0.5, 0.6);
            }
            3 => {
                self.center_text(ctx, 60.0, "Overlay Position", 14.0, 1.0, 1.0, 1.0);
                self.center_text(
                    ctx,
                    85.0,
                    "Where should the overlay appear?",
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
                for (i, p) in [
                    "center",
                    "top-left",
                    "top-right",
                    "bottom-left",
                    "bottom-right",
                ]
                .iter()
                .enumerate()
                {
                    let x = 250.0;
                    let y = 130.0 + i as f32 * 30.0;
                    if self.position == *p {
                        self.rect(ctx, x, y, 12.0, 12.0, 0.48, 0.23, 0.93, 1.0);
                        self.rect(ctx, x + 3.0, y + 3.0, 6.0, 6.0, 1.0, 1.0, 1.0, 1.0);
                    } else {
                        self.rect(ctx, x, y, 12.0, 12.0, 0.5, 0.5, 0.6, 0.3);
                    }
                    self.text(ctx, x + 18.0, y, p, 8.0, 1.0, 1.0, 1.0);
                }
                self.center_text(ctx, 320.0, "Press Enter to continue", 7.0, 0.5, 0.5, 0.6);
            }
            4 => {
                self.center_text(ctx, 60.0, "Startup Behaviour", 14.0, 1.0, 1.0, 1.0);
                self.center_text(
                    ctx,
                    85.0,
                    "What should happen when you start VibeGE?",
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
                for (i, s) in ["hidden", "shown", "minimized"].iter().enumerate() {
                    let y = 140.0 + i as f32 * 50.0;
                    if self.startup == *s {
                        self.rect(ctx, 250.0, y, 300.0, 40.0, 0.25, 0.15, 0.45, 1.0);
                    }
                    self.center_text(ctx, y + 10.0, s, 10.0, 1.0, 1.0, 1.0);
                    self.center_text(
                        ctx,
                        y + 25.0,
                        match *s {
                            "hidden" => "Start in background (tray only)",
                            "shown" => "Show the overlay window on start",
                            "minimized" => "Start minimized to taskbar",
                            _ => "",
                        },
                        7.0,
                        0.5,
                        0.5,
                        0.6,
                    );
                }
                self.center_text(ctx, 320.0, "Press Enter to continue", 7.0, 0.5, 0.5, 0.6);
            }
            5 => {
                self.center_text(ctx, 60.0, "Performance Profile", 14.0, 1.0, 1.0, 1.0);
                self.center_text(
                    ctx,
                    85.0,
                    "Balance performance vs. battery life",
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
                for (i, p) in ["battery", "balanced", "performance"].iter().enumerate() {
                    let y = 140.0 + i as f32 * 50.0;
                    if self.perf == *p {
                        self.rect(ctx, 250.0, y, 300.0, 40.0, 0.25, 0.15, 0.45, 1.0);
                    }
                    self.center_text(ctx, y + 10.0, p, 10.0, 1.0, 1.0, 1.0);
                    self.center_text(
                        ctx,
                        y + 25.0,
                        match *p {
                            "battery" => "Lower frame rate, longer battery",
                            "balanced" => "Standard frame rate and quality",
                            "performance" => "Maximum frame rate, higher power use",
                            _ => "",
                        },
                        7.0,
                        0.5,
                        0.5,
                        0.6,
                    );
                }
                self.center_text(ctx, 320.0, "Press Enter to continue", 7.0, 0.5, 0.5, 0.6);
            }
            6 => {
                self.center_text(ctx, 60.0, "Audio Volume", 14.0, 1.0, 1.0, 1.0);
                self.center_text(
                    ctx,
                    85.0,
                    "Set the default volume for game audio",
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
                self.center_text(
                    ctx,
                    150.0,
                    &format!("{:.0}%", self.volume * 100.0),
                    22.0,
                    0.48,
                    0.23,
                    0.93,
                );
                self.rect(ctx, 250.0, 180.0, 300.0, 8.0, 0.3, 0.3, 0.4, 1.0);
                self.rect(
                    ctx,
                    250.0,
                    180.0,
                    (self.volume * 300.0) as f32,
                    8.0,
                    0.48,
                    0.23,
                    0.93,
                    1.0,
                );
                self.center_text(
                    ctx,
                    210.0,
                    "Left/Right to adjust     Enter to continue",
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
            }
            7 => {
                self.center_text(ctx, 80.0, "You're all set!", 18.0, 1.0, 1.0, 1.0);
                self.center_text(
                    ctx,
                    115.0,
                    &format!("Hotkey: {} + {}", self.hotkey_mod, self.hotkey_key),
                    10.0,
                    0.48,
                    0.23,
                    0.93,
                );
                self.center_text(
                    ctx,
                    135.0,
                    &format!(
                        "Position: {}  |  Startup: {}  |  Mode: {}",
                        self.position, self.startup, self.perf
                    ),
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
                self.center_text(
                    ctx,
                    155.0,
                    &format!(
                        "Volume: {:.0}%  |  Theme: {}",
                        self.volume * 100.0,
                        self.theme
                    ),
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
                self.center_text(ctx, 210.0, "Press Enter to open VibeGE", 9.0, 0.5, 0.5, 0.6);
                self.rect(ctx, 280.0, 250.0, 240.0, 40.0, 0.48, 0.23, 0.93, 1.0);
                self.center_text(ctx, 258.0, "  Launch VibeGE", 12.0, 1.0, 1.0, 1.0);
            }
            _ => {}
        }

        self.draw_step_dots(ctx);
        Ok(SceneAction::Continue)
    }
}
