use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::info;

/// Six settings categories accessible from the overlay.
const CATEGORIES: &[&str] = &[
    "Hotkey",
    "Position",
    "Startup",
    "Performance",
    "Audio",
    "Theme",
];

pub struct SettingsScene {
    tab: usize,
    hotkey_mod: String,
    hotkey_key: String,
    position: String,
    startup: String,
    perf: String,
    volume: f32,
    theme: String,
    dirty: bool,
}

impl SettingsScene {
    pub fn new() -> Self {
        Self {
            tab: 0,
            hotkey_mod: String::new(),
            hotkey_key: String::new(),
            position: String::new(),
            startup: String::new(),
            perf: String::new(),
            volume: 0.7,
            theme: String::new(),
            dirty: false,
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

    fn save(&mut self, ctx: &mut SceneContext) {
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
            },
        });
        self.dirty = false;
        info!("Settings saved");
    }

    fn cycle_str(current: &str, options: &[&str], dir: i32) -> String {
        let idx = options.iter().position(|o| *o == current);
        let next = match idx {
            Some(i) => (i as i32 + dir).rem_euclid(options.len() as i32) as usize,
            None => 0,
        };
        options[next].to_string()
    }
}

impl Scene for SettingsScene {
    fn id(&self) -> SceneId {
        SceneId::Settings
    }

    fn on_create(&mut self, ctx: &mut SceneContext) -> SceneResult {
        let c = ctx.config.get();
        self.hotkey_mod = c.overlay.hotkey_modifiers;
        self.hotkey_key = c.overlay.hotkey_key;
        self.position = c.overlay.position;
        self.startup = c.general.startup_behavior;
        self.perf = c.general.performance_mode;
        self.volume = c.audio.volume;
        self.theme = "dark".into();
        info!("SettingsScene: loaded");
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
        let esc = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("escape"));

        if esc {
            if self.dirty {
                self.save(ctx);
            }
            return Ok(SceneAction::Pop);
        }

        if up {
            self.tab = (self.tab + CATEGORIES.len() - 1) % CATEGORIES.len();
        }
        if down {
            self.tab = (self.tab + 1) % CATEGORIES.len();
        }

        match self.tab {
            0 => {
                // Hotkey
                if left {
                    self.hotkey_key = Self::cycle_str(
                        &self.hotkey_key,
                        &["v", "g", "b", "h", "space", "tab"],
                        -1,
                    );
                    self.dirty = true;
                }
                if right {
                    self.hotkey_key =
                        Self::cycle_str(&self.hotkey_key, &["v", "g", "b", "h", "space", "tab"], 1);
                    self.dirty = true;
                }
                if up {
                    self.hotkey_mod = Self::cycle_str(
                        &self.hotkey_mod,
                        &["ctrl+shift", "ctrl+alt", "alt+shift"],
                        -1,
                    );
                    self.dirty = true;
                }
                if down {
                    self.hotkey_mod = Self::cycle_str(
                        &self.hotkey_mod,
                        &["ctrl+shift", "ctrl+alt", "alt+shift"],
                        1,
                    );
                    self.dirty = true;
                }
            }
            1 => {
                // Position
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
                    self.dirty = true;
                }
            }
            2 => {
                // Startup
                if up || down {
                    self.startup = Self::cycle_str(
                        &self.startup,
                        &["hidden", "shown", "minimized"],
                        if up { -1 } else { 1 },
                    );
                    self.dirty = true;
                }
            }
            3 => {
                // Performance
                if up || down {
                    self.perf = Self::cycle_str(
                        &self.perf,
                        &["battery", "balanced", "performance"],
                        if up { -1 } else { 1 },
                    );
                    self.dirty = true;
                }
            }
            4 => {
                // Audio
                if left {
                    self.volume = (self.volume - 0.1).clamp(0.0, 1.0);
                    self.dirty = true;
                }
                if right {
                    self.volume = (self.volume + 0.1).clamp(0.0, 1.0);
                    self.dirty = true;
                }
            }
            5 => {
                // Theme
                if up || down {
                    self.theme = Self::cycle_str(
                        &self.theme,
                        &["dark", "light", "system"],
                        if up { -1 } else { 1 },
                    );
                }
            }
            _ => {}
        }

        if enter && self.tab == 0 {
            self.dirty = true;
        }

        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        self.clear(ctx);

        // Title
        self.rect(ctx, 30.0, 0.0, 740.0, 44.0, 0.48, 0.23, 0.93, 1.0);
        self.text(ctx, 42.0, 12.0, "Settings", 14.0, 1.0, 1.0, 1.0);
        if self.dirty {
            self.text(ctx, 680.0, 14.0, "* unsaved", 8.0, 0.9, 0.7, 0.2);
        }

        // Category tabs
        for (i, cat) in CATEGORIES.iter().enumerate() {
            let x = 30.0 + i as f32 * 125.0;
            if self.tab == i {
                self.rect(ctx, x, 52.0, 118.0, 26.0, 0.48, 0.23, 0.93, 1.0);
            } else {
                self.rect(ctx, x, 52.0, 118.0, 26.0, 0.10, 0.10, 0.22, 1.0);
            }
            self.text(ctx, x + 10.0, 57.0, cat, 8.0, 1.0, 1.0, 1.0);
        }

        // Content area
        self.rect(ctx, 30.0, 88.0, 740.0, 250.0, 0.10, 0.10, 0.22, 0.5);

        match self.tab {
            0 => {
                self.center_text(ctx, 110.0, "Overlay Hotkey", 12.0, 1.0, 1.0, 1.0);
                self.center_text(
                    ctx,
                    140.0,
                    &format!("{} + {}", self.hotkey_mod, self.hotkey_key),
                    18.0,
                    0.48,
                    0.23,
                    0.93,
                );
                self.center_text(
                    ctx,
                    170.0,
                    "Up/Down: modifier    Left/Right: key",
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
            }
            1 => {
                self.center_text(ctx, 110.0, "Overlay Position", 12.0, 1.0, 1.0, 1.0);
                self.center_text(ctx, 140.0, &self.position, 16.0, 0.48, 0.23, 0.93);
                self.center_text(ctx, 170.0, "Up/Down to change", 7.0, 0.5, 0.5, 0.6);
            }
            2 => {
                self.center_text(ctx, 110.0, "Startup Behaviour", 12.0, 1.0, 1.0, 1.0);
                self.center_text(ctx, 140.0, &self.startup, 16.0, 0.48, 0.23, 0.93);
                self.center_text(ctx, 170.0, "Up/Down to change", 7.0, 0.5, 0.5, 0.6);
            }
            3 => {
                self.center_text(ctx, 110.0, "Performance Profile", 12.0, 1.0, 1.0, 1.0);
                self.center_text(ctx, 140.0, &self.perf, 16.0, 0.48, 0.23, 0.93);
                self.center_text(ctx, 170.0, "Up/Down to change", 7.0, 0.5, 0.5, 0.6);
            }
            4 => {
                self.center_text(ctx, 110.0, "Audio Volume", 12.0, 1.0, 1.0, 1.0);
                self.center_text(
                    ctx,
                    140.0,
                    &format!("{:.0}%", self.volume * 100.0),
                    16.0,
                    0.48,
                    0.23,
                    0.93,
                );
                self.rect(ctx, 300.0, 165.0, 200.0, 6.0, 0.3, 0.3, 0.4, 1.0);
                self.rect(
                    ctx,
                    300.0,
                    165.0,
                    (self.volume * 200.0) as f32,
                    6.0,
                    0.48,
                    0.23,
                    0.93,
                    1.0,
                );
                self.center_text(ctx, 185.0, "Left/Right to adjust", 7.0, 0.5, 0.5, 0.6);
            }
            5 => {
                self.center_text(ctx, 110.0, "Theme", 12.0, 1.0, 1.0, 1.0);
                self.center_text(ctx, 140.0, &self.theme, 16.0, 0.48, 0.23, 0.93);
                self.center_text(ctx, 170.0, "Up/Down to change", 7.0, 0.5, 0.5, 0.6);
            }
            _ => {}
        }

        // Bottom bar
        self.rect(ctx, 30.0, 560.0, 740.0, 22.0, 0.10, 0.10, 0.22, 0.6);
        self.text(
            ctx,
            42.0,
            563.0,
            "Esc: Back & Save     Arrows: Navigate",
            7.0,
            0.5,
            0.5,
            0.6,
        );

        Ok(SceneAction::Continue)
    }
}
