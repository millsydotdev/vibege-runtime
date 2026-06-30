use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Priority level for event subscribers.
/// Higher-priority subscribers receive events first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SubscriberPriority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Monitor = 3,
}

/// Category label for filtering event subscriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventCategory {
    Window,
    Overlay,
    Game,
    Download,
    Config,
    System,
    Input,
    Audio,
    Asset,
}

impl RuntimeEvent {
    pub fn category(&self) -> EventCategory {
        match self {
            RuntimeEvent::WindowCreated
            | RuntimeEvent::WindowHidden
            | RuntimeEvent::WindowMoved { .. }
            | RuntimeEvent::WindowResized { .. }
            | RuntimeEvent::WindowMinimized
            | RuntimeEvent::WindowRestored => EventCategory::Window,
            RuntimeEvent::OverlayShown | RuntimeEvent::OverlayHidden => EventCategory::Overlay,
            RuntimeEvent::GameInstalled { .. }
            | RuntimeEvent::GameRemoved { .. }
            | RuntimeEvent::GameStarted { .. }
            | RuntimeEvent::GameSuspended { .. }
            | RuntimeEvent::GameResumed { .. }
            | RuntimeEvent::GameExited { .. } => EventCategory::Game,
            RuntimeEvent::DownloadStarted { .. }
            | RuntimeEvent::DownloadFinished { .. }
            | RuntimeEvent::DownloadFailed { .. } => EventCategory::Download,
            RuntimeEvent::SettingsChanged { .. } => EventCategory::Config,
            RuntimeEvent::HotkeyPressed
            | RuntimeEvent::NotificationCreated { .. }
            | RuntimeEvent::ShuttingDown
            | RuntimeEvent::MonitorConnected { .. }
            | RuntimeEvent::MonitorDisconnected { .. }
            | RuntimeEvent::DpiChanged { .. }
            | RuntimeEvent::TrayNotificationActivated
            | RuntimeEvent::DiagnosticsReported => EventCategory::System,
            RuntimeEvent::InputCaptured { .. } => EventCategory::Input,
            RuntimeEvent::AudioDeviceChanged { .. } => EventCategory::Audio,
            RuntimeEvent::AssetLoaded { .. } | RuntimeEvent::AssetFailed { .. } => {
                EventCategory::Asset
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    WindowCreated,
    WindowHidden,
    WindowMoved { x: i32, y: i32 },
    WindowResized { width: u32, height: u32 },
    WindowMinimized,
    WindowRestored,
    OverlayShown,
    OverlayHidden,
    MonitorConnected { name: String },
    MonitorDisconnected { name: String },
    DpiChanged { scale: f64 },
    TrayNotificationActivated,
    GameInstalled { name: String, path: PathBuf },
    GameRemoved { name: String },
    GameStarted { name: String },
    GameSuspended { name: String },
    GameResumed { name: String },
    GameExited { name: String },
    DownloadStarted { name: String, url: String },
    DownloadFinished { name: String, path: PathBuf },
    DownloadFailed { name: String, error: String },
    SettingsChanged { key: String },
    HotkeyPressed,
    NotificationCreated { message: String },
    ShuttingDown,
    DiagnosticsReported,
    InputCaptured { key: String },
    AudioDeviceChanged { name: String },
    AssetLoaded { name: String },
    AssetFailed { name: String, error: String },
}

type Subscriber = Box<dyn Fn(&RuntimeEvent) + Send + Sync>;
type FilteredSubscriber = (
    EventCategory,
    SubscriberPriority,
    Box<dyn Fn(&RuntimeEvent) + Send + Sync>,
);

/// Event bus metrics snapshot.
#[derive(Debug, Clone, Default)]
pub struct EventBusMetrics {
    pub total_events_published: u64,
    pub total_subscribers: usize,
    pub total_filtered_subscribers: usize,
    pub events_by_category: std::collections::HashMap<EventCategory, u64>,
}

/// Publish-subscribe event bus with priorities, diagnostics, and panic isolation.
pub struct EventBus {
    subscribers: Mutex<Vec<(SubscriberPriority, Subscriber)>>,
    filtered: Mutex<Vec<FilteredSubscriber>>,
    event_count: AtomicU64,
    category_counts: Mutex<std::collections::HashMap<EventCategory, u64>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Mutex::new(Vec::new()),
            filtered: Mutex::new(Vec::new()),
            event_count: AtomicU64::new(0),
            category_counts: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Register a subscriber that receives every event.
    pub fn subscribe<F>(&self, f: F)
    where
        F: Fn(&RuntimeEvent) + Send + Sync + 'static,
    {
        self.subscribe_with_priority(SubscriberPriority::Normal, f);
    }

    /// Register a subscriber with explicit priority.
    pub fn subscribe_with_priority<F>(&self, priority: SubscriberPriority, f: F)
    where
        F: Fn(&RuntimeEvent) + Send + Sync + 'static,
    {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.push((priority, Box::new(f)));
            subs.sort_by_key(|k| std::cmp::Reverse(k.0));
        }
    }

    /// Register a filtered subscriber with default priority.
    pub fn subscribe_filtered<F>(&self, category: EventCategory, f: F)
    where
        F: Fn(&RuntimeEvent) + Send + Sync + 'static,
    {
        self.subscribe_filtered_with_priority(category, SubscriberPriority::Normal, f);
    }

    /// Register a filtered subscriber with explicit priority.
    pub fn subscribe_filtered_with_priority<F>(
        &self,
        category: EventCategory,
        priority: SubscriberPriority,
        f: F,
    ) where
        F: Fn(&RuntimeEvent) + Send + Sync + 'static,
    {
        if let Ok(mut subs) = self.filtered.lock() {
            subs.push((category, priority, Box::new(f)));
            subs.sort_by(|a, b| b.1.cmp(&a.1));
        }
    }

    /// Publish an event to all matching subscribers with panic isolation.
    /// A panicking subscriber does not prevent other subscribers from receiving the event.
    pub fn publish(&self, event: &RuntimeEvent) {
        self.event_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut counts) = self.category_counts.lock() {
            *counts.entry(event.category()).or_insert(0) += 1;
        }

        let cat = event.category();

        if let Ok(subs) = self.subscribers.lock() {
            for (_, sub) in subs.iter() {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    sub(event);
                }));
            }
        }

        if let Ok(subs) = self.filtered.lock() {
            for (c, _, sub) in subs.iter() {
                if *c == cat {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        sub(event);
                    }));
                }
            }
        }
    }

    /// Returns metrics about the event bus.
    pub fn metrics(&self) -> EventBusMetrics {
        let subs = self.subscribers.lock().map(|s| s.len()).unwrap_or(0);
        let filts = self.filtered.lock().map(|s| s.len()).unwrap_or(0);
        let counts = self
            .category_counts
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();
        EventBusMetrics {
            total_events_published: self.event_count.load(Ordering::Relaxed),
            total_subscribers: subs,
            total_filtered_subscribers: filts,
            events_by_category: counts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_subscribe_and_publish() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c1 = Arc::clone(&count);
        bus.subscribe(move |_| {
            c1.fetch_add(1, Ordering::SeqCst);
        });
        bus.publish(&RuntimeEvent::HotkeyPressed);
        bus.publish(&RuntimeEvent::SettingsChanged {
            key: "volume".into(),
        });
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_filtered_subscriber() {
        let bus = EventBus::new();
        let game_count = Arc::new(AtomicUsize::new(0));
        let gc = Arc::clone(&game_count);
        bus.subscribe_filtered(EventCategory::Game, move |_| {
            gc.fetch_add(1, Ordering::SeqCst);
        });
        bus.publish(&RuntimeEvent::GameStarted {
            name: "pong".into(),
        });
        bus.publish(&RuntimeEvent::HotkeyPressed);
        assert_eq!(game_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_subscriber_priority() {
        let bus = EventBus::new();
        let order = Arc::new(Mutex::new(Vec::new()));
        let o1 = Arc::clone(&order);
        bus.subscribe_with_priority(SubscriberPriority::High, move |_| {
            o1.lock().unwrap().push("high");
        });
        let o2 = Arc::clone(&order);
        bus.subscribe_with_priority(SubscriberPriority::Low, move |_| {
            o2.lock().unwrap().push("low");
        });
        bus.publish(&RuntimeEvent::HotkeyPressed);
        let result = order.lock().unwrap();
        assert_eq!(result[0], "high");
        assert_eq!(result[1], "low");
    }

    #[test]
    fn test_panic_isolation() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        bus.subscribe(move |_| panic!("subscriber panic"));
        let c2 = Arc::clone(&count);
        bus.subscribe(move |_| {
            c2.fetch_add(1, Ordering::SeqCst);
        });
        bus.publish(&RuntimeEvent::HotkeyPressed);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_metrics() {
        let bus = EventBus::new();
        bus.subscribe(|_| {});
        bus.subscribe_filtered(EventCategory::System, |_| {});
        bus.publish(&RuntimeEvent::HotkeyPressed);
        bus.publish(&RuntimeEvent::GameStarted {
            name: "pong".into(),
        });
        let m = bus.metrics();
        assert_eq!(m.total_events_published, 2);
        assert_eq!(m.total_subscribers, 1);
        assert_eq!(m.total_filtered_subscribers, 1);
        assert_eq!(
            *m.events_by_category
                .get(&EventCategory::System)
                .unwrap_or(&0),
            1
        );
    }

    #[test]
    fn test_publish_does_not_panic_on_empty_bus() {
        let bus = EventBus::new();
        bus.publish(&RuntimeEvent::HotkeyPressed);
        bus.publish(&RuntimeEvent::OverlayShown);
    }

    #[test]
    fn test_default_priority() {
        assert_eq!(SubscriberPriority::Normal, SubscriberPriority::default());
    }
}
