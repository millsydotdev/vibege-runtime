use std::path::Path;
use std::path::PathBuf;

use super::context::PackageManifest;
use super::error::RuntimeError;

/// Result of a validation check.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub passed: bool,
    pub checks: Vec<ValidationCheck>,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self {
            passed: true,
            checks: Vec::new(),
        }
    }

    pub fn fail(&mut self, check: ValidationCheck) {
        self.passed = false;
        self.checks.push(check);
    }

    pub fn pass(&mut self, check: ValidationCheck) {
        self.checks.push(check);
    }

    pub fn failures(&self) -> Vec<&ValidationCheck> {
        self.checks.iter().filter(|c| !c.passed).collect()
    }

    pub fn summary(&self) -> String {
        let total = self.checks.len();
        let passed_count = self.checks.iter().filter(|c| c.passed).count();
        let failed_count = total - passed_count;
        format!("{passed_count}/{total} checks passed, {failed_count} failed")
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

/// A single validation check result.
#[derive(Debug, Clone)]
pub struct ValidationCheck {
    pub name: &'static str,
    pub passed: bool,
    pub message: String,
}

impl ValidationCheck {
    pub fn new(name: &'static str, passed: bool, message: String) -> Self {
        Self {
            name,
            passed,
            message,
        }
    }
}

/// Comprehensive package validator.
pub struct PackageValidator;

impl PackageValidator {
    /// Run all validation checks on a package.
    pub fn validate(
        manifest: &PackageManifest,
        entry_data: Option<&[u8]>,
        asset_paths: &[String],
        engine_version: &str,
    ) -> ValidationReport {
        let mut report = ValidationReport::new();

        Self::check_manifest(manifest, &mut report);
        Self::check_entry_point(manifest, entry_data, &mut report);
        Self::check_version(manifest, engine_version, &mut report);
        Self::check_asset_paths(asset_paths, &mut report);
        Self::check_permissions(manifest, &mut report);

        report
    }

    /// Validate a manifest exists and has required fields.
    fn check_manifest(manifest: &PackageManifest, report: &mut ValidationReport) {
        if manifest.name.is_empty() {
            report.fail(ValidationCheck::new(
                "manifest_name",
                false,
                "Package name is empty".into(),
            ));
        } else {
            report.pass(ValidationCheck::new(
                "manifest_name",
                true,
                format!("Package name: {}", manifest.name),
            ));
        }

        if manifest.version.is_empty() {
            report.fail(ValidationCheck::new(
                "manifest_version",
                false,
                "Package version is empty".into(),
            ));
        } else {
            report.pass(ValidationCheck::new(
                "manifest_version",
                true,
                format!("Package version: {}", manifest.version),
            ));
        }

        if manifest.entry_point.is_empty() {
            report.fail(ValidationCheck::new(
                "manifest_entry",
                false,
                "Entry point is empty".into(),
            ));
        } else {
            report.pass(ValidationCheck::new(
                "manifest_entry",
                true,
                format!("Entry point: {}", manifest.entry_point),
            ));
        }
    }

    /// Verify the entry point exists in the package.
    fn check_entry_point(
        manifest: &PackageManifest,
        entry_data: Option<&[u8]>,
        report: &mut ValidationReport,
    ) {
        if manifest.entry_point.is_empty() {
            return;
        }
        match entry_data {
            Some(data) if !data.is_empty() => {
                report.pass(ValidationCheck::new(
                    "entry_point_exists",
                    true,
                    format!(
                        "Entry point '{}' found ({} bytes)",
                        manifest.entry_point,
                        data.len()
                    ),
                ));
            }
            Some(_) => {
                report.fail(ValidationCheck::new(
                    "entry_point_exists",
                    false,
                    format!("Entry point '{}' is empty", manifest.entry_point),
                ));
            }
            None => {
                report.fail(ValidationCheck::new(
                    "entry_point_exists",
                    false,
                    format!(
                        "Entry point '{}' not found in package",
                        manifest.entry_point
                    ),
                ));
            }
        }
    }

    /// Verify the package is compatible with the current engine version.
    fn check_version(
        manifest: &PackageManifest,
        engine_version: &str,
        report: &mut ValidationReport,
    ) {
        if let Some(ref required) = manifest.engine_version {
            if required != engine_version {
                report.fail(ValidationCheck::new(
                    "engine_compatibility",
                    false,
                    format!(
                        "Engine version mismatch: package requires {required}, engine is {engine_version}"
                    ),
                ));
                return;
            }
        }
        report.pass(ValidationCheck::new(
            "engine_compatibility",
            true,
            format!("Engine version: {engine_version}"),
        ));
    }

    /// Check that asset paths don't contain traversal attacks.
    fn check_asset_paths(asset_paths: &[String], report: &mut ValidationReport) {
        let mut all_safe = true;
        for path in asset_paths {
            if path.contains("..") || path.starts_with('/') || path.starts_with('\\') {
                report.fail(ValidationCheck::new(
                    "asset_path_traversal",
                    false,
                    format!("Path traversal detected: {path}"),
                ));
                all_safe = false;
            }
        }
        if all_safe {
            report.pass(ValidationCheck::new(
                "asset_path_traversal",
                true,
                format!("{} asset paths are safe", asset_paths.len()),
            ));
        }
    }

    /// Verify the package has declared required permissions.
    fn check_permissions(manifest: &PackageManifest, report: &mut ValidationReport) {
        let valid_perms = ["storage", "network", "audio", "input", "display"];
        for perm in &manifest.permissions {
            if !valid_perms.contains(&perm.as_str()) {
                report.fail(ValidationCheck::new(
                    "permission_valid",
                    false,
                    format!("Unknown permission: {perm}"),
                ));
            }
        }
        if manifest.permissions.is_empty() {
            report.pass(ValidationCheck::new(
                "permissions",
                true,
                "No permissions required".into(),
            ));
        } else {
            report.pass(ValidationCheck::new(
                "permissions",
                true,
                format!("Permissions: {}", manifest.permissions.join(", ")),
            ));
        }
    }

    /// Sanitize a path to prevent traversal attacks.
    pub fn sanitize_path(base: &Path, entry_path: &str) -> Result<PathBuf, RuntimeError> {
        let sanitized = entry_path
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string();

        if sanitized.contains("..") {
            return Err(RuntimeError::AssetPathTraversal(entry_path.to_string()));
        }

        let result = base.join(&sanitized);
        if !result.starts_with(base) {
            return Err(RuntimeError::AssetPathTraversal(entry_path.to_string()));
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> PackageManifest {
        PackageManifest::new("test_game", "1.0.0", "src/main.lua")
    }

    #[test]
    fn test_validate_valid_package() {
        let m = valid_manifest();
        let report = PackageValidator::validate(&m, Some(b"print('hello')"), &[], "0.2.0-alpha.1");
        assert!(
            report.passed,
            "Valid package should pass: {}",
            report.summary()
        );
    }

    #[test]
    fn test_validate_empty_name() {
        let m = PackageManifest::new("", "1.0.0", "main.lua");
        let report = PackageValidator::validate(&m, Some(b"data"), &[], "0.2.0-alpha.1");
        assert!(!report.passed);
        assert!(report.failures().iter().any(|c| c.name == "manifest_name"));
    }

    #[test]
    fn test_validate_empty_version() {
        let m = PackageManifest::new("test", "", "main.lua");
        let report = PackageValidator::validate(&m, Some(b"data"), &[], "0.2.0-alpha.1");
        assert!(!report.passed);
        assert!(
            report
                .failures()
                .iter()
                .any(|c| c.name == "manifest_version")
        );
    }

    #[test]
    fn test_validate_missing_entry_point() {
        let m = PackageManifest::new("test", "1.0.0", "");
        let report = PackageValidator::validate(&m, Some(b"data"), &[], "0.2.0-alpha.1");
        assert!(!report.passed);
        assert!(report.failures().iter().any(|c| c.name == "manifest_entry"));
    }

    #[test]
    fn test_validate_entry_point_not_found() {
        let m = valid_manifest();
        let report = PackageValidator::validate(&m, None, &[], "0.2.0-alpha.1");
        assert!(!report.passed);
        assert!(
            report
                .failures()
                .iter()
                .any(|c| c.name == "entry_point_exists")
        );
    }

    #[test]
    fn test_validate_engine_version_mismatch() {
        let mut m = valid_manifest();
        m.engine_version = Some("1.0.0".into());
        let report = PackageValidator::validate(&m, Some(b"data"), &[], "0.2.0-alpha.1");
        assert!(!report.passed);
        assert!(
            report
                .failures()
                .iter()
                .any(|c| c.name == "engine_compatibility")
        );
    }

    #[test]
    fn test_validate_asset_path_traversal() {
        let paths = vec!["safe.lua".into(), "../etc/passwd".into()];
        let m = valid_manifest();
        let report = PackageValidator::validate(&m, Some(b"data"), &paths, "0.2.0-alpha.1");
        assert!(!report.passed);
        assert!(
            report
                .failures()
                .iter()
                .any(|c| c.name == "asset_path_traversal")
        );
    }

    #[test]
    fn test_validate_safe_asset_paths() {
        let paths = vec!["assets/sprites/player.png".into(), "src/main.lua".into()];
        let m = valid_manifest();
        let report = PackageValidator::validate(&m, Some(b"data"), &paths, "0.2.0-alpha.1");
        assert!(report.passed);
    }

    #[test]
    fn test_validate_unknown_permission() {
        let mut m = valid_manifest();
        m.permissions = vec!["unknown_perm".into()];
        let report = PackageValidator::validate(&m, Some(b"data"), &[], "0.2.0-alpha.1");
        assert!(!report.passed);
        assert!(
            report
                .failures()
                .iter()
                .any(|c| c.name == "permission_valid")
        );
    }

    #[test]
    fn test_report_summary() {
        let mut report = ValidationReport::new();
        report.pass(ValidationCheck::new("check1", true, "ok".into()));
        report.fail(ValidationCheck::new("check2", false, "fail".into()));
        assert_eq!(report.summary(), "1/2 checks passed, 1 failed");
    }

    #[test]
    fn test_sanitize_path_safe() {
        let base = Path::new("/tmp/games");
        let result = PackageValidator::sanitize_path(base, "src/main.lua").unwrap();
        assert_eq!(result, Path::new("/tmp/games/src/main.lua"));
    }

    #[test]
    fn test_sanitize_path_traversal() {
        let base = Path::new("/tmp/games");
        let result = PackageValidator::sanitize_path(base, "../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_path_absolute_in_package() {
        let base = Path::new("/tmp/games");
        let result = PackageValidator::sanitize_path(base, "/etc/passwd").unwrap();
        assert_eq!(result, Path::new("/tmp/games/etc/passwd"));
        assert!(result.starts_with(base));
    }

    #[test]
    fn test_report_failures() {
        let mut report = ValidationReport::new();
        report.pass(ValidationCheck::new("a", true, "".into()));
        report.fail(ValidationCheck::new("b", false, "fail".into()));
        assert_eq!(report.failures().len(), 1);
        assert_eq!(report.failures()[0].name, "b");
    }
}
