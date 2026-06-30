use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Opaque identifier for an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetId(u64);

impl AssetId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AssetId({})", self.0)
    }
}

/// Tracks the reference count and runs a cleanup callback when the last
/// handle is dropped.
pub struct ResourceLifetime {
    ref_count: AtomicU32,
    cleanup: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl ResourceLifetime {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            ref_count: AtomicU32::new(1),
            cleanup: Mutex::new(None),
        })
    }

    pub fn with_cleanup<F>(cleanup: F) -> Arc<Self>
    where
        F: FnOnce() + Send + 'static,
    {
        Arc::new(Self {
            ref_count: AtomicU32::new(1),
            cleanup: Mutex::new(Some(Box::new(cleanup))),
        })
    }

    pub fn ref_count(&self) -> u32 {
        self.ref_count.load(Ordering::Relaxed)
    }

    pub fn increment(&self) -> u32 {
        self.ref_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Decrements the ref count. Returns true if the count reached zero.
    pub fn decrement(&self) -> bool {
        let prev = self.ref_count.fetch_sub(1, Ordering::Release);
        if prev == 1 {
            std::sync::atomic::fence(Ordering::Acquire);
            if let Ok(mut cleanup) = self.cleanup.lock()
                && let Some(f) = cleanup.take()
            {
                f();
            }
            true
        } else {
            false
        }
    }
}

impl std::fmt::Debug for ResourceLifetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceLifetime")
            .field("ref_count", &self.ref_count())
            .finish()
    }
}

/// Typed handle to an asset in the asset manager.
///
/// Cloning increments the reference count. Dropping decrements it.
/// When the last handle is dropped, the resource is cleaned up.
pub struct AssetHandle<T> {
    pub(crate) id: AssetId,
    pub(crate) key: String,
    pub(crate) lifetime: Arc<ResourceLifetime>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> AssetHandle<T> {
    pub fn new(id: AssetId, key: String, lifetime: Arc<ResourceLifetime>) -> Self {
        Self {
            id,
            key,
            lifetime,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn id(&self) -> AssetId {
        self.id
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn ref_count(&self) -> u32 {
        self.lifetime.ref_count()
    }
}

impl<T> Clone for AssetHandle<T> {
    fn clone(&self) -> Self {
        self.lifetime.increment();
        Self {
            id: self.id,
            key: self.key.clone(),
            lifetime: Arc::clone(&self.lifetime),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Drop for AssetHandle<T> {
    fn drop(&mut self) {
        self.lifetime.decrement();
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for AssetHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetHandle")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("ref_count", &self.lifetime.ref_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_id_creation() {
        let id = AssetId::new(42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_asset_id_display() {
        let id = AssetId::new(7);
        assert_eq!(format!("{id}"), "AssetId(7)");
    }

    #[test]
    fn test_resource_lifetime_refcount() {
        let lifetime = ResourceLifetime::new();
        assert_eq!(lifetime.ref_count(), 1);
        lifetime.increment();
        assert_eq!(lifetime.ref_count(), 2);
        lifetime.decrement();
        assert_eq!(lifetime.ref_count(), 1);
    }

    #[test]
    fn test_resource_lifetime_cleanup_on_drop() {
        let cleaned = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cleaned_clone = Arc::clone(&cleaned);
        let lifetime = ResourceLifetime::with_cleanup(move || {
            cleaned_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        assert!(!cleaned.load(std::sync::atomic::Ordering::SeqCst));
        lifetime.decrement();
        assert!(cleaned.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_handle_clone_increments_refcount() {
        let id = AssetId::new(1);
        let lifetime = ResourceLifetime::new();
        let handle = AssetHandle::<()>::new(id, "test".into(), Arc::clone(&lifetime));
        assert_eq!(handle.ref_count(), 1);

        let cloned = handle.clone();
        assert_eq!(handle.ref_count(), 2);
        assert_eq!(cloned.id(), id);
        assert_eq!(cloned.key(), "test");
        drop(cloned);
        assert_eq!(handle.ref_count(), 1);
    }

    #[test]
    fn test_handle_drop_decrements() {
        let id = AssetId::new(2);
        let lifetime = ResourceLifetime::new();
        let handle = AssetHandle::<()>::new(id, "test".into(), Arc::clone(&lifetime));
        assert_eq!(lifetime.ref_count(), 1);
        drop(handle);
        assert_eq!(lifetime.ref_count(), 0);
    }
}
