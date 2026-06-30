use std::path::PathBuf;
use tracing::info;
use crate::scene::{Scene, SceneId, SceneContext, SceneAction, SceneResult};

/// A game entry from the library or demo list.
struct GameEntry {
    name: String,
    desc: String,
    author: String,
    /// "live" or "installed"
    status: String,
    /// Path to the entry file, or "demo" for the embedded demo.
    path: String,
}

impl GameEntry {
    fn demo(name: &str, desc: &str, author: &str) -> Self {
        Self { name: name.into(), desc: desc.into(), author: author.into(), status: "live".into(), path: "demo".into() }
    }
}

/// Native Rust HomeScene — game library browser.
/// Replaces the Lua launcher.lua entirely.
pub struct HomeScene {
    entries: Vec<GameEntry>,
    selection: usize,
    has_scanned: bool,
}

impl HomeScene {
    pub fn new() -> Self {
        Self { entries: Vec::new(), selection: 0, has_scanned: false }
    }

    fn clear(&self, ctx: &mut SceneContext) {
        ctx.renderer.set_clear(0.05, 0.05, 0.15, 1.0);
    }

    fn rect(&self, ctx: &mut SceneContext, x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, b: f32, a: f32) {
        ctx.renderer.draw_rect(x, y, w, h, r, g, b, a);
    }

    fn text(&self, ctx: &mut SceneContext, x: f32, y: f32, s: &str, sz: f32, r: f32, g: f32, b: f32) {
        ctx.renderer.draw_text(x, y, s, sz, r, g, b);
    }

    fn input_down(&self, ctx: &SceneContext, key: &str) -> bool {
        ctx.input.lock().unwrap().is_key_down(vibege_input::key_name_to_code(key))
    }

    fn input_pressed(&self, ctx: &SceneContext, key: &str) -> bool {
        ctx.input.lock().unwrap().is_key_pressed(vibege_input::key_name_to_code(key))
    }

    fn scan_installed_games(&mut self) {
        if self.has_scanned { return; }
        self.has_scanned = true;

        let dir = vibege_config::installed_games_dir();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() { continue; }
                let meta_path = path.join(".vibege-install.json");
                if !meta_path.exists() { continue; }
                if let Ok(content) = std::fs::read_to_string(&meta_path) {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                        let name = meta["name"].as_str().unwrap_or("").to_string();
                        let entry_f = meta["entry"].as_str().unwrap_or("src/main.lua");
                        if !name.is_empty() {
                            self.entries.push(GameEntry {
                                name,
                                desc: "Installed game".into(),
                                author: "Local".into(),
                                status: "installed".into(),
                                path: path.join(entry_f).to_string_lossy().to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Add demo entries if none found
        if self.entries.is_empty() {
            self.entries.push(GameEntry::demo("Pong", "Classic paddle arcade", "VibeGE"));
            self.entries.push(GameEntry::demo("Void Drifter", "Space exploration", "VibeGE Labs"));
        }
    }

    fn launch_selected(&self, ctx: &mut SceneContext) -> SceneResult {
        let game = &self.entries[self.selection];
        info!(game = %game.name, path = %game.path, "Launching game");

        if game.path == "demo" {
            // Load embedded demo game
            let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/demo-game.lua"));
            let game_scene = Box::new(super::game_scene::GameScene::new(
                source.to_string(),
                ctx.renderer.clone(),
                ctx.input.clone(),
                None, // audio, will be passed through context later
            ));
            return Ok(SceneAction::Push(game_scene));
        }

        // Load from file
        let path = PathBuf::from(&game.path);
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(source) => {
                    let game_scene = Box::new(super::game_scene::GameScene::new(
                        source, ctx.renderer.clone(), ctx.input.clone(), None,
                    ));
                    Ok(SceneAction::Push(game_scene))
                }
                Err(e) => {
                    info!("Failed to read game file: {e}");
                    Ok(SceneAction::Continue)
                }
            }
        } else {
            info!("Game file not found: {}", path.display());
            Ok(SceneAction::Continue)
        }
    }

    fn draw_card(&self, ctx: &mut SceneContext, x: f32, y: f32, w: f32, game: &GameEntry, selected: bool) {
        let card_h = 72.0;
        // Card background
        if selected {
            self.rect(ctx, x, y, w, card_h, 0.25, 0.15, 0.45, 1.0);
            self.rect(ctx, x, y, 3.0, card_h, 0.48, 0.23, 0.93, 1.0);
        } else {
            self.rect(ctx, x, y, w, card_h, 0.10, 0.10, 0.22, 1.0);
        }

        // Game name
        self.text(ctx, x + 16.0, y + 8.0, &game.name, 10.0, 1.0, 1.0, 1.0);
        // Description
        self.text(ctx, x + 16.0, y + 26.0, &game.desc, 8.0, 0.5, 0.5, 0.6);
        // Author
        let author = format!("by {}", game.author);
        self.text(ctx, x + 16.0, y + 42.0, &author, 7.0, 0.5, 0.5, 0.6);

        // Status badge
        let sx = x + w - 85.0;
        if game.status == "live" {
            self.rect(ctx, sx, y + 8.0, 70.0, 16.0, 0.2, 0.8, 0.4, 0.2);
            self.text(ctx, sx + 8.0, y + 10.0, "LIVE", 8.0, 0.2, 0.8, 0.4);
        } else {
            self.rect(ctx, sx, y + 8.0, 70.0, 16.0, 0.9, 0.7, 0.2, 0.2);
            self.text(ctx, sx + 8.0, y + 10.0, "INSTALLED", 7.0, 0.9, 0.7, 0.2);
        }
    }
}

impl Scene for HomeScene {
    fn id(&self) -> SceneId { SceneId::Home }

    fn on_create(&mut self, ctx: &mut SceneContext) -> SceneResult {
        info!("HomeScene: started");
        // Reset input state
        ctx.input.lock().unwrap().end_frame();
        self.scan_installed_games();
        info!(count = self.entries.len(), "HomeScene: games loaded");
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        if self.input_pressed(ctx, "up") && self.selection > 0 {
            self.selection -= 1;
        }
        if self.input_pressed(ctx, "down") && self.selection + 1 < self.entries.len() {
            self.selection += 1;
        }
        if self.input_pressed(ctx, "enter") || self.input_pressed(ctx, "space") {
            return self.launch_selected(ctx);
        }

        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        self.clear(ctx);

        let margin = 30.0;
        let list_w = 800.0 - margin * 2.0;
        let mut y = 0.0;

        // Title header
        self.rect(ctx, margin, 0.0, list_w, 44.0, 0.48, 0.23, 0.93, 1.0);
        self.text(ctx, margin + 12.0, 12.0, "VibeGE Game Store", 14.0, 1.0, 1.0, 1.0);
        self.text(ctx, margin + list_w - 130.0, 16.0, "AI-Friendly Overlay", 7.0, 1.0, 1.0, 1.0);
        y += 52.0;

        // Instruction bar
        self.rect(ctx, margin, y, list_w, 18.0, 0.10, 0.10, 0.22, 0.7);
        self.text(ctx, margin + 8.0, y + 3.0,
            "Arrows: Navigate     Enter: Launch     Esc: Home", 7.0, 0.5, 0.5, 0.6);
        y += 26.0;

        // Game cards
        for (i, game) in self.entries.iter().enumerate() {
            self.draw_card(ctx, margin, y, list_w, game, i == self.selection);
            y += 80.0; // card height + gap
        }

        // Bottom bar
        self.rect(ctx, margin, 578.0, list_w, 18.0, 0.10, 0.10, 0.22, 0.5);
        self.text(ctx, margin + 8.0, 580.0, "vibege-runtime v0.1.0", 7.0, 0.5, 0.5, 0.6);

        Ok(SceneAction::Continue)
    }
}
