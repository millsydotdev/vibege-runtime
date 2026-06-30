/// Texture asset wrapping GPU resources.
///
/// Stores the bind group index into the renderer's texture list,
/// along with dimensions and format metadata.
#[derive(Debug, Clone)]
pub struct TextureAsset {
    /// Index into the renderer's texture bind groups list.
    pub bind_group_index: usize,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

impl TextureAsset {
    pub fn new(bind_group_index: usize, width: u32, height: u32) -> Self {
        Self {
            bind_group_index,
            width,
            height,
            format: "rgba8".into(),
        }
    }
}

/// Audio asset wrapping PCM sample data.
#[derive(Debug, Clone)]
pub struct AudioAsset {
    /// 16-bit signed PCM samples at 44100 Hz.
    pub samples: Vec<i16>,
    pub duration_secs: f32,
}

impl AudioAsset {
    pub fn new(samples: Vec<i16>) -> Self {
        let duration_secs = if samples.is_empty() {
            0.0
        } else {
            samples.len() as f32 / 44100.0
        };
        Self {
            samples,
            duration_secs,
        }
    }

    pub fn memory_bytes(&self) -> usize {
        self.samples.len() * 2
    }
}

/// Font asset wrapping a bitmap font atlas.
#[derive(Debug, Clone)]
pub struct FontAsset {
    /// RGBA pixel data for the font atlas.
    pub atlas_rgba: Vec<u8>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub char_width: u32,
    pub char_height: u32,
    pub chars_per_row: u32,
}

impl FontAsset {
    pub fn new(
        atlas_rgba: Vec<u8>,
        atlas_width: u32,
        atlas_height: u32,
        char_width: u32,
        char_height: u32,
        chars_per_row: u32,
    ) -> Self {
        Self {
            atlas_rgba,
            atlas_width,
            atlas_height,
            char_width,
            char_height,
            chars_per_row,
        }
    }
}

/// Lua source code asset.
#[derive(Debug, Clone)]
pub struct LuaSourceAsset {
    pub source: String,
}

impl LuaSourceAsset {
    pub fn new(source: String) -> Self {
        Self { source }
    }
}

/// Generic raw binary data asset.
#[derive(Debug, Clone)]
pub struct RawAsset {
    pub data: Vec<u8>,
    pub mime_type: String,
}

impl RawAsset {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            mime_type: "application/octet-stream".into(),
        }
    }

    pub fn with_mime(data: Vec<u8>, mime_type: String) -> Self {
        Self { data, mime_type }
    }
}

/// A mounted .vibepkg package that can list and read entries.
#[derive(Debug, Clone)]
pub struct PackageAsset {
    pub name: String,
    pub version: String,
    pub entry_point: String,
    entries: Vec<(String, Vec<u8>, u64)>,
}

impl PackageAsset {
    pub fn new(
        name: String,
        version: String,
        entry_point: String,
        entries: Vec<(String, Vec<u8>, u64)>,
    ) -> Self {
        Self {
            name,
            version,
            entry_point,
            entries,
        }
    }

    pub fn entries(&self) -> &[(String, Vec<u8>, u64)] {
        &self.entries
    }

    pub fn read_entry(&self, path: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|(p, _, _)| p == path)
            .map(|(_, data, _)| data.as_slice())
    }

    pub fn entry_names(&self) -> Vec<&str> {
        self.entries.iter().map(|(p, _, _)| p.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_asset() {
        let tex = TextureAsset::new(0, 64, 32);
        assert_eq!(tex.bind_group_index, 0);
        assert_eq!(tex.width, 64);
        assert_eq!(tex.height, 32);
    }

    #[test]
    fn test_audio_asset() {
        let samples = vec![0i16; 44100];
        let audio = AudioAsset::new(samples.clone());
        assert_eq!(audio.samples.len(), 44100);
        assert!((audio.duration_secs - 1.0).abs() < 1e-6);
        assert_eq!(audio.memory_bytes(), 44100 * 2);
    }

    #[test]
    fn test_audio_asset_empty() {
        let audio = AudioAsset::new(vec![]);
        assert!((audio.duration_secs - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_lua_source_asset() {
        let src = LuaSourceAsset::new("print('hello')".into());
        assert_eq!(src.source, "print('hello')");
    }

    #[test]
    fn test_raw_asset() {
        let raw = RawAsset::new(vec![1, 2, 3]);
        assert_eq!(raw.data, vec![1, 2, 3]);
        assert_eq!(raw.mime_type, "application/octet-stream");
    }

    #[test]
    fn test_raw_asset_with_mime() {
        let raw = RawAsset::with_mime(vec![0, 0], "image/png".into());
        assert_eq!(raw.mime_type, "image/png");
    }

    #[test]
    fn test_package_asset() {
        let entries = vec![
            ("src/main.lua".into(), b"print('hello')".to_vec(), 14),
            ("assets/icon.png".into(), b"PNG_DATA".to_vec(), 8),
        ];
        let pkg = PackageAsset::new("test".into(), "1.0".into(), "src/main.lua".into(), entries);
        assert_eq!(pkg.name, "test");
        assert_eq!(pkg.version, "1.0");
        assert_eq!(pkg.read_entry("src/main.lua"), Some(&b"print('hello')"[..]));
        assert_eq!(pkg.read_entry("missing"), None);
        assert_eq!(pkg.entry_names(), vec!["src/main.lua", "assets/icon.png"]);
    }
}
