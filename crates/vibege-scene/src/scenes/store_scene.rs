use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use std::io::Read;
use tracing::info;

const BACKEND: &str = "http://localhost:3000/api/v1";

fn fetch_registry() -> Result<Vec<serde_json::Value>, String> {
    let mut body = String::new();
    ureq::get(&format!("{BACKEND}/registry?limit=50"))
        .call()
        .map_err(|e| format!("HTTP: {e}"))?
        .into_body()
        .into_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("Read: {e}"))?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("JSON: {e}"))?;
    Ok(json["packages"].as_array().cloned().unwrap_or_default())
}

pub struct StoreScene {
    games: Vec<serde_json::Value>,
    selection: usize,
    loading: bool,
    error: Option<String>,
}

impl StoreScene {
    pub fn new() -> Self {
        Self {
            games: Vec::new(),
            selection: 0,
            loading: true,
            error: None,
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

impl Scene for StoreScene {
    fn id(&self) -> SceneId {
        SceneId::Store
    }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        info!("StoreScene: fetching games from backend");
        match fetch_registry() {
            Ok(games) => {
                self.games = games;
                self.loading = false;
                info!(count = self.games.len(), "Store loaded");
            }
            Err(e) => {
                self.error = Some(e);
                self.loading = false;
            }
        }
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        if self.loading {
            return Ok(SceneAction::Continue);
        }
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

        if esc {
            return Ok(SceneAction::Pop);
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

        if enter {
            if let Some(game) = self.games.get(self.selection) {
                let id = game["id"].as_str().unwrap_or("");
                let name = game["name"].as_str().unwrap_or("unnamed");
                info!("Store: installing {name} ({id})");

                // Download .vibepkg
                let dl_url = format!("{BACKEND}/registry/{id}/download");
                match ureq::get(&dl_url).call() {
                    Ok(resp) => {
                        let mut data: Vec<u8> = Vec::new();
                        if resp
                            .into_body()
                            .into_reader()
                            .read_to_end(&mut data)
                            .is_ok()
                        {
                            let install_dir = vibege_config::installed_games_dir().join(name);
                            if install_dir.exists() {
                                info!("Game already installed: {name}");
                            } else if let Err(e) = install_package(&data, name) {
                                info!("Install failed: {e}");
                            } else {
                                info!("Installed: {name}");
                            }
                        }
                    }
                    Err(e) => info!("Download failed: {e}"),
                }
            }
        }

        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        self.clear(ctx);

        self.rect(ctx, 30.0, 0.0, 740.0, 44.0, 0.48, 0.23, 0.93, 1.0);
        self.text(ctx, 42.0, 12.0, "Game Store", 14.0, 1.0, 1.0, 1.0);

        if self.loading {
            self.text(ctx, 350.0, 290.0, "Loading...", 10.0, 0.5, 0.5, 0.6);
        } else if let Some(ref err) = self.error {
            self.text(
                ctx,
                300.0,
                280.0,
                "Could not connect to Store",
                10.0,
                0.9,
                0.3,
                0.3,
            );
            self.text(ctx, 260.0, 310.0, err, 7.0, 0.5, 0.5, 0.6);
        } else if self.games.is_empty() {
            self.text(ctx, 320.0, 280.0, "No games available", 10.0, 0.5, 0.5, 0.6);
        } else {
            self.rect(ctx, 30.0, 48.0, 740.0, 18.0, 0.10, 0.10, 0.22, 0.7);
            self.text(
                ctx,
                42.0,
                51.0,
                "Arrows: Browse     Enter: Install     Esc: Back",
                7.0,
                0.5,
                0.5,
                0.6,
            );

            let mut y = 76.0;
            for (i, game) in self.games.iter().enumerate() {
                let card_h = 52.0;
                let name = game["name"].as_str().unwrap_or("Unknown");
                let desc = game["description"].as_str().unwrap_or("");
                let dl = game["downloads"].as_u64().unwrap_or(0);

                if i == self.selection {
                    self.rect(ctx, 30.0, y, 740.0, card_h, 0.25, 0.15, 0.45, 1.0);
                    self.rect(ctx, 30.0, y, 3.0, card_h, 0.48, 0.23, 0.93, 1.0);
                } else {
                    self.rect(ctx, 30.0, y, 740.0, card_h, 0.10, 0.10, 0.22, 1.0);
                }

                self.text(ctx, 46.0, y + 6.0, name, 10.0, 1.0, 1.0, 1.0);
                self.text(ctx, 46.0, y + 26.0, desc, 7.0, 0.5, 0.5, 0.6);
                self.text(
                    ctx,
                    680.0,
                    y + 26.0,
                    &format!("{} dl", dl),
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );

                y += card_h + 4.0;
            }
        }

        // Bottom bar
        self.rect(ctx, 30.0, 560.0, 740.0, 22.0, 0.10, 0.10, 0.22, 0.6);
        self.text(ctx, 42.0, 563.0, "Esc: Back", 7.0, 0.5, 0.5, 0.6);

        Ok(SceneAction::Continue)
    }
}

fn install_package(data: &[u8], name: &str) -> Result<(), String> {
    use std::io::Write;
    // Verify ZIP header
    if data.len() < 4 || data[0] != 0x50 || data[1] != 0x4B || data[2] != 0x03 || data[3] != 0x04 {
        return Err("Invalid .vibepkg format".into());
    }

    let install_dir = vibege_config::installed_games_dir().join(name);
    std::fs::create_dir_all(&install_dir).map_err(|e| format!("Create dir: {e}"))?;

    // Extract ZIP using the zip crate
    let cursor = std::io::Cursor::new(data);
    match zip::ZipArchive::new(cursor) {
        Ok(mut archive) => {
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).map_err(|e| format!("ZIP entry: {e}"))?;
                if entry.is_dir() {
                    continue;
                }
                let target = install_dir.join(entry.name());
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("Dir: {e}"))?;
                }
                let mut content = Vec::new();
                entry
                    .read_to_end(&mut content)
                    .map_err(|e| format!("Read: {e}"))?;
                let mut f = std::fs::File::create(&target).map_err(|e| format!("Create: {e}"))?;
                f.write_all(&content).map_err(|e| format!("Write: {e}"))?;
            }
        }
        Err(e) => return Err(format!("Invalid ZIP: {e}")),
    }

    // Write install manifest
    let meta = serde_json::json!({
        "name": name,
        "entry": "src/main.lua",
        "version": "0.1.0",
        "installed_at": format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()),
    });
    let meta_path = install_dir.join(".vibege-install.json");
    std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Meta: {e}"))?;

    Ok(())
}
