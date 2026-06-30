use crate::scene::SceneContext;

/// Shared UI drawing helpers to eliminate duplication across scenes.
pub struct UiDraw;

impl UiDraw {
    pub fn clear(ctx: &mut SceneContext) {
        ctx.renderer.set_clear(0.05, 0.05, 0.15, 1.0);
    }

    pub fn rect(
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

    pub fn text(ctx: &mut SceneContext, x: f32, y: f32, s: &str, sz: f32, r: f32, g: f32, b: f32) {
        ctx.renderer.draw_text(x, y, s, sz, r, g, b);
    }
}
