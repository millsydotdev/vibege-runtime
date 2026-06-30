use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::info;

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
                if let Ok(mut meta) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = meta.as_object_mut() {
                        obj.insert(
                            "_path".into(),
                            serde_json::Value::String(path.to_string_lossy().to_string()),
                        );
                        let size: u64 = path
                            .read_dir()
                            .ok()
                            .map(|e| {
                                e.flatten()
                                    .filter_map(|f| f.metadata().ok())
                                    .map(|m| m.len())
                                    .sum()
                            })
                            .unwrap_or(0);
                        obj.insert(
                            "_size".into(),
                            serde_json::Value::Number(serde_json::Number::from(size)),
                        );
                    }
                    games.push(meta);
                }
            }
        }
    }
    games.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(&b["name"].as_str().unwrap_or(""))
    });
    games
}

fn size_str(size: u64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    }
}

pub struct LibraryScene {
    selection: usize,
    games: Vec<serde_json::Value>,
    favourites: std::collections::HashSet<String>,
}

impl LibraryScene {
    pub fn new() -> Self {
        Self {
            selection: 0,
            games: scan_games(),
            favourites: std::collections::HashSet::new(),
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
}

impl Scene for LibraryScene {
    fn id(&self) -> SceneId {
        SceneId::Library
    }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        info!("LibraryScene: {} games found", self.games.len());
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
        let r = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("r"));
        let f = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("f"));

        if esc {
            return Ok(SceneAction::Pop);
        }
        if r {
            self.games = scan_games();
            self.selection = 0;
            return Ok(SceneAction::Continue);
        }

        if self.games.is_empty() {
            return Ok(SceneAction::Continue);
        }

        if up && self.selection > 0 {
            self.selection -= 1;
        }
        if down && self.selection + 1 < self.games.len() {
            self.selection += 1;
        }

        if f {
            if let Some(game) = self.games.get(self.selection) {
                let name = game["name"].as_str().unwrap_or("").to_string();
                if !self.favourites.insert(name.clone()) {
                    self.favourites.remove(&name);
                }
            }
        }

        if del {
            if let Some(game) = self.games.get(self.selection) {
                let name = game["name"].as_str().unwrap_or("").to_string();
                if let Some(path) = game["_path"].as_str() {
                    std::fs::remove_dir_all(path).ok();
                    info!("Uninstalled: {name}");
                }
            }
            self.games = scan_games();
            self.selection = 0;
        }

        if enter {
            if let Some(game) = self.games.get(self.selection) {
                let entry = game["entry"].as_str().unwrap_or("src/main.lua");
                let base = game["_path"].as_str().unwrap_or("");
                let full_path = std::path::Path::new(base).join(entry);
                if full_path.exists() {
                    if let Ok(source) = std::fs::read_to_string(&full_path) {
                        // Update last played
                        if let Ok(content) = std::fs::read_to_string(
                            std::path::Path::new(base).join(".vibege-install.json"),
                        ) {
                            if let Ok(mut meta) =
                                serde_json::from_str::<serde_json::Value>(&content)
                            {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs();
                                if let Some(obj) = meta.as_object_mut() {
                                    obj.insert(
                                        "last_played".into(),
                                        serde_json::Value::Number(serde_json::Number::from(now)),
                                    );
                                }
                                let _ = std::fs::write(
                                    std::path::Path::new(base).join(".vibege-install.json"),
                                    serde_json::to_string_pretty(&meta).unwrap_or_default(),
                                );
                            }
                        }
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

        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        self.clear(ctx);

        // Title
        self.rect(ctx, 30.0, 0.0, 740.0, 44.0, 0.48, 0.23, 0.93, 1.0);
        self.text(ctx, 42.0, 12.0, "Game Library", 14.0, 1.0, 1.0, 1.0);
        self.text(
            ctx,
            620.0,
            14.0,
            &format!("{} installed", self.games.len()),
            8.0,
            0.5,
            0.5,
            0.6,
        );

        // Instructions
        self.rect(ctx, 30.0, 48.0, 740.0, 18.0, 0.10, 0.10, 0.22, 0.7);
        self.text(ctx, 42.0, 51.0, "Arrows: Navigate     Enter: Launch     F: Favourite     R: Refresh     Del: Uninstall     Esc: Back", 7.0, 0.5, 0.5, 0.6);

        if self.games.is_empty() {
            self.text(ctx, 300.0, 280.0, "No games installed", 12.0, 0.5, 0.5, 0.6);
            self.text(
                ctx,
                260.0,
                310.0,
                "Use 'vibege install <file>.vibepkg' to install games",
                8.0,
                0.5,
                0.5,
                0.6,
            );
        } else {
            let mut y = 76.0;
            for (i, game) in self.games.iter().enumerate() {
                let card_h = 52.0;
                if i == self.selection {
                    self.rect(ctx, 30.0, y, 740.0, card_h, 0.25, 0.15, 0.45, 1.0);
                    self.rect(ctx, 30.0, y, 3.0, card_h, 0.48, 0.23, 0.93, 1.0);
                } else {
                    self.rect(ctx, 30.0, y, 740.0, card_h, 0.10, 0.10, 0.22, 1.0);
                }

                let name = game["name"].as_str().unwrap_or("Unknown");
                let version = game["version"].as_str().unwrap_or("0.1.0");
                let author = game["author"].as_str().unwrap_or("Unknown");
                let entry = game["entry"].as_str().unwrap_or("src/main.lua");
                let size_val = game["_size"].as_u64().unwrap_or(0);
                let is_fav = self.favourites.contains(name);

                let fav = if is_fav { "★ " } else { "  " };
                self.text(
                    ctx,
                    46.0,
                    y + 6.0,
                    &format!("{}{}", fav, name),
                    10.0,
                    1.0,
                    1.0,
                    1.0,
                );
                self.text(
                    ctx,
                    46.0,
                    y + 26.0,
                    &format!("v{} by {}  |  {}", version, author, size_str(size_val)),
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );
                self.text(ctx, 600.0, y + 26.0, entry, 7.0, 0.5, 0.5, 0.6);

                y += card_h + 4.0;
            }
        }

        Ok(SceneAction::Continue)
    }
}
