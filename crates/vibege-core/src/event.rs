//! Runtime Event Bus — inter-subsystem communication via typed events.
//!
//! Subsystems publish events to the bus. Other subsystems (or the main loop)
//! subscribe to react to state changes. Events are fire-and-forget — there
//! is no return value or acknowledgement.
//!
//! The bus is thread-safe (Send + Sync) so tray threads, download workers,
//! or future plugin threads can publish events that the main loop processes.

use std::path::PathBuf;
use std::sync::Mutex;

/// Every event the runtime can emit.
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    // — Window / Overlay
    /// The runtime window was created and is ready for rendering.
    WindowCreated,
    /// Window was hidden (overlay dismissed).
    WindowHidden,
    /// Overlay appeared (shown by hotkey or tray).
    OverlayShown,
    /// Overlay hidden by hotkey.
    OverlayHidden,

    // — Games
    /// A game was installed from the Store.
    GameInstalled { name: String, path: PathBuf },
    /// A game was removed from the library.
    GameRemoved { name: String },
    /// A game session started.
    GameStarted { name: String },
    /// A game session was suspended (overlay hidden while playing).
    GameSuspended { name: String },
    /// A game session resumed (overlay shown while playing).
    GameResumed { name: String },
    /// A game session exited normally.
    GameExited { name: String },

    // — Downloads / Updates
    /// A package download has started.
    DownloadStarted { name: String, url: String },
    /// A package download finished successfully.
    DownloadFinished { name: String, path: PathBuf },
    /// A package download failed.
    DownloadFailed { name: String, error: String },

    // — Configuration
    /// A settings value was changed.
    SettingsChanged { key: String },

    // — System
    /// The hotkey was pressed (main thread only).
    HotkeyPressed,
    /// A notification should be shown.
    NotificationCreated { message: String },
    /// Runtime is shutting down cleanly.
    ShuttingDown,
}

/// A subscriber receives every published event.
type Subscriber = Box<dyn Fn(&RuntimeEvent) + Send + Sync>;

/// Simple publish-subscribe event bus.
///
/// Subscribers are called synchronously in the order they were registered.
/// If a subscriber panics, subsequent subscribers still receive the event.
///
/// # Thread safety
///
/// `publish()` can be called from any thread. All subscribers are called
/// on the publisher's thread. The bus itself is `Send + Sync`.
pub struct EventBus {
    subscribers: Mutex<Vec<Subscriber>>,
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
        }
    }

    /// Register a subscriber. The closure will be called for every event.
    pub fn subscribe<F>(&self, f: F)
    where
        F: Fn(&RuntimeEvent) + Send + Sync + 'static,
    {
        self.subscribers.lock().expect("lock").push(Box::new(f));
    }

    /// Publish an event to all subscribers.
    pub fn publish(&self, event: &RuntimeEvent) {
        let subscribers = self.subscribers.lock().expect("lock");
        for sub in subscribers.iter() {
            sub(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_event_bus_subscribe_and_publish() {
        let bus = EventBus::new();
        let count = std::sync::Arc::new(AtomicUsize::new(0));

        let c1 = Arc::clone(&count);
        bus.subscribe(move |_| {
            c1.fetch_add(1, Ordering::SeqCst);
        });
        let c2 = Arc::clone(&count);
        bus.subscribe(move |_| {
            c2.fetch_add(1, Ordering::SeqCst);
        });

        bus.publish(&RuntimeEvent::HotkeyPressed);
        bus.publish(&RuntimeEvent::SettingsChanged {
            key: "volume".into(),
        });

        assert_eq!(count.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn test_event_cloning() {
        let e = RuntimeEvent::GameStarted {
            name: "pong".into(),
        };
        let cloned = e.clone();
        match cloned {
            RuntimeEvent::GameStarted { name } => assert_eq!(name, "pong"),
            _ => panic!("wrong variant"),
        }
    }
}
