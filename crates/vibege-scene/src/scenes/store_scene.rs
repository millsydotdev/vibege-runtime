use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use std::io::Read;
use tracing::info;

fn fetch_registry(backend: &str, search: &str) -> Result<Vec<serde_json::Value>, String> {
    let url = if search.is_empty() {
        format!("{backend}/registry?limit=50")
    } else {
        format!("{backend}/registry?limit=50&search={}", urlencoding(search))
    };
    let mut body = String::new();
    ureq::get(&url)
        .call()
        .map_err(|e| format!("HTTP: {e}"))?
        .into_body()
        .into_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("Read: {e}"))?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("JSON: {e}"))?;
    Ok(json["packages"].as_array().cloned().unwrap_or_default())
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c.to_string(),
            ' ' => "+".into(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

fn download_package(backend: &str, id: &str) -> Result<Vec<u8>, String> {
    let mut data: Vec<u8> = Vec::new();
    ureq::get(&format!("{backend}/registry/{id}/download"))
        .call()
        .map_err(|e| format!("Download HTTP: {e}"))?
        .into_body()
        .into_reader()
        .read_to_end(&mut data)
        .map_err(|e| format!("Download read: {e}"))?;
    Ok(data)
}

pub struct StoreScene {
    games: Vec<serde_json::Value>,
    selection: usize,
    loading: bool,
    error: Option<String>,
    backend: String,
    search: String,
    search_mode: bool,
    search_cursor: usize,
}

impl StoreScene {
    pub fn new(backend: String) -> Self {
        Self {
            games: Vec::new(),
            selection: 0,
            loading: true,
            error: None,
            backend,
            search: String::new(),
            search_mode: false,
            search_cursor: 0,
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
        let bk = self.backend.clone();
        info!("StoreScene: fetching from {bk}");
        match fetch_registry(&bk, "") {
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
        let s = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("s"));
        let r = ctx
            .input
            .lock()
            .expect("lock")
            .is_key_pressed(vibege_input::key_name_to_code("r"));

        if esc && !self.search_mode {
            return Ok(SceneAction::Pop);
        }
        if esc && self.search_mode {
            self.search_mode = false;
            self.search.clear();
            return Ok(SceneAction::Continue);
        }

        if s && !self.search_mode {
            self.search_mode = true;
            self.search_cursor = 0;
            return Ok(SceneAction::Continue);
        }

        if self.search_mode {
            // Simplified alpha search input using up/down to cycle letters
            if up {
                let c = self.search.chars().last().unwrap_or('a');
                let next = match c {
                    'a'..='y' => ((c as u8) + 1) as char,
                    'z' => ' ',
                    ' ' => 'a',
                    _ => 'a',
                };
                if self.search_cursor == 0 {
                    self.search = next.to_string();
                } else {
                    self.search.pop();
                    self.search.push(next);
                }
            }
            if down {
                let c = self.search.chars().last().unwrap_or('a');
                let prev = match c {
                    'b'..='z' => ((c as u8) - 1) as char,
                    'a' => ' ',
                    ' ' => 'z',
                    _ => 'a',
                };
                if self.search_cursor == 0 {
                    self.search = prev.to_string();
                } else {
                    self.search.pop();
                    self.search.push(prev);
                }
            }
            if enter && !self.search.is_empty() {
                self.loading = true;
                let bk = self.backend.clone();
                let q = self.search.clone();
                match fetch_registry(&bk, &q) {
                    Ok(games) => {
                        self.games = games;
                        self.loading = false;
                        self.selection = 0;
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.loading = false;
                    }
                }
            }
            return Ok(SceneAction::Continue);
        }

        // Normal navigation
        if r {
            self.loading = true;
            let bk = self.backend.clone();
            match fetch_registry(&bk, "") {
                Ok(games) => {
                    self.games = games;
                    self.loading = false;
                    self.selection = 0;
                }
                Err(e) => {
                    self.error = Some(e);
                    self.loading = false;
                }
            }
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

        if enter {
            if let Some(game) = self.games.get(self.selection) {
                let id = game["id"].as_str().unwrap_or("");
                let name = game["name"].as_str().unwrap_or("unnamed");
                info!("Store: installing {name} ({id})");
                match download_package(&self.backend, id) {
                    Ok(data) => {
                        if let Err(e) = install_package(&data, name) {
                            info!("Install failed: {e}");
                        } else {
                            info!("Installed: {name}");
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

        // Title
        self.rect(ctx, 30.0, 0.0, 740.0, 44.0, 0.48, 0.23, 0.93, 1.0);
        if self.search_mode {
            self.text(
                ctx,
                42.0,
                12.0,
                &format!("Search: {}", self.search),
                14.0,
                1.0,
                1.0,
                1.0,
            );
        } else {
            self.text(ctx, 42.0, 12.0, "Game Store", 14.0, 1.0, 1.0, 1.0);
        }

        if self.loading {
            self.text(ctx, 350.0, 290.0, "Loading...", 10.0, 0.5, 0.5, 0.6);
            return Ok(SceneAction::Continue);
        }

        if let Some(ref err) = self.error {
            self.text(ctx, 300.0, 280.0, "Store unavailable", 10.0, 0.9, 0.3, 0.3);
            self.text(ctx, 260.0, 310.0, err, 7.0, 0.5, 0.5, 0.6);
            self.text(ctx, 280.0, 340.0, "Press R to retry", 8.0, 0.5, 0.5, 0.6);
            return Ok(SceneAction::Continue);
        }

        // Instructions
        self.rect(ctx, 30.0, 48.0, 740.0, 18.0, 0.10, 0.10, 0.22, 0.7);
        self.text(
            ctx,
            42.0,
            51.0,
            "Arrows: Browse     Enter: Install     S: Search     R: Refresh     Esc: Back",
            7.0,
            0.5,
            0.5,
            0.6,
        );

        if self.games.is_empty() {
            self.text(ctx, 320.0, 280.0, "No games found", 10.0, 0.5, 0.5, 0.6);
            self.text(
                ctx,
                280.0,
                310.0,
                "Check backend is running",
                8.0,
                0.5,
                0.5,
                0.6,
            );
        } else {
            let mut y = 76.0;
            for (i, game) in self.games.iter().enumerate() {
                let card_h = 52.0;
                let name = game["name"].as_str().unwrap_or("Unknown");
                let desc = game["description"].as_str().unwrap_or("");
                let dl = game["downloads"].as_u64().unwrap_or(0);
                let status = game["status"].as_str().unwrap_or("");

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

                // Status badge
                if status == "approved" {
                    self.rect(ctx, 680.0, y + 4.0, 50.0, 14.0, 0.2, 0.8, 0.4, 0.2);
                    self.text(ctx, 686.0, y + 5.0, "LIVE", 7.0, 0.2, 0.8, 0.4);
                }

                y += card_h + 4.0;
            }
        }

        // Bottom bar
        self.rect(ctx, 30.0, 560.0, 740.0, 22.0, 0.10, 0.10, 0.22, 0.6);
        self.text(
            ctx,
            42.0,
            563.0,
            "Esc: Back     S: Search     R: Refresh     Enter: Install",
            7.0,
            0.5,
            0.5,
            0.6,
        );

        Ok(SceneAction::Continue)
    }
}

/// Install a .vibepkg buffer to the game library.
pub fn install_package(data: &[u8], name: &str) -> Result<(), String> {
    use std::io::Write;
    if data.len() < 4 || data[0] != 0x50 || data[1] != 0x4B || data[2] != 0x03 || data[3] != 0x04 {
        return Err("Invalid .vibepkg: not a ZIP archive".into());
    }
    let install_dir = vibege_config::installed_games_dir().join(name);
    std::fs::create_dir_all(&install_dir).map_err(|e| format!("Create dir: {e}"))?;
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
    let meta = serde_json::json!({
        "name": name, "entry": "src/main.lua", "version": "0.1.0",
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
