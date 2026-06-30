use std::collections::HashMap;
use std::sync::Mutex;

use mlua::{Lua, Table};

/// Per-game key-value storage.
///
/// Each game gets its own isolated namespace. Values are string-based
/// and stored in memory during the session. Future: persist to disk.
pub struct GameStorage {
    data: Mutex<HashMap<String, String>>,
}

fn lock_storage(
    mtx: &Mutex<HashMap<String, String>>,
) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
    mtx.lock().unwrap_or_else(|e| {
        tracing::warn!("Storage mutex poisoned — recovering inner data");
        e.into_inner()
    })
}

impl GameStorage {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }

    pub fn save(&self, key: &str, value: &str) {
        let mut data = lock_storage(&self.data);
        data.insert(key.to_string(), value.to_string());
    }

    pub fn load(&self, key: &str) -> Option<String> {
        let data = lock_storage(&self.data);
        data.get(key).cloned()
    }

    pub fn delete(&self, key: &str) {
        let mut data = lock_storage(&self.data);
        data.remove(key);
    }

    pub fn keys(&self) -> Vec<String> {
        let data = lock_storage(&self.data);
        let mut keys: Vec<String> = data.keys().cloned().collect();
        keys.sort();
        keys
    }

    pub fn clear(&self) {
        let mut data = lock_storage(&self.data);
        data.clear();
    }
}

impl Default for GameStorage {
    fn default() -> Self {
        Self::new()
    }
}

pub fn register_storage_api(lua: &Lua, storage: &'static GameStorage) -> Result<Table, String> {
    let storage_table = lua.create_table().map_err(|e| e.to_string())?;

    let save_fn = lua
        .create_function(move |_, (key, value): (String, String)| {
            storage.save(&key, &value);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    storage_table
        .set("save", save_fn)
        .map_err(|e| e.to_string())?;

    let load_fn = lua
        .create_function(move |_, key: String| {
            let result = storage.load(&key);
            Ok(result)
        })
        .map_err(|e| e.to_string())?;
    storage_table
        .set("load", load_fn)
        .map_err(|e| e.to_string())?;

    let delete_fn = lua
        .create_function(move |_, key: String| {
            storage.delete(&key);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    storage_table
        .set("delete", delete_fn)
        .map_err(|e| e.to_string())?;

    let keys_fn = lua
        .create_function(move |lua, _: ()| {
            let keys = storage.keys();
            let tbl = lua.create_table()?;
            for (i, key) in keys.iter().enumerate() {
                tbl.set(i + 1, key.clone())?;
            }
            Ok(tbl)
        })
        .map_err(|e| e.to_string())?;
    storage_table
        .set("keys", keys_fn)
        .map_err(|e| e.to_string())?;

    Ok(storage_table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_save_and_load() {
        let storage = GameStorage::new();
        storage.save("score", "42");
        assert_eq!(storage.load("score"), Some("42".to_string()));
    }

    #[test]
    fn test_storage_load_missing() {
        let storage = GameStorage::new();
        assert_eq!(storage.load("missing"), None);
    }

    #[test]
    fn test_storage_delete() {
        let storage = GameStorage::new();
        storage.save("temp", "value");
        assert!(storage.load("temp").is_some());
        storage.delete("temp");
        assert!(storage.load("temp").is_none());
    }

    #[test]
    fn test_storage_keys() {
        let storage = GameStorage::new();
        storage.save("b", "2");
        storage.save("a", "1");
        storage.save("c", "3");
        let keys = storage.keys();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_storage_clear() {
        let storage = GameStorage::new();
        storage.save("a", "1");
        storage.save("b", "2");
        assert_eq!(storage.keys().len(), 2);
        storage.clear();
        assert!(storage.keys().is_empty());
    }

    #[test]
    fn test_storage_overwrite() {
        let storage = GameStorage::new();
        storage.save("key", "first");
        storage.save("key", "second");
        assert_eq!(storage.load("key"), Some("second".to_string()));
    }
}
