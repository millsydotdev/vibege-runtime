use crate::AssetId;
use std::time::Instant;

/// Identifies the type of an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetTypeId {
    Texture,
    Audio,
    Font,
    LuaSource,
    Package,
    Raw,
}

impl AssetTypeId {
    pub fn name(&self) -> &'static str {
        match self {
            AssetTypeId::Texture => "texture",
            AssetTypeId::Audio => "audio",
            AssetTypeId::Font => "font",
            AssetTypeId::LuaSource => "lua_source",
            AssetTypeId::Package => "package",
            AssetTypeId::Raw => "raw",
        }
    }
}

impl std::fmt::Display for AssetTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Where the asset was loaded from.
#[derive(Debug, Clone)]
pub enum AssetSource {
    /// Loaded from an external file on disk.
    File(String),
    /// Loaded from within a mounted package.
    Package(String, String),
    /// Embedded at compile time.
    Embedded(&'static str),
    /// Generated procedurally at runtime.
    Procedural(String),
    /// Raw bytes provided by the caller.
    Memory,
}

impl std::fmt::Display for AssetSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetSource::File(path) => write!(f, "file:{path}"),
            AssetSource::Package(pkg, path) => write!(f, "pkg:{pkg}:{path}"),
            AssetSource::Embedded(name) => write!(f, "embedded:{name}"),
            AssetSource::Procedural(desc) => write!(f, "procedural:{desc}"),
            AssetSource::Memory => write!(f, "memory"),
        }
    }
}

/// Metadata associated with a loaded asset.
#[derive(Debug, Clone)]
pub struct AssetMetadata {
    pub id: AssetId,
    pub key: String,
    pub asset_type: AssetTypeId,
    pub source: AssetSource,
    pub size_bytes: u64,
    pub format: String,
    pub loaded_at: Instant,
    pub last_accessed: Instant,
    pub load_count: u64,
}

impl AssetMetadata {
    pub fn new(
        id: AssetId,
        key: String,
        asset_type: AssetTypeId,
        source: AssetSource,
        size_bytes: u64,
        format: String,
    ) -> Self {
        let now = Instant::now();
        Self {
            id,
            key,
            asset_type,
            source,
            size_bytes,
            format,
            loaded_at: now,
            last_accessed: now,
            load_count: 1,
        }
    }

    pub fn touch(&mut self) {
        self.last_accessed = Instant::now();
        self.load_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_type_id_names() {
        assert_eq!(AssetTypeId::Texture.name(), "texture");
        assert_eq!(AssetTypeId::Audio.name(), "audio");
        assert_eq!(AssetTypeId::Font.name(), "font");
        assert_eq!(AssetTypeId::LuaSource.name(), "lua_source");
        assert_eq!(AssetTypeId::Package.name(), "package");
        assert_eq!(AssetTypeId::Raw.name(), "raw");
    }

    #[test]
    fn test_asset_source_display() {
        assert_eq!(
            format!("{}", AssetSource::File("a.png".into())),
            "file:a.png"
        );
        assert_eq!(
            format!("{}", AssetSource::Package("game".into(), "tex.png".into())),
            "pkg:game:tex.png"
        );
        assert_eq!(
            format!("{}", AssetSource::Embedded("font")),
            "embedded:font"
        );
        assert_eq!(
            format!("{}", AssetSource::Procedural("sine".into())),
            "procedural:sine"
        );
        assert_eq!(format!("{}", AssetSource::Memory), "memory");
    }

    #[test]
    fn test_metadata_touch() {
        let id = AssetId::new(1);
        let src = AssetSource::Memory;
        let mut meta = AssetMetadata::new(
            id,
            "test".into(),
            AssetTypeId::Texture,
            src,
            1024,
            "png".into(),
        );
        assert_eq!(meta.load_count, 1);
        meta.touch();
        assert_eq!(meta.load_count, 2);
    }
}
