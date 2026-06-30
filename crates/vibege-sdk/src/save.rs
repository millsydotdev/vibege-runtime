use std::path::PathBuf;
use std::sync::Arc;

use mlua::{Lua, Table};

pub struct SaveManager {
    base_dir: PathBuf,
    game_name: String,
}

impl SaveManager {
    pub fn new(base_dir: PathBuf, game_name: &str) -> Self {
        Self {
            base_dir,
            game_name: game_name.to_string(),
        }
    }

    fn save_dir(&self) -> PathBuf {
        self.base_dir.join("saves").join(&self.game_name)
    }

    fn save_path(&self, slot: &str) -> PathBuf {
        self.save_dir().join(format!("{slot}.save"))
    }

    pub fn save(&self, slot: &str, data: &str) -> Result<(), String> {
        let path = self.save_path(slot);
        std::fs::create_dir_all(self.save_dir()).map_err(|e| e.to_string())?;
        let checksum = hex::encode({
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(data.as_bytes());
            h.finalize()
        });
        let payload = format!("sha256:{checksum}\n{data}");
        std::fs::write(&path, &payload).map_err(|e| format!("Save failed: {e}"))?;
        Ok(())
    }

    pub fn load(&self, slot: &str) -> Result<Option<String>, String> {
        let path = self.save_path(slot);
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| format!("Load failed: {e}"))?;
        if let Some((rest, newline_pos)) = raw
            .strip_prefix("sha256:")
            .and_then(|r| r.find('\n').map(|p| (r, p)))
        {
            let stored = &rest[..newline_pos];
            let data = &rest[newline_pos + 1..];
            let computed = hex::encode({
                use sha2::{Digest, Sha256};
                let mut h = Sha256::new();
                h.update(data.as_bytes());
                h.finalize()
            });
            if stored != computed {
                return Err("Save data corrupted (checksum mismatch)".to_string());
            }
            return Ok(Some(data.to_string()));
        }
        Ok(Some(raw))
    }

    pub fn delete(&self, slot: &str) -> Result<bool, String> {
        let path = self.save_path(slot);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        Ok(true)
    }

    pub fn exists(&self, slot: &str) -> bool {
        self.save_path(slot).exists()
    }

    pub fn enumerate(&self) -> Result<Vec<String>, String> {
        let dir = self.save_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut slots = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if let Some(stripped) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.strip_suffix(".save"))
            {
                slots.push(stripped.to_string());
            }
        }
        slots.sort();
        Ok(slots)
    }

    pub fn metadata(&self, slot: &str) -> Result<Option<(u64, String)>, String> {
        let path = self.save_path(slot);
        if !path.exists() {
            return Ok(None);
        }
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();
        Ok(Some((meta.len(), modified)))
    }
}

pub fn register_save_api(lua: &Lua, base_dir: PathBuf, game_name: &str) -> Result<Table, String> {
    let mgr = Arc::new(SaveManager::new(base_dir, game_name));
    let s = lua.create_table().map_err(|e| e.to_string())?;

    let m = Arc::clone(&mgr);
    let save_fn = lua
        .create_function(move |_, (slot, data): (String, String)| {
            m.save(&slot, &data).map_err(mlua::Error::external)
        })
        .map_err(|e| e.to_string())?;
    s.set("save", save_fn).map_err(|e| e.to_string())?;

    let m = Arc::clone(&mgr);
    let load_fn = lua
        .create_function(move |_, slot: String| m.load(&slot).map_err(mlua::Error::external))
        .map_err(|e| e.to_string())?;
    s.set("load", load_fn).map_err(|e| e.to_string())?;

    let m = Arc::clone(&mgr);
    let delete_fn = lua
        .create_function(move |_, slot: String| m.delete(&slot).map_err(mlua::Error::external))
        .map_err(|e| e.to_string())?;
    s.set("delete", delete_fn).map_err(|e| e.to_string())?;

    let m = Arc::clone(&mgr);
    let exists_fn = lua
        .create_function(move |_, slot: String| Ok(m.exists(&slot)))
        .map_err(|e| e.to_string())?;
    s.set("exists", exists_fn).map_err(|e| e.to_string())?;

    let m = Arc::clone(&mgr);
    let enum_fn = lua
        .create_function(move |lua, _: ()| {
            let slots = m.enumerate().map_err(mlua::Error::external)?;
            let t = lua.create_table()?;
            for (i, slot) in slots.iter().enumerate() {
                t.set(i + 1, slot.clone())?;
            }
            Ok(t)
        })
        .map_err(|e| e.to_string())?;
    s.set("enumerate", enum_fn).map_err(|e| e.to_string())?;

    let m = Arc::clone(&mgr);
    let meta_fn = lua
        .create_function(move |lua, slot: String| {
            match m.metadata(&slot).map_err(mlua::Error::external)? {
                Some((size, modified)) => {
                    let t = lua.create_table()?;
                    t.set("size", size)?;
                    t.set("modified", modified)?;
                    Ok(Some(t))
                }
                None => Ok(None),
            }
        })
        .map_err(|e| e.to_string())?;
    s.set("metadata", meta_fn).map_err(|e| e.to_string())?;

    Ok(s)
}
