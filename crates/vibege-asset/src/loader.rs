use crate::metadata::AssetSource;
use crate::types::{AudioAsset, LuaSourceAsset, RawAsset, TextureAsset};

/// Errors that can occur during asset loading.
#[derive(Debug)]
pub enum LoaderError {
    InvalidData(String),
    UnsupportedFormat(String),
    Io(std::io::Error),
    DecodeFailed(String),
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::InvalidData(msg) => write!(f, "Invalid data: {msg}"),
            LoaderError::UnsupportedFormat(msg) => write!(f, "Unsupported format: {msg}"),
            LoaderError::Io(e) => write!(f, "IO error: {e}"),
            LoaderError::DecodeFailed(msg) => write!(f, "Decode failed: {msg}"),
        }
    }
}

impl std::error::Error for LoaderError {}

impl From<std::io::Error> for LoaderError {
    fn from(e: std::io::Error) -> Self {
        LoaderError::Io(e)
    }
}

/// Result type for asset loading.
pub type LoaderResult<T> = Result<T, LoaderError>;

/// The signature for a texture loader callback.
///
/// The renderer provides this callback so the asset manager can create
/// GPU textures without depending on wgpu directly.
pub type TextureLoaderFn =
    Box<dyn Fn(&[u8], AssetSource) -> LoaderResult<TextureAsset> + Send + Sync>;

/// Type alias for returning a texture loader callback,
/// used to avoid complex type in function signatures.
pub type TextureLoaderCreator = TextureLoaderFn;

/// Loader for texture data (PNG, JPEG, etc.).
///
/// Decodes image bytes in software (via the `image` crate) and returns
/// a `TextureAsset` with the raw RGBA pixels. The GPU-side upload is
/// handled by the renderer's callback.
pub struct TextureLoader;

impl TextureLoader {
    /// Validate that data can be decoded as an image.
    pub fn validate(data: &[u8]) -> LoaderResult<()> {
        let reader = image::ImageReader::new(std::io::Cursor::new(data))
            .with_guessed_format()
            .map_err(|e| LoaderError::InvalidData(e.to_string()))?;
        if reader.format().is_none() {
            return Err(LoaderError::UnsupportedFormat(
                "Unknown image format".into(),
            ));
        }
        Ok(())
    }

    /// Load a texture asset by decoding image bytes.
    /// Returns the raw RGBA data along with dimensions.
    pub fn load(data: &[u8]) -> LoaderResult<(Vec<u8>, u32, u32)> {
        let img = image::load_from_memory(data)
            .map_err(|e| LoaderError::DecodeFailed(e.to_string()))?
            .to_rgba8();
        let (width, height) = img.dimensions();
        Ok((img.into_raw(), width, height))
    }
}

/// Loader for audio data.
pub struct AudioLoader;

impl AudioLoader {
    /// Validate that data represents valid audio.
    pub fn validate(data: &[u8]) -> LoaderResult<()> {
        if data.is_empty() {
            return Err(LoaderError::InvalidData("Empty audio data".into()));
        }
        Ok(())
    }

    /// "Load" raw PCM data into an AudioAsset.
    /// Currently only accepts pre-decoded 44100 Hz PCM i16 data.
    /// Future: add WAV/MP3/OGG decoding.
    pub fn load(samples: Vec<i16>) -> AudioAsset {
        AudioAsset::new(samples)
    }
}

/// Loader for raw binary data.
pub struct RawLoader;

impl RawLoader {
    pub fn validate(data: &[u8]) -> LoaderResult<()> {
        if data.is_empty() {
            return Err(LoaderError::InvalidData("Empty raw data".into()));
        }
        Ok(())
    }

    pub fn load(data: Vec<u8>) -> RawAsset {
        RawAsset::new(data)
    }
}

/// Loader for Lua source code.
pub struct LuaSourceLoader;

impl LuaSourceLoader {
    pub fn validate(data: &[u8]) -> LoaderResult<()> {
        if data.is_empty() {
            return Err(LoaderError::InvalidData("Empty Lua source".into()));
        }
        // Check for UTF-8 validity as a basic sanity check
        if std::str::from_utf8(data).is_err() {
            return Err(LoaderError::InvalidData(
                "Lua source is not valid UTF-8".into(),
            ));
        }
        Ok(())
    }

    pub fn load(data: &[u8]) -> LoaderResult<LuaSourceAsset> {
        let source = std::str::from_utf8(data)
            .map_err(|e| LoaderError::InvalidData(format!("Invalid UTF-8: {e}")))?
            .to_string();
        Ok(LuaSourceAsset::new(source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_loader_validate_empty() {
        let result = TextureLoader::validate(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_texture_loader_validate_random() {
        let result = TextureLoader::validate(b"not an image");
        assert!(result.is_err());
    }

    #[test]
    fn test_audio_loader_validate_empty() {
        let result = AudioLoader::validate(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_audio_loader_validate_valid() {
        let result = AudioLoader::validate(&[0u8; 100]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_audio_loader_load_samples() {
        let audio = AudioLoader::load(vec![0i16; 44100]);
        assert_eq!(audio.samples.len(), 44100);
    }

    #[test]
    fn test_lua_source_loader_validate_empty() {
        let result = LuaSourceLoader::validate(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_lua_source_loader_validate_valid() {
        let result = LuaSourceLoader::validate(b"print('hello')");
        assert!(result.is_ok());
    }

    #[test]
    fn test_lua_source_loader_validate_non_utf8() {
        let result = LuaSourceLoader::validate(&[0xFF, 0xFE, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn test_lua_source_loader_load() {
        let asset = LuaSourceLoader::load(b"x = 42").unwrap();
        assert_eq!(asset.source, "x = 42");
    }

    #[test]
    fn test_raw_loader_validate_empty() {
        let result = RawLoader::validate(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_raw_loader_load() {
        let raw = RawLoader::load(vec![1, 2, 3]);
        assert_eq!(raw.data, vec![1, 2, 3]);
    }
}
