// ─── Scene Manager Tests ────────────────────────────────────────────

#![allow(deprecated)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::kind::SceneKind;
use super::message::SceneMessage;
use super::{Scene, SceneAction, SceneContext, SceneId, SceneResult};
use crate::scene::manager::SceneManager;

// ─── Global GPU harness (initialised once, shared across threads) ───

struct SharedContext {
    renderer: Arc<vibege_renderer::Renderer>,
    input: Arc<Mutex<vibege_input::InputManager>>,
    config: Arc<vibege_config::ConfigHandle>,
    assets: Arc<vibege_asset::AssetManager>,
}

static SHARED: OnceLock<SharedContext> = OnceLock::new();

fn ctx_new() -> SceneContext {
    let shared = SHARED.get_or_init(|| {
        let (event_loop, window) = create_window();

        let renderer = pollster::block_on(vibege_renderer::Renderer::new(
            Arc::clone(&window),
            800,
            600,
        ))
        .expect("Renderer initialisation — requires a GPU");

        // Leak event_loop to prevent its Drop from conflicting with
        // wgpu's TLS destructor ordering.
        Box::leak(Box::new(event_loop));

        SharedContext {
            renderer: Arc::new(renderer),
            input: Arc::new(Mutex::new(vibege_input::InputManager::new())),
            config: Arc::new(vibege_config::ConfigHandle::new()),
            assets: Arc::new(vibege_asset::AssetManager::new()),
        }
    });

    SceneContext::new(
        800,
        600,
        Arc::clone(&shared.renderer),
        Arc::clone(&shared.input),
        Arc::clone(&shared.config),
        None,
        None,
        Arc::clone(&shared.assets),
        None,
    )
}

fn create_window() -> (winit::event_loop::EventLoop<()>, Arc<winit::window::Window>) {
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let mut builder = winit::event_loop::EventLoop::builder();
        builder.with_any_thread(true);
        let el = builder.build().expect("EventLoop (any_thread)");
        let w = Arc::new(
            el.create_window(
                winit::window::WindowAttributes::default()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0)),
            )
            .expect("Test window"),
        );
        (el, w)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "linux")]
        {
            use winit::platform::x11::EventLoopBuilderExtX11;
            let mut builder = winit::event_loop::EventLoop::builder();
            builder.with_any_thread(true);
            let el = builder.build().expect("EventLoop (any_thread)");
            let w = Arc::new(
                el.create_window(
                    winit::window::WindowAttributes::default()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0)),
                )
                .expect("Test window"),
            );
            return (el, w);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let el = winit::event_loop::EventLoop::builder()
                .build()
                .expect("EventLoop");
            let w = Arc::new(
                el.create_window(
                    winit::window::WindowAttributes::default()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0)),
                )
                .expect("Test window"),
            );
            (el, w)
        }
    }
}

fn with_mgr<F, T>(f: F) -> T
where
    F: FnOnce(&mut SceneManager, &mut SceneContext) -> T,
{
    let mut ctx = ctx_new();
    let mut mgr = SceneManager::new();
    f(&mut mgr, &mut ctx)
}

// ─── Mock Scene ──────────────────────────────────────────────────────

// Each test thread gets its own log so parallel execution doesn't interleave.
thread_local! {
    static LIFECYCLE_LOG: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

fn reset_log() {
    LIFECYCLE_LOG.with(|l| l.borrow_mut().clear());
}

fn log(msg: String) {
    LIFECYCLE_LOG.with(|l| l.borrow_mut().push(msg));
}

fn peek_log() -> Vec<String> {
    LIFECYCLE_LOG.with(|l| l.borrow().clone())
}

struct TestScene {
    id: SceneId,
    kind: SceneKind,
    uid: usize,
    fail_on: Vec<&'static str>,
    state: Option<String>,
}

impl TestScene {
    fn new(id: SceneId) -> Self {
        Self {
            id,
            kind: SceneKind::Normal,
            uid: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            fail_on: Vec::new(),
            state: None,
        }
    }

    fn with_kind(mut self, kind: SceneKind) -> Self {
        self.kind = kind;
        self
    }

    fn failing(mut self, method: &'static str) -> Self {
        self.fail_on.push(method);
        self
    }

    fn with_state(mut self, data: &str) -> Self {
        self.state = Some(data.to_string());
        self
    }

    fn name(&self) -> String {
        format!("{:?}#{}", self.id, self.uid)
    }

    fn check_fail(&self, method: &str) -> SceneResult {
        if self.fail_on.contains(&method) {
            Err(format!("{} failed on {}", self.name(), method))
        } else {
            Ok(SceneAction::Continue)
        }
    }
}

impl Scene for TestScene {
    fn id(&self) -> SceneId {
        self.id.clone()
    }

    fn kind(&self) -> SceneKind {
        self.kind
    }

    fn on_create(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        log(format!("{}:on_create", self.name()));
        self.check_fail("create")
    }

    fn on_enter(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        log(format!("{}:on_enter", self.name()));
        self.check_fail("enter")
    }

    fn on_activate(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        log(format!("{}:on_activate", self.name()));
        self.check_fail("activate")
    }

    fn on_suspend(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        log(format!("{}:on_suspend", self.name()));
        self.check_fail("suspend")
    }

    fn on_resume(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        log(format!("{}:on_resume", self.name()));
        self.check_fail("resume")
    }

    fn on_deactivate(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        log(format!("{}:on_deactivate", self.name()));
        self.check_fail("deactivate")
    }

    fn on_exit(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        log(format!("{}:on_exit", self.name()));
        self.check_fail("exit")
    }

    fn on_destroy(&mut self, _ctx: &mut SceneContext) {
        log(format!("{}:on_destroy", self.name()));
    }

    fn on_update(&mut self, _ctx: &mut SceneContext, _dt: f64) -> SceneResult {
        log(format!("{}:on_update", self.name()));
        self.check_fail("update")
    }

    fn on_render(&mut self, _ctx: &mut SceneContext) -> SceneResult {
        log(format!("{}:on_render", self.name()));
        self.check_fail("render")
    }

    fn on_message(&mut self, _ctx: &mut SceneContext, _msg: &SceneMessage) -> SceneResult {
        log(format!("{}:on_message", self.name()));
        self.check_fail("message")
    }

    fn save_state(&self) -> Option<String> {
        self.state.clone()
    }

    fn restore_state(&mut self, data: &str) -> Result<(), String> {
        log(format!("{}:restore_state({})", self.name(), data));
        self.state = Some(data.to_string());
        Ok(())
    }
}

// ─── Assertion helpers ───────────────────────────────────────────────

// Check if ANY log entry contains the given suffix (ignores #uid prefix).
fn assert_log_has(log: &[String], suffix: &str) {
    assert!(
        log.iter()
            .any(|l| l.ends_with(suffix) || l.contains(suffix)),
        "Expected log containing '{suffix}', got: {log:?}"
    );
}

fn assert_log_not_has(log: &[String], suffix: &str) {
    assert!(
        !log.iter()
            .any(|l| l.ends_with(suffix) || l.contains(suffix)),
        "Expected log NOT containing '{suffix}', got: {log:?}"
    );
}

// ─── Tests ───────────────────────────────────────────────────────────

#[test]
fn test_stack_push_and_depth() {
    with_mgr(|mgr, ctx| {
        assert!(mgr.is_empty());
        assert_eq!(mgr.depth(), 0);
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        assert!(!mgr.is_empty());
        assert_eq!(mgr.depth(), 1);
        mgr.push(Box::new(TestScene::new(SceneId::Settings)), ctx);
        assert_eq!(mgr.depth(), 2);
    });
}

#[test]
fn test_pop_decreases_depth() {
    with_mgr(|mgr, ctx| {
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Settings)), ctx);
        assert_eq!(mgr.depth(), 2);
        mgr.pop(ctx);
        assert_eq!(mgr.depth(), 1);
        mgr.pop(ctx);
        assert_eq!(mgr.depth(), 0);
        assert!(mgr.is_empty());
    });
}

#[test]
fn test_replace_replaces_top() {
    with_mgr(|mgr, ctx| {
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Settings)), ctx);
        assert_eq!(mgr.depth(), 2);
        mgr.replace(Box::new(TestScene::new(SceneId::Store)), ctx);
        assert_eq!(mgr.depth(), 2);
        assert_eq!(mgr.active().unwrap().id(), SceneId::Store);
    });
}

#[test]
fn test_lifecycle_ordering_push_pop() {
    with_mgr(|mgr, ctx| {
        reset_log();
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        let log1 = peek_log();
        assert_log_has(&log1, ":on_create");
        assert_log_has(&log1, ":on_enter");
        assert_log_has(&log1, ":on_activate");

        reset_log();
        mgr.push(Box::new(TestScene::new(SceneId::Settings)), ctx);
        let log2 = peek_log();
        assert_log_has(&log2, ":on_deactivate");
        assert_log_has(&log2, ":on_suspend");
        assert_log_has(&log2, ":on_create");
        assert_log_has(&log2, ":on_activate");

        reset_log();
        mgr.pop(ctx);
        let log3 = peek_log();
        assert_log_has(&log3, ":on_deactivate");
        assert_log_has(&log3, ":on_exit");
        assert_log_has(&log3, ":on_resume");
        assert_log_has(&log3, ":on_activate");
    });
}

#[test]
fn test_overlay_does_not_suspend_below() {
    with_mgr(|mgr, ctx| {
        reset_log();
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);

        reset_log();
        mgr.push_overlay(
            Box::new(TestScene::new(SceneId::Pause).with_kind(SceneKind::Overlay)),
            ctx,
        );
        let log = peek_log();
        assert_log_not_has(&log, ":on_suspend");
        assert_log_not_has(&log, ":on_deactivate");
        assert_log_has(&log, ":on_create");

        reset_log();
        mgr.pop_overlay(ctx);
        let log2 = peek_log();
        assert_log_has(&log2, ":on_exit");
        assert_log_not_has(&log2, ":on_resume");
        assert_log_not_has(&log2, ":on_activate");
    });
}

#[test]
fn test_has_modal() {
    with_mgr(|mgr, ctx| {
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        assert!(!mgr.has_modal());
        mgr.push_modal(
            Box::new(TestScene::new(SceneId::Pause).with_kind(SceneKind::Modal)),
            ctx,
        );
        assert!(mgr.has_modal());
        mgr.pop_modal(ctx);
        assert!(!mgr.has_modal());
    });
}

#[test]
fn test_persistent_survives_pop() {
    with_mgr(|mgr, ctx| {
        mgr.push_persistent(
            Box::new(TestScene::new(SceneId::Downloads).with_kind(SceneKind::Persistent)),
            ctx,
        );
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        assert_eq!(mgr.persistent_count(), 1);
        assert_eq!(mgr.depth(), 1);
        mgr.pop(ctx);
        assert_eq!(
            mgr.persistent_count(),
            1,
            "Persistent scene should survive pop"
        );
        assert_eq!(mgr.depth(), 0);
    });
}

#[test]
fn test_persistent_updates_every_frame() {
    with_mgr(|mgr, ctx| {
        mgr.push_persistent(
            Box::new(TestScene::new(SceneId::Downloads).with_kind(SceneKind::Persistent)),
            ctx,
        );
        reset_log();
        let _ = mgr.update(ctx, 0.016);
        let log = peek_log();
        assert_log_has(&log, ":on_update");
    });
}

#[test]
fn test_state_persistence() {
    with_mgr(|mgr, ctx| {
        reset_log();
        let scene = TestScene::new(SceneId::Settings).with_state(r#"{"volume": 0.8}"#);
        mgr.push(Box::new(scene), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        reset_log();
        mgr.pop(ctx);
        let log = peek_log();
        assert_log_has(&log, "restore_state");
    });
}

#[test]
fn test_error_fallback_shows_modal() {
    with_mgr(|mgr, ctx| {
        mgr.set_error_fallback(Box::new(
            TestScene::new(SceneId::Error).with_kind(SceneKind::Modal),
        ));
        reset_log();
        let failing = TestScene::new(SceneId::Game).failing("create");
        mgr.push(Box::new(failing), ctx);
        let _ = mgr.process_pending(ctx);
        assert!(mgr.has_modal(), "Error fallback should be shown as modal");
        assert_eq!(
            mgr.active().unwrap().id(),
            SceneId::Error,
            "Active scene should be error fallback"
        );
    });
}

#[test]
fn test_message_routing() {
    with_mgr(|mgr, ctx| {
        reset_log();
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Settings)), ctx);
        reset_log();
        let msg = SceneMessage::custom("test_event", "hello");
        mgr.broadcast(&msg, ctx);
        let log = peek_log();
        assert_log_has(&log, ":on_message");
    });
}

#[test]
fn test_shutdown_cleans_up_all_scenes() {
    with_mgr(|mgr, ctx| {
        reset_log();
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Settings)), ctx);
        mgr.push_overlay(
            Box::new(TestScene::new(SceneId::Pause).with_kind(SceneKind::Overlay)),
            ctx,
        );
        reset_log();
        mgr.shutdown(ctx);
        let log = peek_log();
        let destroy_count = log.iter().filter(|l| l.contains("on_destroy")).count();
        assert_eq!(
            destroy_count, 3,
            "All 3 scenes should be destroyed, got {log:?}"
        );
    });
}

#[test]
fn test_pop_to_root_clears_to_single() {
    with_mgr(|mgr, ctx| {
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Library)), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Settings)), ctx);
        assert_eq!(mgr.depth(), 3);
        mgr.pop_to_root(Box::new(TestScene::new(SceneId::Home)), ctx);
        assert_eq!(mgr.depth(), 1);
        assert_eq!(mgr.active().unwrap().id(), SceneId::Home);
    });
}

#[test]
fn test_pop_to_depth() {
    with_mgr(|mgr, ctx| {
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Library)), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Settings)), ctx);
        mgr.push(Box::new(TestScene::new(SceneId::Store)), ctx);
        assert_eq!(mgr.depth(), 4);
        mgr.apply(SceneAction::PopTo(2), ctx).unwrap();
        assert_eq!(mgr.depth(), 2);
    });
}

#[test]
fn test_empty_pop_is_noop() {
    with_mgr(|mgr, ctx| {
        mgr.pop(ctx);
        mgr.pop_overlay(ctx);
        mgr.pop_modal(ctx);
        assert!(mgr.is_empty());
    });
}

#[test]
fn test_active_returns_topmost_overlay() {
    with_mgr(|mgr, ctx| {
        mgr.push(Box::new(TestScene::new(SceneId::Home)), ctx);
        assert_eq!(mgr.active().unwrap().id(), SceneId::Home);
        mgr.push_overlay(
            Box::new(TestScene::new(SceneId::Pause).with_kind(SceneKind::Overlay)),
            ctx,
        );
        assert_eq!(mgr.active().unwrap().id(), SceneId::Pause);
    });
}
