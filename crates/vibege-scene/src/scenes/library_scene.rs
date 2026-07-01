use std::sync::Arc;

use crate::input_helper::InputState;
use crate::library::manager::LibraryManager;
use crate::library::models::{InstalledGame, LibrarySortField, LibrarySortOrder};
use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::info;

pub struct LibraryScene {
    manager: Arc<LibraryManager>,
    selection: usize,
    view_mode: ViewMode,
    display_mode: DisplayMode,
    sort_field: LibrarySortField,
    sort_order: LibrarySortOrder,
    filter_favorites: bool,
    filter_updates: bool,
    game_names: Vec<String>,
    multi_select: Vec<usize>,
}

enum ViewMode {
    List,
    Collections,
    CollectionView(usize),
}

enum DisplayMode {
    List,
    Grid,
}

impl LibraryScene {
    pub fn new(backend: String) -> Self {
        let manager = Arc::new(LibraryManager::new(backend));
        manager.initialize();
        let game_names = manager.games().into_iter().map(|g| g.name.clone()).collect();
        Self {
            manager,
            selection: 0,
            view_mode: ViewMode::List,
            display_mode: DisplayMode::List,
            sort_field: LibrarySortField::Name,
            sort_order: LibrarySortOrder::Ascending,
            filter_favorites: false,
            filter_updates: false,
            game_names,
            multi_select: Vec::new(),
        }
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

    fn current_games(&self) -> Vec<InstalledGame> {
        let mut games = match &self.view_mode {
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
        };

        if self.filter_favorites {
            let favs = self.manager.collections.by_kind(crate::library::models::CollectionKind::Favorites);
            games.retain(|g| favs.contains(&g.name));
        }
        if self.filter_updates {
            games.retain(|g| self.manager.has_update(&g.name));
        }

        let descending = self.sort_order == LibrarySortOrder::Descending;
        match self.sort_field {
            LibrarySortField::Name => {
                games.sort_by_key(|g| g.name.clone());
                if descending { games.reverse(); }
            }
            LibrarySortField::LastPlayed => {
                games.sort_by_key(|g| std::cmp::Reverse(g.last_played));
                if !descending { games.reverse(); }
            }
            LibrarySortField::PlayTime => {
                games.sort_by_key(|g| std::cmp::Reverse(g.total_play_time_secs));
                if !descending { games.reverse(); }
            }
            LibrarySortField::InstallDate => {
                games.sort_by_key(|g| std::cmp::Reverse(g.installed_at));
                if !descending { games.reverse(); }
            }
            _ => {}
        }

        games
    }

    fn sort_field_name(&self) -> &'static str {
        match self.sort_field {
            LibrarySortField::Name => "Name",
            LibrarySortField::LastPlayed => "Last Played",
            LibrarySortField::PlayTime => "Play Time",
            LibrarySortField::InstallDate => "Installed",
            _ => "Name",
        }
    }

    fn cycle_sort(&mut self) {
        self.sort_field = match self.sort_field {
            LibrarySortField::Name => LibrarySortField::LastPlayed,
            LibrarySortField::LastPlayed => LibrarySortField::PlayTime,
            LibrarySortField::PlayTime => LibrarySortField::InstallDate,
            LibrarySortField::InstallDate => LibrarySortField::Name,
            _ => LibrarySortField::Name,
        };
    }

    fn toggle_multi_select(&mut self, idx: usize) {
        if let Some(pos) = self.multi_select.iter().position(|&i| i == idx) {
            self.multi_select.remove(pos);
        } else {
            self.multi_select.push(idx);
        }
    }

    fn bulk_operation_label(&self) -> String {
        if self.multi_select.is_empty() {
            return String::new();
        }
        format!(" {} selected  [X] Clear", self.multi_select.len())
    }
}

impl Scene for LibraryScene {
    fn id(&self) -> SceneId {
        SceneId::Library
    }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        info!("LibraryScene: {} games found", self.manager.registry.count());
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        let inp = InputState::new(&ctx.input, &[
            "up", "down", "enter", "escape", "r", "f", "delete", "c", "u",
            "v", "s", "x", "a",
        ]);

        if inp.pressed(4) {
            match &self.view_mode {
                ViewMode::CollectionView(_) => { self.view_mode = ViewMode::Collections; self.selection = 0; }
                ViewMode::Collections => { self.view_mode = ViewMode::List; self.selection = 0; }
                ViewMode::List => { return Ok(SceneAction::Pop); }
            }
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(5) {
            self.manager.refresh();
            self.game_names = self.manager.games().into_iter().map(|g| g.name.clone()).collect();
            self.selection = 0;
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(9) && matches!(self.view_mode, ViewMode::List) {
            self.view_mode = ViewMode::Collections;
            self.selection = 0;
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(8) && matches!(self.view_mode, ViewMode::List) {
            self.manager.refresh_updates();
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(10) && matches!(self.view_mode, ViewMode::List) {
            self.display_mode = match self.display_mode {
                DisplayMode::List => DisplayMode::Grid,
                DisplayMode::Grid => DisplayMode::List,
            };
            self.selection = 0;
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(11) && matches!(self.view_mode, ViewMode::List) {
            self.cycle_sort();
            self.selection = 0;
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(6) && matches!(self.view_mode, ViewMode::List) {
            self.filter_favorites = !self.filter_favorites;
            self.selection = 0;
            return Ok(SceneAction::Continue);
        }

        if matches!(self.view_mode, ViewMode::List) {
            if inp.pressed(12) {
                if !self.multi_select.is_empty() {
                    self.multi_select.clear();
                }
            }

            // Multi-select with shift held
            if inp.pressed(2) {
                let input = ctx.input.lock().expect("lock");
                let shift = input.is_key_down(vibege_input::key_name_to_code("lshift"))
                    || input.is_key_down(vibege_input::key_name_to_code("rshift"));
                drop(input);
                if shift && !self.multi_select.is_empty() {
                    self.toggle_multi_select(self.selection);
                    return Ok(SceneAction::Continue);
                }
                if !self.multi_select.is_empty() {
                    self.multi_select.clear();
                }
            }
        }

        match &self.view_mode {
            ViewMode::Collections => {
                let collections = self.manager.collections.all();
                if inp.pressed(0) && self.selection > 0 { self.selection -= 1; }
                if inp.pressed(1) && self.selection + 1 < collections.len() { self.selection += 1; }
                if inp.pressed(2) { self.view_mode = ViewMode::CollectionView(self.selection); self.selection = 0; }
            }
            _ => {
                let games = self.current_games();
                if games.is_empty() { return Ok(SceneAction::Continue); }

                if inp.pressed(0) && self.selection > 0 { self.selection -= 1; }
                if inp.pressed(1) && self.selection + 1 < games.len() { self.selection += 1; }

                if inp.pressed(7) {
                    if let Some(game) = games.get(self.selection) {
                        if let Err(e) = self.manager.uninstall(&game.name) {
                            info!("Uninstall failed: {e}");
                        } else {
                            info!("Uninstalled: {}", game.name);
                            self.game_names = self.manager.games().into_iter().map(|g| g.name.clone()).collect();
                            self.selection = 0;
                        }
                    }
                }

                if inp.pressed(13) {
                    if let Some(game) = games.get(self.selection) {
                        let now_fav = self.manager.toggle_favorite(&game.name);
                        info!("{} is now {}", game.name, if now_fav { "favorite" } else { "unfavorited" });
                    }
                }

                if inp.pressed(2) {
                    if let Some(game) = games.get(self.selection) {
                        let full_path = game.path.join(&game.entry_point);
                        if full_path.exists() {
                            if let Ok(source) = std::fs::read_to_string(&full_path) {
                                self.manager.launch(&game.name);
                                let gs = Box::new(super::game_scene::GameScene::new(
                                    source, game.name.clone(), ctx.screen_width, ctx.screen_height,
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

        let mg = 24.0;
        let list_w = 800.0 - mg * 2.0;
        let mut y = 6.0;

        // Title bar
        self.rect(ctx, mg, y, list_w, 36.0, 0.48, 0.23, 0.93, 1.0);

        match &self.view_mode {
            ViewMode::Collections => {
                self.text(ctx, 36.0, 14.0, "Collections", 12.0, 1.0, 1.0, 1.0);
                y += 42.0;

                self.rect(ctx, mg, y, list_w, 16.0, 0.10, 0.10, 0.22, 0.7);
                self.text(ctx, 36.0, y + 2.0, "Up/Down: Browse     Enter: View     Esc: Back", 6.0, 0.5, 0.5, 0.6);
                y += 22.0;

                let collections = self.manager.collections.all();
                for (i, c) in collections.iter().enumerate() {
                    let ch = 42.0;
                    if i == self.selection {
                        self.rect(ctx, mg, y, list_w, ch, 0.25, 0.15, 0.45, 1.0);
                        self.rect(ctx, mg, y, 3.0, ch, 0.48, 0.23, 0.93, 1.0);
                    } else {
                        self.rect(ctx, mg, y, list_w, ch, 0.10, 0.10, 0.22, 1.0);
                    }
                    self.text(ctx, mg + 16.0, y + 6.0, &c.name, 9.0, 1.0, 1.0, 1.0);
                    self.text(ctx, mg + 16.0, y + 26.0, &format!("{} games", c.game_names.len()), 6.0, 0.5, 0.5, 0.6);
                    y += ch + 3.0;
                }
            }
            _ => {
                let count = self.manager.registry.count();
                let update_count = self.manager.available_updates().len();
                let mode = match self.display_mode { DisplayMode::List => "List", DisplayMode::Grid => "Grid" };
                let title = format!("Game Library  |  {} installed  {} | Sort: {} | {}", count,
                    if update_count > 0 { format!("| {} updates", update_count) } else { String::new() },
                    self.sort_field_name(), mode);
                self.text(ctx, 36.0, 14.0, &title, 10.0, 1.0, 1.0, 1.0);
                y += 42.0;

                // Instruction bar
                let mut instructions = "Up/Down: Browse  Enter: Launch  V: View  S: Sort  F: Fav filter  U: Updates  C: Collections  R: Refresh  Del: Uninstall  Shift+Enter: Multi  Esc: Back".to_string();
                let bulk = self.bulk_operation_label();
                if !bulk.is_empty() {
                    instructions.push_str("  |");
                    instructions.push_str(&bulk);
                }
                self.rect(ctx, mg, y, list_w, 16.0, 0.10, 0.10, 0.22, 0.7);
                self.text(ctx, 36.0, y + 2.0, &instructions, 5.0, 0.5, 0.5, 0.6);
                y += 22.0;

                // Filter indicator
                if self.filter_favorites || self.filter_updates {
                    let mut filters = Vec::new();
                    if self.filter_favorites { filters.push("★ Favorites"); }
                    if self.filter_updates { filters.push("● Updates"); }
                    let bar = format!("Filter: {}", filters.join(" | "));
                    self.rect(ctx, mg, y, list_w, 14.0, 0.15, 0.15, 0.30, 0.8);
                    self.text(ctx, 36.0, y + 1.0, &bar, 6.0, 0.7, 0.7, 0.9);
                    y += 18.0;
                }

                let games = self.current_games();
                if games.is_empty() {
                    self.text(ctx, 300.0, 280.0, "No games found", 12.0, 0.5, 0.5, 0.6);
                } else {
                    match self.display_mode {
                        DisplayMode::List => {
                            for (i, game) in games.iter().enumerate() {
                                let ch = 46.0;
                                let selected = i == self.selection;
                                let multi = self.multi_select.contains(&i);

                                if selected {
                                    self.rect(ctx, mg, y, list_w, ch, 0.25, 0.15, 0.45, 1.0);
                                    self.rect(ctx, mg, y, 3.0, ch, 0.48, 0.23, 0.93, 1.0);
                                } else if multi {
                                    self.rect(ctx, mg, y, list_w, ch, 0.20, 0.20, 0.35, 1.0);
                                } else {
                                    self.rect(ctx, mg, y, list_w, ch, 0.08, 0.08, 0.20, 1.0);
                                }

                                let is_fav = self.manager.collections.is_favorite(&game.name);
                                let has_update = self.manager.has_update(&game.name);
                                let fav = if is_fav { "★ " } else { "  " };
                                let update_badge = if has_update { " ●" } else { "" };
                                let multi_badge = if multi { " ✓" } else { "" };

                                self.text(ctx, mg + 16.0, y + 4.0,
                                    &format!("{}{}{}{}", fav, game.name, update_badge, multi_badge),
                                    9.0, 1.0, 1.0, 1.0);

                                let details = format!("v{} by {} | {} | {} | {} plays",
                                    game.version, game.author, format_file_size(game.size_bytes),
                                    format_duration(game.total_play_time_secs), game.play_count);
                                self.text(ctx, mg + 16.0, y + 26.0, &details, 6.0, 0.5, 0.5, 0.6);

                                y += ch + 2.0;
                            }
                        }
                        DisplayMode::Grid => {
                            let cols = 4;
                            let gap = 6.0;
                            let card_w = (list_w - gap * (cols as f32 - 1.0)) / cols as f32;
                            let card_h = 90.0;
                            let mut x = mg;

                            for (i, game) in games.iter().enumerate() {
                                if i > 0 && i % cols == 0 {
                                    x = mg;
                                    y += card_h + gap;
                                }

                                let selected = i == self.selection;
                                let is_fav = self.manager.collections.is_favorite(&game.name);

                                if selected {
                                    self.rect(ctx, x, y, card_w, card_h, 0.25, 0.15, 0.45, 1.0);
                                } else {
                                    self.rect(ctx, x, y, card_w, card_h, 0.08, 0.08, 0.20, 1.0);
                                }

                                let label = if is_fav { format!("★ {}", game.name) } else { game.name.clone() };
                                self.text(ctx, x + 6.0, y + 6.0, &label, 8.0, 1.0, 1.0, 1.0);
                                self.text(ctx, x + 6.0, y + 28.0, &format!("v{}", game.version), 6.0, 0.5, 0.5, 0.6);
                                self.text(ctx, x + 6.0, y + 42.0, &format!("{} plays", game.play_count), 6.0, 0.5, 0.5, 0.6);
                                self.text(ctx, x + 6.0, y + 56.0, &format_duration(game.total_play_time_secs), 6.0, 0.4, 0.4, 0.5);
                                self.text(ctx, x + 6.0, y + 70.0, &format_file_size(game.size_bytes), 6.0, 0.4, 0.4, 0.5);

                                if self.manager.has_update(&game.name) {
                                    self.rect(ctx, x + card_w - 24.0, y + 4.0, 20.0, 10.0, 0.9, 0.7, 0.2, 0.2);
                                    self.text(ctx, x + card_w - 22.0, y + 5.0, "Upd", 5.0, 0.9, 0.7, 0.2);
                                }

                                x += card_w + gap;
                            }
                            y += card_h + 6.0;
                        }
                    }
                }
            }
        }

        // Bottom bar
        let by = 600.0 - 22.0;
        self.rect(ctx, mg, by, list_w, 18.0, 0.10, 0.10, 0.22, 0.6);
        self.text(ctx, 36.0, by + 2.0,
            "Esc: Back  Enter: Launch  V: View toggle  S: Sort  F: Favs  U: Update check  C: Collections  R: Refresh  Del: Uninstall",
            5.0, 0.5, 0.5, 0.6);

        Ok(SceneAction::Continue)
    }
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{} B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else { format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)) }
}

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    if hours > 0 { format!("{}h {}m", hours, minutes) }
    else { format!("{}m", minutes) }
}
