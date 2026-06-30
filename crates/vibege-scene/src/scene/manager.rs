use super::{Scene, SceneAction, SceneContext, SceneResult};
use tracing::info;

pub struct SceneManager {
    stack: Vec<Box<dyn Scene>>,
}

impl SceneManager {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn push(&mut self, mut scene: Box<dyn Scene>, ctx: &mut SceneContext) -> SceneResult {
        if let Some(current) = self.stack.last_mut() {
            current.on_suspend(ctx)?;
        }
        scene.on_create(ctx)?;
        let action = scene.on_enter(ctx)?;
        self.stack.push(scene);

        // If on_enter requested a transition, process it now
        match action {
            SceneAction::Continue => Ok(SceneAction::Continue),
            SceneAction::Replace(s) => {
                self.stack.pop();
                self.push(s, ctx)
            }
            SceneAction::Push(s) => self.push(s, ctx),
            SceneAction::Pop => self.pop(ctx),
            SceneAction::PopToRoot(s) => self.pop_to_root(s, ctx),
            SceneAction::Exit => {
                self.stack.pop();
                Ok(SceneAction::Exit)
            }
        }
    }

    pub fn pop(&mut self, ctx: &mut SceneContext) -> SceneResult {
        if let Some(mut exiting) = self.stack.pop() {
            exiting.on_exit(ctx)?;
            exiting.on_destroy(ctx);
        }
        if let Some(previous) = self.stack.last_mut() {
            previous.on_resume(ctx)?;
        }
        Ok(SceneAction::Continue)
    }

    pub fn replace(&mut self, mut scene: Box<dyn Scene>, ctx: &mut SceneContext) -> SceneResult {
        if let Some(mut exiting) = self.stack.pop() {
            exiting.on_exit(ctx)?;
            exiting.on_destroy(ctx);
        }
        scene.on_create(ctx)?;
        scene.on_enter(ctx)?;
        self.stack.push(scene);
        Ok(SceneAction::Continue)
    }

    pub fn pop_to_root(
        &mut self,
        mut scene: Box<dyn Scene>,
        ctx: &mut SceneContext,
    ) -> SceneResult {
        while self.stack.len() > 1 {
            if let Some(mut exiting) = self.stack.pop() {
                exiting.on_exit(ctx)?;
                exiting.on_destroy(ctx);
            }
        }
        if let Some(mut exiting) = self.stack.pop() {
            exiting.on_exit(ctx)?;
            exiting.on_destroy(ctx);
        }
        scene.on_create(ctx)?;
        scene.on_enter(ctx)?;
        self.stack.push(scene);
        Ok(SceneAction::Continue)
    }

    pub fn update(&mut self, ctx: &mut SceneContext, dt: f64) -> SceneResult {
        let Some(scene) = self.stack.last_mut() else {
            return Ok(SceneAction::Exit);
        };
        scene.on_update(ctx, dt)
    }

    pub fn render(&mut self, ctx: &mut SceneContext) -> SceneResult {
        let Some(scene) = self.stack.last_mut() else {
            return Ok(SceneAction::Exit);
        };
        scene.on_render(ctx)
    }

    pub fn apply(&mut self, action: SceneAction, ctx: &mut SceneContext) -> SceneResult {
        match action {
            SceneAction::Continue => Ok(SceneAction::Continue),
            SceneAction::Push(s) => self.push(s, ctx),
            SceneAction::Replace(s) => self.replace(s, ctx),
            SceneAction::Pop => self.pop(ctx),
            SceneAction::PopToRoot(s) => self.pop_to_root(s, ctx),
            SceneAction::Exit => {
                info!("Scene requested exit");
                Ok(SceneAction::Exit)
            }
        }
    }

    pub fn active(&self) -> Option<&dyn Scene> {
        self.stack.last().map(|s| s.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn shutdown(&mut self, ctx: &mut SceneContext) {
        while let Some(mut scene) = self.stack.pop() {
            scene.on_exit(ctx).ok();
            scene.on_destroy(ctx);
        }
    }
}
