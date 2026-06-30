use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::info;

/// Displays installed games, allows launch and uninstall.
pub struct LibraryScene {
    selection: usize,
}

impl LibraryScene {
    pub fn new() -> Self {
        Self { selection: 0 }
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

    fn scan_games() -> Vec<serde_json::Value> {
        let mut games = Vec::new();
        let dir = vibege_config::installed_games_dir();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let meta_path = path.join(".vibege-install.json");
                if !meta_path.exists() {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&meta_path) {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                        let mut m = meta.clone();
                        m["_path"] = serde_json::Value::String(path.to_string_lossy().to_string());
                        games.push(m);
                    }
                }
            }
        }
        games
    }
}

impl Scene for LibraryScene {
    fn id(&self) -> SceneId {
        SceneId::Library
    }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        info!("LibraryScene: showing installed games");
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        let games = Self::scan_games();
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
        let del = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("delete"));

        if esc {
            return Ok(SceneAction::Pop);
        }

        if games.is_empty() {
            return Ok(SceneAction::Continue);
        }

        if up && self.selection > 0 {
            self.selection -= 1;
        }
        if down && self.selection + 1 < games.len() {
            self.selection += 1;
        }

        if enter {
            if let Some(game) = games.get(self.selection) {
                let entry = game["entry"].as_str().unwrap_or("src/main.lua");
                let base = game["_path"].as_str().unwrap_or("");
                let full_path = std::path::Path::new(base).join(entry);
                if full_path.exists() {
                    if let Ok(source) = std::fs::read_to_string(&full_path) {
                        let gs = Box::new(super::game_scene::GameScene::new(
                            source,
                            ctx.renderer.clone(),
                            ctx.input.clone(),
                            None,
                        ));
                        return Ok(SceneAction::Push(gs));
                    }
                }
            }
        }

        if del {
            if let Some(game) = games.get(self.selection) {
                let name = game["name"].as_str().unwrap_or("");
                let path = game["_path"].as_str().unwrap_or("");
                if !path.is_empty() {
                    std::fs::remove_dir_all(path).ok();
                    info!("Uninstalled: {name}");
                }
            }
        }

        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        self.clear(ctx);
        let games = Self::scan_games();

        // Title
        self.rect(ctx, 30.0, 0.0, 740.0, 44.0, 0.48, 0.23, 0.93, 1.0);
        self.text(ctx, 42.0, 12.0, "Installed Games", 14.0, 1.0, 1.0, 1.0);
        self.text(
            ctx,
            600.0,
            14.0,
            &format!("{} games", games.len()),
            8.0,
            0.5,
            0.5,
            0.6,
        );

        if games.is_empty() {
            self.text(ctx, 300.0, 280.0, "No games installed", 12.0, 0.5, 0.5, 0.6);
            self.text(
                ctx,
                260.0,
                310.0,
                "Browse the Store to find games",
                8.0,
                0.5,
                0.5,
                0.6,
            );
        } else {
            let mut y = 56.0;
            for (i, game) in games.iter().enumerate() {
                let name = game["name"].as_str().unwrap_or("Unknown");
                let entry = game["entry"].as_str().unwrap_or("src/main.lua");
                let selected = i == self.selection;

                if selected {
                    self.rect(ctx, 30.0, y, 740.0, 50.0, 0.25, 0.15, 0.45, 1.0);
                    self.rect(ctx, 30.0, y, 3.0, 50.0, 0.48, 0.23, 0.93, 1.0);
                } else {
                    self.rect(ctx, 30.0, y, 740.0, 50.0, 0.10, 0.10, 0.22, 1.0);
                }

                self.text(ctx, 46.0, y + 8.0, name, 10.0, 1.0, 1.0, 1.0);
                self.text(ctx, 46.0, y + 28.0, entry, 7.0, 0.5, 0.5, 0.6);

                y += 56.0;
            }
        }

        // Bottom bar
        self.rect(ctx, 30.0, 560.0, 740.0, 22.0, 0.10, 0.10, 0.22, 0.6);
        self.text(
            ctx,
            42.0,
            563.0,
            "Arrows: Navigate     Enter: Launch     Delete: Uninstall     Esc: Back",
            7.0,
            0.5,
            0.5,
            0.6,
        );

        Ok(SceneAction::Continue)
    }
}
