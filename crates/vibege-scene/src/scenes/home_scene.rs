use std::path::PathBuf;
use std::sync::Arc;

use crate::input_helper::InputState;
use crate::library::manager::LibraryManager;
use crate::library::models::{CollectionKind, InstalledGame};
use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use tracing::info;

struct Section {
    label: &'static str,
    games: Vec<InstalledGame>,
}

pub struct HomeScene {
    manager: Arc<LibraryManager>,
    sections: Vec<Section>,
    section_start: Vec<usize>,
    flat_selection: usize,
    section_idx: usize,
    item_idx: usize,
    quick_action_selected: bool,
    quick_action_idx: usize,
}

impl HomeScene {
    pub fn new() -> Self {
        let backend = "http://localhost:3000/api/v1".to_string();
        let manager = Arc::new(LibraryManager::new(backend));
        Self {
            manager,
            sections: Vec::new(),
            section_start: Vec::new(),
            flat_selection: 0,
            section_idx: 0,
            item_idx: 0,
            quick_action_selected: false,
            quick_action_idx: 0,
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

    fn rebuild_sections(&mut self) {
        let games = self.manager.games();

        let recently_played_names = self.manager.history.recently_played(5);
        let recently_played: Vec<InstalledGame> = recently_played_names
            .iter()
            .filter_map(|name| games.iter().find(|g| g.name == *name).cloned())
            .collect();

        let fav_names = self.manager.collections.by_kind(CollectionKind::Favorites);
        let favourites: Vec<InstalledGame> = fav_names
            .iter()
            .filter_map(|name| games.iter().find(|g| g.name == *name).cloned())
            .collect();

        self.sections = Vec::new();

        if !recently_played.is_empty() {
            self.sections.push(Section {
                label: "Recently Played",
                games: recently_played,
            });
        }

        if !favourites.is_empty() {
            self.sections.push(Section {
                label: "Favourites",
                games: favourites,
            });
        }

        if !games.is_empty() {
            self.sections.push(Section {
                label: "All Games",
                games: games.clone(),
            });
        }

        if self.sections.is_empty() {
            let demos = vec![
                InstalledGame::new("Pong".into(), PathBuf::from("demo")),
                InstalledGame::new("Void Drifter".into(), PathBuf::from("demo")),
            ];
            self.sections.push(Section {
                label: "Demo Games",
                games: demos,
            });
        }

        self.section_start.clear();
        let mut acc = 0;
        for s in &self.sections {
            self.section_start.push(acc);
            acc += s.games.len();
        }
        self.flat_selection = self.flat_selection.min(acc.saturating_sub(1));
        self.resolve_selection();
    }

    fn resolve_selection(&mut self) {
        if self.quick_action_selected {
            return;
        }
        let mut remaining = self.flat_selection;
        for (i, s) in self.sections.iter().enumerate() {
            if remaining < s.games.len() {
                self.section_idx = i;
                self.item_idx = remaining;
                return;
            }
            remaining = remaining.saturating_sub(s.games.len());
        }
        self.section_idx = self.sections.len().saturating_sub(1);
        self.item_idx = self
            .sections
            .last()
            .map(|s| s.games.len().saturating_sub(1))
            .unwrap_or(0);
    }

    fn total_game_count(&self) -> usize {
        self.sections.iter().map(|s| s.games.len()).sum()
    }

    fn launch(&self, ctx: &mut SceneContext, idx: usize, path: &str) -> SceneResult {
        if path == "demo" || path.is_empty() {
            let source = include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../resources/demo-game.lua"
            ));
            let gs = Box::new(super::game_scene::GameScene::new(
                source.to_string(),
                "Demo".into(),
                ctx.screen_width,
                ctx.screen_height,
            ));
            return Ok(SceneAction::Push(gs));
        }
        let p = PathBuf::from(path);
        if p.exists() {
            match std::fs::read_to_string(&p) {
                Ok(source) => {
                    let name = self
                        .sections
                        .get(self.section_idx)
                        .and_then(|s| s.games.get(idx))
                        .map(|g| g.name.clone())
                        .unwrap_or_else(|| "Game".into());
                    let gs = Box::new(super::game_scene::GameScene::new(
                        source,
                        name,
                        ctx.screen_width,
                        ctx.screen_height,
                    ));
                    Ok(SceneAction::Push(gs))
                }
                Err(e) => {
                    info!("Failed to read game file: {e}");
                    Ok(SceneAction::Continue)
                }
            }
        } else {
            info!("Game file not found: {}", p.display());
            Ok(SceneAction::Continue)
        }
    }
}

impl Scene for HomeScene {
    fn id(&self) -> SceneId {
        SceneId::Home
    }

    fn on_create(&mut self, ctx: &mut SceneContext) -> SceneResult {
        info!("HomeScene: started");
        ctx.input.lock().expect("lock").end_frame();
        self.manager.initialize();
        self.rebuild_sections();
        info!(
            sections = self.sections.len(),
            games = self.total_game_count(),
            "HomeScene: data loaded"
        );
        Ok(SceneAction::Continue)
    }

    fn on_resume(&mut self, ctx: &mut SceneContext) -> SceneResult {
        ctx.input.lock().expect("lock").end_frame();
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        let inp = InputState::new(
            &ctx.input,
            &[
                "up", "down", "left", "right", "enter", "space", "escape", "s", "l", "o", "f",
            ],
        );

        if inp.pressed(6) {
            if self.quick_action_selected {
                self.quick_action_selected = false;
            }
            return Ok(SceneAction::Pop);
        }

        if inp.pressed(7) {
            return Ok(SceneAction::Push(Box::new(
                super::settings_scene::SettingsScene::new(),
            )));
        }
        if inp.pressed(8) {
            let backend = ctx.config.get().general.backend_url;
            return Ok(SceneAction::Push(Box::new(
                super::library_scene::LibraryScene::new(backend),
            )));
        }
        if inp.pressed(9) {
            let backend = ctx.config.get().general.backend_url;
            return Ok(SceneAction::Push(Box::new(
                super::store_scene::StoreScene::new(backend),
            )));
        }

        if inp.pressed(3) || inp.pressed(10) {
            if self.quick_action_selected {
                self.quick_action_selected = false;
            }
        }

        if inp.pressed(2) || inp.pressed(3) {
            if !self.quick_action_selected {
                let total = self.total_game_count();
                if inp.pressed(2) {
                    self.quick_action_selected = true;
                    self.quick_action_idx = 0;
                    return Ok(SceneAction::Continue);
                }
                if self.quick_action_selected {
                    return Ok(SceneAction::Continue);
                }
                if total > 0 {
                    if let Some(s) = self.sections.get(self.section_idx) {
                        if let Some(game) = s.games.get(self.item_idx) {
                            return self.launch(ctx, self.item_idx, &game.path.to_string_lossy());
                        }
                    }
                }
                return Ok(SceneAction::Continue);
            }
            if inp.pressed(3) {
                match self.quick_action_idx {
                    0 => {
                        return Ok(SceneAction::Push(Box::new(
                            super::settings_scene::SettingsScene::new(),
                        )));
                    }
                    1 => {
                        let backend = ctx.config.get().general.backend_url;
                        return Ok(SceneAction::Push(Box::new(
                            super::library_scene::LibraryScene::new(backend),
                        )));
                    }
                    2 => {
                        let backend = ctx.config.get().general.backend_url;
                        return Ok(SceneAction::Push(Box::new(
                            super::store_scene::StoreScene::new(backend),
                        )));
                    }
                    _ => {}
                }
            }
        }

        if inp.pressed(0) {
            if self.quick_action_selected {
                if self.quick_action_idx > 0 {
                    self.quick_action_idx -= 1;
                }
            } else {
                let total = self.total_game_count();
                if total > 0 && self.flat_selection > 0 {
                    self.flat_selection -= 1;
                    self.resolve_selection();
                } else if self.flat_selection == 0 {
                    self.quick_action_selected = true;
                    self.quick_action_idx = 0;
                }
            }
        }

        if inp.pressed(1) {
            if self.quick_action_selected {
                if self.quick_action_idx < 2 {
                    self.quick_action_idx += 1;
                } else {
                    self.quick_action_selected = false;
                    self.flat_selection = 0;
                    self.resolve_selection();
                }
            } else {
                let total = self.total_game_count();
                if self.flat_selection + 1 < total {
                    self.flat_selection += 1;
                    self.resolve_selection();
                }
            }
        }

        Ok(SceneAction::Continue)
    }

    fn on_render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        self.clear(ctx);

        let mg = 24.0;
        let list_w = 800.0 - mg * 2.0;
        let mut y = 0.0;

        // Title
        self.rect(ctx, mg, y, list_w, 40.0, 0.48, 0.23, 0.93, 1.0);
        self.text(ctx, mg + 12.0, 10.0, "VibeGE", 14.0, 1.0, 1.0, 1.0);
        let play_count = self.manager.history.all().len();
        self.text(
            ctx,
            mg + list_w - 120.0,
            13.0,
            &format!("{} sessions", play_count),
            7.0,
            0.9,
            0.9,
            1.0,
        );
        y += 46.0;

        // Quick actions row
        let quick_actions = ["[S] Settings", "[L] Library", "[O] Store"];
        let qa_w = (list_w - 8.0) / 3.0;
        for (i, label) in quick_actions.iter().enumerate() {
            let qx = mg + i as f32 * (qa_w + 4.0);
            let sel = self.quick_action_selected && self.quick_action_idx == i;
            self.rect(
                ctx,
                qx,
                y,
                qa_w,
                28.0,
                if sel { 0.35 } else { 0.12 },
                if sel { 0.20 } else { 0.12 },
                if sel { 0.55 } else { 0.25 },
                1.0,
            );
            if sel {
                self.rect(ctx, qx, y, 2.0, 28.0, 0.48, 0.23, 0.93, 1.0);
            }
            self.text(ctx, qx + 10.0, y + 7.0, label, 8.0, 1.0, 1.0, 1.0);
        }
        y += 34.0;

        // Tip
        self.rect(ctx, mg, y, list_w, 16.0, 0.07, 0.07, 0.18, 0.6);
        self.text(
            ctx,
            mg + 8.0,
            y + 2.0,
            "Arrows: Navigate     Enter: Launch/Open     Esc: Exit     F: Toggle quick actions",
            6.0,
            0.45,
            0.45,
            0.55,
        );
        y += 22.0;

        // Section cards
        for (si, section) in self.sections.iter().enumerate() {
            let section_sel = si == self.section_idx && !self.quick_action_selected;

            // Section header
            self.rect(ctx, mg, y, list_w, 22.0, 0.15, 0.15, 0.30, 1.0);
            self.text(ctx, mg + 10.0, y + 4.0, section.label, 8.0, 0.7, 0.7, 0.9);
            self.text(
                ctx,
                mg + list_w - 40.0,
                y + 4.0,
                &format!("{}", section.games.len()),
                7.0,
                0.45,
                0.45,
                0.55,
            );
            y += 26.0;

            for (gi, game) in section.games.iter().enumerate() {
                let selected = section_sel && gi == self.item_idx;
                let ch = 48.0;

                if selected {
                    self.rect(ctx, mg, y, list_w, ch, 0.25, 0.15, 0.45, 1.0);
                    self.rect(ctx, mg, y, 3.0, ch, 0.48, 0.23, 0.93, 1.0);
                } else {
                    self.rect(ctx, mg, y, list_w, ch, 0.08, 0.08, 0.20, 1.0);
                }

                self.text(ctx, mg + 14.0, y + 6.0, &game.name, 9.0, 1.0, 1.0, 1.0);
                if !game.description.is_empty() {
                    self.text(
                        ctx,
                        mg + 14.0,
                        y + 26.0,
                        &game.description,
                        7.0,
                        0.5,
                        0.5,
                        0.6,
                    );
                }

                // Info badges
                let mut badge_x = mg + list_w - 10.0;
                if game.pinned {
                    badge_x -= 50.0;
                    self.rect(ctx, badge_x, y + 6.0, 42.0, 14.0, 0.9, 0.7, 0.2, 0.15);
                    self.text(ctx, badge_x + 4.0, y + 7.0, "PINNED", 6.0, 0.9, 0.7, 0.2);
                }
                if game.total_play_time_secs > 0 {
                    let label = format!("{}m", game.total_play_time_secs / 60);
                    badge_x -= label.len() as f32 * 7.0 + 10.0;
                    self.rect(
                        ctx,
                        badge_x,
                        y + 6.0,
                        label.len() as f32 * 7.0 + 6.0,
                        14.0,
                        0.2,
                        0.3,
                        0.6,
                        0.15,
                    );
                    self.text(ctx, badge_x + 3.0, y + 7.0, &label, 6.0, 0.4, 0.5, 0.8);
                }
                if game.play_count > 0 {
                    let label = format!("{}x", game.play_count);
                    badge_x -= label.len() as f32 * 7.0 + 10.0;
                    self.rect(
                        ctx,
                        badge_x,
                        y + 6.0,
                        label.len() as f32 * 7.0 + 6.0,
                        14.0,
                        0.3,
                        0.5,
                        0.3,
                        0.15,
                    );
                    self.text(ctx, badge_x + 3.0, y + 7.0, &label, 6.0, 0.3, 0.7, 0.4);
                }

                y += ch + 2.0;
            }

            y += 6.0;
        }

        Ok(SceneAction::Continue)
    }
}
