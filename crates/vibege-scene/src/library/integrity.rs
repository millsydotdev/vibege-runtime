use super::models::InstalledGame;

/// Result of an integrity check.
#[derive(Debug, Clone)]
pub struct IntegrityReport {
    pub passed: bool,
    pub checks: Vec<IntegrityCheck>,
}

impl IntegrityReport {
    pub fn new() -> Self {
        Self {
            passed: true,
            checks: Vec::new(),
        }
    }

    pub fn summary(&self) -> String {
        let total = self.checks.len();
        let passed_count = self.checks.iter().filter(|c| c.passed).count();
        let failed_count = total - passed_count;
        format!("{passed_count}/{total} checks passed, {failed_count} failed")
    }
}

impl Default for IntegrityReport {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct IntegrityCheck {
    pub name: &'static str,
    pub passed: bool,
    pub message: String,
}

impl IntegrityCheck {
    pub fn new(name: &'static str, passed: bool, message: String) -> Self {
        Self {
            name,
            passed,
            message,
        }
    }
}

/// Checks the integrity of installed games.
pub struct IntegrityChecker;

impl IntegrityChecker {
    /// Run all integrity checks for a game.
    pub fn check(game: &InstalledGame) -> IntegrityReport {
        let mut report = IntegrityReport::new();

        Self::check_directory_exists(game, &mut report);
        Self::check_manifest_exists(game, &mut report);
        Self::check_entry_point_exists(game, &mut report);
        Self::check_engine_compatibility(game, &mut report);

        report
    }

    fn check_directory_exists(game: &InstalledGame, report: &mut IntegrityReport) {
        if game.path.exists() && game.path.is_dir() {
            report.checks.push(IntegrityCheck::new(
                "directory_exists",
                true,
                format!("Directory exists: {}", game.path.display()),
            ));
        } else {
            report.checks.push(IntegrityCheck::new(
                "directory_exists",
                false,
                format!("Directory missing: {}", game.path.display()),
            ));
            report.passed = false;
        }
    }

    fn check_manifest_exists(game: &InstalledGame, report: &mut IntegrityReport) {
        let meta_path = game.path.join(".vibege-install.json");
        if meta_path.exists() {
            report.checks.push(IntegrityCheck::new(
                "manifest_exists",
                true,
                "Manifest file exists".into(),
            ));
        } else {
            report.checks.push(IntegrityCheck::new(
                "manifest_exists",
                false,
                "Manifest file (.vibege-install.json) missing".into(),
            ));
            report.passed = false;
        }
    }

    fn check_entry_point_exists(game: &InstalledGame, report: &mut IntegrityReport) {
        let entry_path = game.path.join(&game.entry_point);
        if entry_path.exists() {
            report.checks.push(IntegrityCheck::new(
                "entry_point_exists",
                true,
                format!("Entry point exists: {}", game.entry_point),
            ));
        } else {
            report.checks.push(IntegrityCheck::new(
                "entry_point_exists",
                false,
                format!("Entry point missing: {}", game.entry_point),
            ));
            report.passed = false;
        }
    }

    fn check_engine_compatibility(game: &InstalledGame, report: &mut IntegrityReport) {
        if game.engine_version == "0.2.0-alpha.1" || game.engine_version.is_empty() {
            report.checks.push(IntegrityCheck::new(
                "engine_compatibility",
                true,
                format!("Engine version: {}", game.engine_version),
            ));
        } else {
            report.checks.push(IntegrityCheck::new(
                "engine_compatibility",
                true,
                format!(
                    "Engine version: {} (assuming compatible)",
                    game.engine_version
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn valid_game() -> InstalledGame {
        InstalledGame {
            name: "test".into(),
            path: PathBuf::from("/tmp/test_game"),
            entry_point: "main.lua".into(),
            version: "1.0".into(),
            author: "".into(),
            description: "".into(),
            installed_at: 0,
            last_played: 0,
            play_count: 0,
            total_play_time_secs: 0,
            size_bytes: 0,
            engine_version: "0.2.0-alpha.1".into(),
            category: "".into(),
            genres: vec![],
            tags: vec![],
            hidden: false,
            pinned: false,
        }
    }

    #[test]
    fn test_check_missing_directory() {
        let game = valid_game();
        let report = IntegrityChecker::check(&game);
        assert!(!report.passed);
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "directory_exists" && !c.passed)
        );
    }

    #[test]
    fn test_summary_format() {
        let mut report = IntegrityReport::new();
        report
            .checks
            .push(IntegrityCheck::new("a", true, "ok".into()));
        report
            .checks
            .push(IntegrityCheck::new("b", false, "fail".into()));
        assert_eq!(report.summary(), "1/2 checks passed, 1 failed");
    }
}
