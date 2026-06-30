use std::sync::Arc;

use crate::input_helper::InputState;
use crate::scene::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use crate::store::manager::StoreManager;
use crate::store::models::{SearchQuery, SortField};
use tracing::info;

pub struct StoreScene {
    manager: Arc<StoreManager>,
    selection: usize,
    section_selection: usize,
    search_text: String,
    search_mode: bool,
    search_cursor: usize,
    #[allow(dead_code)]
    page: u32,
    active_section: usize,
    show_sections: bool,
}

impl StoreScene {
    pub fn new(backend: String) -> Self {
        Self {
            manager: Arc::new(StoreManager::new(backend)),
            selection: 0,
            section_selection: 0,
            search_text: String::new(),
            search_mode: false,
            search_cursor: 0,
            page: 0,
            active_section: 0,
            show_sections: true,
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
        info!("StoreScene: fetching from {}", self.manager.backend_url());
        self.manager.fetch(0);
        Ok(SceneAction::Continue)
    }

    fn on_update(&mut self, ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        if self.manager.loading() {
            return Ok(SceneAction::Continue);
        }

        let inp = InputState::new(
            &ctx.input,
            &[
                "up", "down", "enter", "escape", "s", "r", "left", "right", "f5",
            ],
        );

        if inp.pressed(4) && !self.search_mode {
            return Ok(SceneAction::Pop);
        }

        if inp.pressed(4) /* esc */ && self.search_mode {
            self.search_mode = false;
            self.search_text.clear();
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(5) /* s */ && !self.search_mode {
            self.search_mode = true;
            self.search_cursor = 0;
            self.show_sections = false;
            return Ok(SceneAction::Continue);
        }

        if inp.pressed(6)
        /* r */
        {
            self.manager.refresh();
            return Ok(SceneAction::Continue);
        }

        if self.search_mode {
            if inp.pressed(0)
            /* up */
            {
                let c = self.search_text.chars().last().unwrap_or('a');
                let next = match c {
                    'a'..='y' => ((c as u8) + 1) as char,
                    'z' => ' ',
                    ' ' => 'a',
                    _ => 'a',
                };
                if self.search_cursor == 0 {
                    self.search_text = next.to_string();
                } else {
                    self.search_text.pop();
                    self.search_text.push(next);
                }
            }
            if inp.pressed(1)
            /* down */
            {
                let c = self.search_text.chars().last().unwrap_or('a');
                let prev = match c {
                    'b'..='z' => ((c as u8) - 1) as char,
                    'a' => ' ',
                    ' ' => 'z',
                    _ => 'a',
                };
                if self.search_cursor == 0 {
                    self.search_text = prev.to_string();
                } else {
                    self.search_text.pop();
                    self.search_text.push(prev);
                }
            }
            if inp.pressed(2) && !self.search_text.is_empty() {
                let q = SearchQuery {
                    text: self.search_text.clone(),
                    sort_by: SortField::Relevance,
                    ..Default::default()
                };
                let _results = self.manager.search(&q);
                self.show_sections = false;
            }
            return Ok(SceneAction::Continue);
        }

        // Section view
        if self.show_sections {
            let sections = self.manager.sections();
            if !sections.is_empty() {
                if inp.pressed(0) /* up */ && self.section_selection > 0 {
                    self.section_selection -= 1;
                }
                if inp.pressed(1) /* down */ && self.section_selection + 1 < sections.len() {
                    self.section_selection += 1;
                }
                if inp.pressed(2)
                /* enter */
                {
                    self.active_section = self.section_selection;
                    self.show_sections = false;
                }
            }
        } else {
            // Game list view
            let games = self.listings_for_current_view();
            if inp.pressed(3) /* left */ && self.show_sections {
                self.show_sections = true;
            }

            if games.is_empty() {
                return Ok(SceneAction::Continue);
            }

            if inp.pressed(0) && self.selection > 0 {
                self.selection -= 1;
            }
            if inp.pressed(1) && self.selection + 1 < games.len() {
                self.selection += 1;
            }

            if inp.pressed(2) {
                if let Some(game) = games.get(self.selection) {
                    info!("Store: installing {} ({})", game.name, game.id);
                    match self.manager.download_package(&game.id) {
                        Ok(data) => {
                            if let Err(e) = self.manager.install_package(&data, &game.name) {
                                info!("Install failed: {e}");
                            } else {
                                info!("Installed: {}", game.name);
                            }
                        }
                        Err(e) => info!("Download failed: {e}"),
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
        if self.search_mode {
            self.text(
                ctx,
                42.0,
                12.0,
                &format!("Search: {}", self.search_text),
                14.0,
                1.0,
                1.0,
                1.0,
            );
        } else {
            self.text(ctx, 42.0, 12.0, "Game Store", 14.0, 1.0, 1.0, 1.0);
        }

        if self.manager.loading() {
            self.text(ctx, 350.0, 290.0, "Loading...", 10.0, 0.5, 0.5, 0.6);
            return Ok(SceneAction::Continue);
        }

        if let Some(ref err) = self.manager.error() {
            self.text(ctx, 300.0, 280.0, "Store unavailable", 10.0, 0.9, 0.3, 0.3);
            self.text(ctx, 260.0, 310.0, err, 7.0, 0.5, 0.5, 0.6);
            self.text(ctx, 280.0, 340.0, "Press R to retry", 8.0, 0.5, 0.5, 0.6);
            return Ok(SceneAction::Continue);
        }

        // Instruction bar
        self.rect(ctx, 30.0, 48.0, 740.0, 18.0, 0.10, 0.10, 0.22, 0.7);
        let instructions = if self.search_mode {
            "Up/Down: Cycle letters     Enter: Search     Esc: Cancel"
        } else if self.show_sections {
            "Up/Down: Browse sections     Enter: View     S: Search     R: Refresh     Esc: Back"
        } else {
            "Up/Down: Browse     Enter: Install     S: Search     R: Refresh     Esc: Back"
        };
        self.text(ctx, 42.0, 51.0, instructions, 7.0, 0.5, 0.5, 0.6);

        // Section browsing view
        if self.show_sections {
            let sections = self.manager.sections();
            if sections.is_empty() {
                self.text(ctx, 320.0, 280.0, "No games found", 10.0, 0.5, 0.5, 0.6);
                self.text(
                    ctx,
                    260.0,
                    310.0,
                    "Check backend is running",
                    8.0,
                    0.5,
                    0.5,
                    0.6,
                );
            } else {
                let mut y = 76.0;
                for (i, section) in sections.iter().enumerate() {
                    let card_h = 52.0;
                    if i == self.section_selection {
                        self.rect(ctx, 30.0, y, 740.0, card_h, 0.25, 0.15, 0.45, 1.0);
                        self.rect(ctx, 30.0, y, 3.0, card_h, 0.48, 0.23, 0.93, 1.0);
                    } else {
                        self.rect(ctx, 30.0, y, 740.0, card_h, 0.10, 0.10, 0.22, 1.0);
                    }

                    self.text(ctx, 46.0, y + 6.0, &section.title, 10.0, 1.0, 1.0, 1.0);
                    let preview: String = section
                        .games
                        .iter()
                        .take(3)
                        .map(|g| g.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.text(ctx, 46.0, y + 26.0, &preview, 7.0, 0.5, 0.5, 0.6);
                    self.text(
                        ctx,
                        680.0,
                        y + 6.0,
                        &format!("{} games", section.games.len()),
                        7.0,
                        0.5,
                        0.5,
                        0.6,
                    );

                    y += card_h + 4.0;
                }
            }
        } else {
            // Game list view
            let games = self.listings_for_current_view();
            if games.is_empty() {
                self.text(ctx, 320.0, 280.0, "No games found", 10.0, 0.5, 0.5, 0.6);
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

                    self.text(ctx, 46.0, y + 6.0, &game.name, 10.0, 1.0, 1.0, 1.0);
                    self.text(ctx, 46.0, y + 26.0, &game.description, 7.0, 0.5, 0.5, 0.6);

                    // File size or download count
                    let info_text = format!("{} dl", game.downloads);
                    self.text(ctx, 680.0, y + 26.0, &info_text, 7.0, 0.5, 0.5, 0.6);

                    // Status badge
                    if game.status == "approved" {
                        self.rect(ctx, 680.0, y + 4.0, 50.0, 14.0, 0.2, 0.8, 0.4, 0.2);
                        self.text(ctx, 686.0, y + 5.0, "LIVE", 7.0, 0.2, 0.8, 0.4);
                    }

                    y += card_h + 4.0;
                }
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

impl StoreScene {
    /// Get the games to show in the current view.
    fn listings_for_current_view(&self) -> Vec<crate::store::models::GameListing> {
        let sections = self.manager.sections();
        if self.search_mode {
            let q = SearchQuery {
                text: self.search_text.clone(),
                ..Default::default()
            };
            return self.manager.search(&q);
        }
        if !self.show_sections && !sections.is_empty() && self.active_section < sections.len() {
            return sections[self.active_section].games.clone();
        }
        self.manager.listings()
    }
}
