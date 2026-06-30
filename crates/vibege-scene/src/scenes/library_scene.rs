use std::sync::Arc;

use crate::input_helper::InputState;
use crate::library::manager::LibraryManager;
use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::info;

pub struct LibraryScene {
    manager: Arc<LibraryManager>,
    selection: usize,
    view_mode: ViewMode,
    game_names: Vec<String>,
}

enum ViewMode {
    List,
    Collections,
    CollectionView(usize),
}

impl LibraryScene {
    pub fn new(backend: String) -> Self {
        let manager = Arc::new(LibraryManager::new(backend));
        manager.initialize();
        let game_names = manager
            .games()
            .into_iter()
            .map(|g| g.name.clone())
            .collect();
        Self {
            manager,
            selection: 0,
            view_mode: ViewMode::List,
            game_names,
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

    fn current_games(&self) -> Vec<crate::library::models::InstalledGame> {
        match &self.view_mode {
            ViewMode::List => self.manager.games(),
            ViewMode::Collections => Vec::new(),
            ViewMode::CollectionView(idx) => {
                let collections = self.manager.collections.all();
                collections
                    .get(*idx)
                    .map(|c| {
                        c.game_names
                            .iter()
                            .filter_map(|name| self.manager.registry.get(name))
                            .collect()
                    })
                    .unwrap_or_default()
            }
        }
    }
}

impl Scene for LibraryScene {
    fn id(&self) -> SceneId {
        SceneId::Library
    }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        info!(
            "LibraryScene: {} games found",
            self.manager.registry.count()
        );
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        let inp = InputState::new(
            &ctx.input,
            &[
                "up", "down", "enter", "escape", "r", "f", "delete", "c", "u",
            ],
        );

        if inp.pressed(4)
        /* esc */
        {
            match &self.view_mode {
                ViewMode::CollectionView(_) => {
                    self.view_mode = ViewMode::Collections;
                    self.selection = 0;
                }
                ViewMode::Collections => {
                    self.view_mode = ViewMode::List;
                    self.selection = 0;
                }
                ViewMode::List => {
                    return Ok(SceneAction::Pop);
                }
            }
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(5)
        /* r */
        {
            self.manager.refresh();
            self.game_names = self
                .manager
                .games()
                .into_iter()
                .map(|g| g.name.clone())
                .collect();
            self.selection = 0;
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(9) /* c */ && matches!(self.view_mode, ViewMode::List) {
            self.view_mode = ViewMode::Collections;
            self.selection = 0;
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(8) /* u */ && matches!(self.view_mode, ViewMode::List) {
            self.manager.refresh_updates();
            return Ok(SceneAction::Continue);
        }

        match &self.view_mode {
            ViewMode::Collections => {
                let collections = self.manager.collections.all();
                if inp.pressed(0) && self.selection > 0 {
                    self.selection -= 1;
                }
                if inp.pressed(1) && self.selection + 1 < collections.len() {
                    self.selection += 1;
                }
                if inp.pressed(2) {
                    self.view_mode = ViewMode::CollectionView(self.selection);
                    self.selection = 0;
                }
            }
            _ => {
                let games = self.current_games();
                if games.is_empty() {
                    return Ok(SceneAction::Continue);
                }

                if inp.pressed(0) && self.selection > 0 {
                    self.selection -= 1;
                }
                if inp.pressed(1) && self.selection + 1 < games.len() {
                    self.selection += 1;
                }

                if inp.pressed(6)
                /* f */
                {
                    if let Some(game) = games.get(self.selection) {
                        let now_fav = self.manager.toggle_favorite(&game.name);
                        info!(
                            "{} is now {}",
                            game.name,
                            if now_fav { "favorite" } else { "unfavorited" }
                        );
                    }
                }

                if inp.pressed(7)
                /* del */
                {
                    if let Some(game) = games.get(self.selection) {
                        if let Err(e) = self.manager.uninstall(&game.name) {
                            info!("Uninstall failed: {e}");
                        } else {
                            info!("Uninstalled: {}", game.name);
                            self.game_names = self
                                .manager
                                .games()
                                .into_iter()
                                .map(|g| g.name.clone())
                                .collect();
                            self.selection = 0;
                        }
                    }
                }

                if inp.pressed(2) {
                    if let Some(game) = games.get(self.selection) {
                        let entry = &game.entry_point;
                        let base = &game.path;
                        let full_path = base.join(entry);
                        if full_path.exists() {
                            if let Ok(source) = std::fs::read_to_string(&full_path) {
                                self.manager.launch(&game.name);
                                let gs = Box::new(super::game_scene::GameScene::new(
                                    source,
                                    game.name.clone(),
                                    ctx.screen_width,
                                    ctx.screen_height,
                                ));
                                return Ok(SceneAction::Push(gs));
                            }
                        }
                    }
                }
            }
        }

        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        self.clear(ctx);

        // Title bar
        self.rect(ctx, 30.0, 0.0, 740.0, 44.0, 0.48, 0.23, 0.93, 1.0);

        match &self.view_mode {
            ViewMode::Collections => {
                self.text(ctx, 42.0, 12.0, "Collections", 14.0, 1.0, 1.0, 1.0);

                self.rect(ctx, 30.0, 48.0, 740.0, 18.0, 0.10, 0.10, 0.22, 0.7);
                self.text(
                    ctx,
                    42.0,
                    51.0,
                    "Up/Down: Browse     Enter: View     Esc: Back",
                    7.0,
                    0.5,
                    0.5,
                    0.6,
                );

                let collections = self.manager.collections.all();
                let mut y = 76.0;
                for (i, collection) in collections.iter().enumerate() {
                    let card_h = 52.0;
                    if i == self.selection {
                        self.rect(ctx, 30.0, y, 740.0, card_h, 0.25, 0.15, 0.45, 1.0);
                        self.rect(ctx, 30.0, y, 3.0, card_h, 0.48, 0.23, 0.93, 1.0);
                    } else {
                        self.rect(ctx, 30.0, y, 740.0, card_h, 0.10, 0.10, 0.22, 1.0);
                    }
                    self.text(ctx, 46.0, y + 6.0, &collection.name, 10.0, 1.0, 1.0, 1.0);
                    self.text(
                        ctx,
                        46.0,
                        y + 26.0,
                        &format!("{} games", collection.game_names.len()),
                        7.0,
                        0.5,
                        0.5,
                        0.6,
                    );
                    y += card_h + 4.0;
                }
            }
            _ => {
                let count = self.manager.registry.count();
                let update_count = self.manager.available_updates().len();
                let title = if update_count > 0 {
                    format!(
                        "Game Library  |  {} installed  |  {} updates",
                        count, update_count
                    )
                } else {
                    format!("Game Library  |  {} installed", count)
                };
                self.text(ctx, 42.0, 12.0, &title, 14.0, 1.0, 1.0, 1.0);

                self.rect(ctx, 30.0, 48.0, 740.0, 18.0, 0.10, 0.10, 0.22, 0.7);
                self.text(ctx, 42.0, 51.0,
                    "Up/Down: Browse     Enter: Launch     F: Favourite     C: Collections     U: Check Updates     R: Refresh     Del: Uninstall     Esc: Back",
                    7.0, 0.5, 0.5, 0.6,
                );

                let games = self.current_games();
                if games.is_empty() {
                    self.text(ctx, 300.0, 280.0, "No games found", 12.0, 0.5, 0.5, 0.6);
                    if matches!(self.view_mode, ViewMode::List) {
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
                    }
                } else {
                    let mut y = 76.0;
                    for (i, game) in games.iter().enumerate() {
                        let card_h = 52.0;
                        if i == self.selection {
                            self.rect(ctx, 30.0, y, 740.0, card_h, 0.25, 0.15, 0.45, 1.0);
                            self.rect(ctx, 30.0, y, 3.0, card_h, 0.48, 0.23, 0.93, 1.0);
                        } else {
                            self.rect(ctx, 30.0, y, 740.0, card_h, 0.10, 0.10, 0.22, 1.0);
                        }

                        let is_fav = self.manager.collections.is_favorite(&game.name);
                        let has_update = self.manager.has_update(&game.name);
                        let fav = if is_fav { "★ " } else { "  " };

                        self.text(
                            ctx,
                            46.0,
                            y + 6.0,
                            &format!("{}{}", fav, game.name),
                            10.0,
                            1.0,
                            1.0,
                            1.0,
                        );

                        if has_update {
                            self.rect(ctx, 680.0, y + 4.0, 56.0, 14.0, 0.9, 0.7, 0.2, 0.2);
                            self.text(ctx, 686.0, y + 5.0, "UPDATE", 7.0, 0.9, 0.7, 0.2);
                        }

                        let size_str = format_file_size(game.size_bytes);
                        let details = format!(
                            "v{} by {}  |  {}  |  {} plays",
                            game.version, game.author, size_str, game.play_count
                        );
                        self.text(ctx, 46.0, y + 26.0, &details, 7.0, 0.5, 0.5, 0.6);
                        self.text(ctx, 600.0, y + 26.0, &game.entry_point, 7.0, 0.5, 0.5, 0.6);

                        y += card_h + 4.0;
                    }
                }
            }
        }

        // Bottom bar
        self.rect(ctx, 30.0, 560.0, 740.0, 22.0, 0.10, 0.10, 0.22, 0.6);
        self.text(ctx, 42.0, 563.0,
            "Esc: Back     Enter: Launch     F: Fav     C: Collections     U: Updates     R: Refresh",
            7.0, 0.5, 0.5, 0.6,
        );

        Ok(SceneAction::Continue)
    }
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
