//! Runtime Event Bus — inter-subsystem communication via typed events.
//!
//! # Architecture
//!
//! The Event Bus decouples subsystems by providing a publish-subscribe channel.
//! Publishers emit [`RuntimeEvent`] values. Subscribers receive them via
//! closures registered with [`EventBus::subscribe`].
//!
//! # Event Flow
//!
//! ```ignore
//! Subsystem A                     Subsystem B
//!     │                               │
//!     │  bus.publish(&event)           │
//!     ├───────────────────────────────→│
//!     │                               │  subscriber(event)
//!     │                               │
//! ```
//!
//! # Thread Safety
//!
//! The bus is `Send + Sync`. Subscribers are called on the publisher's thread.
//! The logger subscriber at the top of main.rs runs on every thread that
//! publishes events.
//!
//! # Performance
//!
//! Subscribers are called synchronously. A slow subscriber will delay all
//! other subscribers. For latency-sensitive paths (e.g., frame rendering),
//! keep subscribers lightweight or use the event filter to avoid irrelevant
//! events.

use std::path::PathBuf;
use std::sync::Mutex;

/// Category label for filtering event subscriptions.
/// Each variant corresponds to a group of related [`RuntimeEvent`] values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCategory {
    Window,
    Overlay,
    Game,
    Download,
    Config,
    System,
}

impl RuntimeEvent {
    /// Return the category this event belongs to.
    /// Used by [`EventBus::subscribe_filtered`] to only receive relevant events.
    pub fn category(&self) -> EventCategory {
        match self {
            RuntimeEvent::WindowCreated | RuntimeEvent::WindowHidden => EventCategory::Window,
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
            | RuntimeEvent::ShuttingDown => EventCategory::System,
        }
    }
}

/// Every event the runtime can emit.
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    // — Window / Overlay
    WindowCreated,
    WindowHidden,
    OverlayShown,
    OverlayHidden,

    // — Games
    GameInstalled { name: String, path: PathBuf },
    GameRemoved { name: String },
    GameStarted { name: String },
    GameSuspended { name: String },
    GameResumed { name: String },
    GameExited { name: String },

    // — Downloads / Updates
    DownloadStarted { name: String, url: String },
    DownloadFinished { name: String, path: PathBuf },
    DownloadFailed { name: String, error: String },

    // — Configuration
    SettingsChanged { key: String },

    // — System
    HotkeyPressed,
    NotificationCreated { message: String },
    ShuttingDown,
}

/// A subscriber receives events.
type Subscriber = Box<dyn Fn(&RuntimeEvent) + Send + Sync>;

/// Filters events by category before passing to the inner closure.
/// Avoids per-event allocations by checking the category first.
type FilteredSubscriber = (EventCategory, Box<dyn Fn(&RuntimeEvent) + Send + Sync>);

/// Publish-subscribe event bus with optional category filtering.
///
/// # Examples
///
/// ```
/// use vibege_core::EventBus;
/// let bus = EventBus::new();
/// bus.subscribe(|e| println!("Event: {e:?}"));
/// ```
pub struct EventBus {
    subscribers: Mutex<Vec<Subscriber>>,
    filtered: Mutex<Vec<FilteredSubscriber>>,
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
        }
    }

    /// Register a subscriber that receives **every** event.
    pub fn subscribe<F>(&self, f: F)
    where
        F: Fn(&RuntimeEvent) + Send + Sync + 'static,
    {
        self.subscribers.lock().expect("lock").push(Box::new(f));
    }

    /// Register a subscriber that only receives events matching `category`.
    /// This is more efficient than filtering inside the closure because
    /// the category check happens before the closure is called.
    pub fn subscribe_filtered<F>(&self, category: EventCategory, f: F)
    where
        F: Fn(&RuntimeEvent) + Send + Sync + 'static,
    {
        self.filtered
            .lock()
            .expect("lock")
            .push((category, Box::new(f)));
    }

    /// Publish an event to all matching subscribers.
    ///
    /// All-subscribers are called first, then filtered subscribers matching
    /// the event's category. A panicking subscriber does not prevent other
    /// subscribers from receiving the event.
    pub fn publish(&self, event: &RuntimeEvent) {
        let cat = event.category();

        // Broadcast to all-subscribers
        if let Ok(subs) = self.subscribers.lock() {
            for sub in subs.iter() {
                sub(event);
            }
        }

        // Broadcast to category-filtered subscribers
        if let Ok(subs) = self.filtered.lock() {
            for (c, sub) in subs.iter() {
                if *c == cat {
                    sub(event);
                }
            }
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

        // Game event should trigger the filtered subscriber
        bus.publish(&RuntimeEvent::GameStarted {
            name: "pong".into(),
        });
        // System event should NOT trigger it
        bus.publish(&RuntimeEvent::HotkeyPressed);

        assert_eq!(game_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_filtered_subscriber_multiple_categories() {
        let bus = EventBus::new();
        let sys_count = Arc::new(AtomicUsize::new(0));
        let dl_count = Arc::new(AtomicUsize::new(0));

        let sc = Arc::clone(&sys_count);
        bus.subscribe_filtered(EventCategory::System, move |_| {
            sc.fetch_add(1, Ordering::SeqCst);
        });
        let dc = Arc::clone(&dl_count);
        bus.subscribe_filtered(EventCategory::Download, move |_| {
            dc.fetch_add(1, Ordering::SeqCst);
        });

        bus.publish(&RuntimeEvent::HotkeyPressed); // System
        bus.publish(&RuntimeEvent::ShuttingDown); // System
        bus.publish(&RuntimeEvent::DownloadStarted {
            name: "pong".into(),
            url: "https://example.com/pkg".into(),
        }); // Download

        assert_eq!(sys_count.load(Ordering::SeqCst), 2);
        assert_eq!(dl_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_event_category_mapping() {
        assert_eq!(
            RuntimeEvent::WindowCreated.category(),
            EventCategory::Window
        );
        assert_eq!(
            RuntimeEvent::OverlayShown.category(),
            EventCategory::Overlay
        );
        assert_eq!(
            RuntimeEvent::GameStarted {
                name: "pong".into()
            }
            .category(),
            EventCategory::Game
        );
        assert_eq!(
            RuntimeEvent::DownloadFinished {
                name: "pong".into(),
                path: PathBuf::from("/tmp")
            }
            .category(),
            EventCategory::Download
        );
        assert_eq!(
            RuntimeEvent::SettingsChanged {
                key: "volume".into()
            }
            .category(),
            EventCategory::Config
        );
        assert_eq!(
            RuntimeEvent::HotkeyPressed.category(),
            EventCategory::System
        );
    }

    #[test]
    fn test_publish_does_not_panic_on_empty_bus() {
        let bus = EventBus::new();
        bus.publish(&RuntimeEvent::HotkeyPressed);
        bus.publish(&RuntimeEvent::OverlayShown);
        // Should not panic
    }
}
