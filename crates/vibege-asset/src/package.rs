use std::path::Path;

use crate::loader::LoaderResult;
use crate::types::PackageAsset;

/// Mounts and reads entries from .vibepkg (ZIP) archives.
pub struct PackageMount;

impl PackageMount {
    /// Mount a .vibepkg buffer and return a PackageAsset.
    ///
    /// Validates the ZIP header, extracts all entries, and returns
    /// a PackageAsset with in-memory entry data.
    pub fn mount(data: &[u8], name: &str) -> LoaderResult<PackageAsset> {
        // Validate ZIP header
        if data.len() < 4
            || data[0] != 0x50
            || data[1] != 0x4B
            || data[2] != 0x03
            || data[3] != 0x04
        {
            return Err(crate::loader::LoaderError::InvalidData(
                "Not a valid ZIP archive".into(),
            ));
        }

        let cursor = std::io::Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| crate::loader::LoaderError::InvalidData(format!("ZIP error: {e}")))?;

        let mut entries = Vec::new();
        let mut entry_point = String::from("src/main.lua");
        let mut version = String::from("0.1.0");

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| crate::loader::LoaderError::InvalidData(format!("Entry {i}: {e}")))?;

            if entry.is_dir() {
                continue;
            }

            let entry_name = entry.name().to_string();
            let mut content = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut content)
                .map_err(crate::loader::LoaderError::Io)?;

            let size = content.len() as u64;

            // Check for manifest metadata
            if (entry_name == "vibege.json" || entry_name == "manifest.json")
                && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&content)
            {
                if let Some(ep) = json["entry"].as_str() {
                    entry_point = ep.to_string();
                }
                if let Some(v) = json["version"].as_str() {
                    version = v.to_string();
                }
            }

            entries.push((entry_name, content, size));
        }

        Ok(PackageAsset::new(
            name.to_string(),
            version,
            entry_point,
            entries,
        ))
    }

    /// Mount from a .vibepkg file on disk.
    pub fn mount_file(path: &Path, name: &str) -> LoaderResult<PackageAsset> {
        let data = std::fs::read(path).map_err(crate::loader::LoaderError::Io)?;
        Self::mount(&data, name)
    }

    pub fn validate(data: &[u8]) -> LoaderResult<()> {
        if data.len() < 4
            || data[0] != 0x50
            || data[1] != 0x4B
            || data[2] != 0x03
            || data[3] != 0x04
        {
            return Err(crate::loader::LoaderError::InvalidData(
                "Not a valid ZIP archive".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let options = zip::write::SimpleFileOptions::default();

        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }

        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn test_package_mount() {
        let data = create_test_zip(&[("src/main.lua", b"print('hello')")]);
        let pkg = PackageMount::mount(&data, "test_game").unwrap();
        assert_eq!(pkg.name, "test_game");
        assert_eq!(pkg.entry_names(), vec!["src/main.lua"]);
        assert_eq!(pkg.read_entry("src/main.lua"), Some(&b"print('hello')"[..]));
    }

    #[test]
    fn test_package_mount_with_manifest() {
        let data = create_test_zip(&[
            (
                "manifest.json",
                br#"{"entry": "game.lua", "version": "2.0.0"}"#,
            ),
            ("game.lua", b"x = 1"),
        ]);
        let pkg = PackageMount::mount(&data, "game").unwrap();
        assert_eq!(pkg.version, "2.0.0");
        assert_eq!(pkg.entry_point, "game.lua");
    }

    #[test]
    fn test_package_mount_invalid_header() {
        let result = PackageMount::mount(b"not a zip", "bad");
        assert!(result.is_err());
    }

    #[test]
    fn test_package_mount_empty() {
        let data = create_test_zip(&[("placeholder.txt", b"")]);
        let pkg = PackageMount::mount(&data, "empty").unwrap();
        // Only the placeholder entry should exist (empty file)
        assert_eq!(pkg.entries().len(), 1);
    }

    #[test]
    fn test_package_validate() {
        let data = create_test_zip(&[("a.txt", b"hello")]);
        assert!(PackageMount::validate(&data).is_ok());
        assert!(PackageMount::validate(b"bad").is_err());
    }
}
