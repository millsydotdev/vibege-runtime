use std::backtrace::Backtrace;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static CRASH_REPORT_DIR: OnceLock<PathBuf> = OnceLock::new();
static PANIC_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// A structured crash report written to disk on panic.
#[derive(Debug, serde::Serialize)]
pub struct CrashReport {
    pub timestamp: String,
    pub version: String,
    pub panic_message: String,
    pub backtrace: String,
    pub uptime_secs: f64,
    pub location: Option<String>,
}

/// Installs a custom panic hook that captures backtraces and writes crash dumps.
///
/// The crash dump is written to `{crash_dir}/crash_{timestamp}.json`.
/// The crash directory defaults to the current directory, but can be configured
/// via `set_crash_report_dir()`.
///
/// This function is idempotent — calling it multiple times only installs the
/// hook once. The original panic hook is preserved and called after the crash
/// dump is written.
pub fn install_panic_hook() {
    if PANIC_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        return; // already installed
    }

    let previous_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()));

        let backtrace = Backtrace::capture();

        let report = CrashReport {
            timestamp: chrono_now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            panic_message: message,
            backtrace: format!("{backtrace:#}"),
            uptime_secs: 0.0, // set by caller if available
            location,
        };

        // Write crash dump to file
        let dir = CRASH_REPORT_DIR
            .get()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        if let Err(e) = write_crash_report(&dir, &report) {
            eprintln!("[VIBEGE] Failed to write crash report: {e}");
        }

        // Log the crash
        eprintln!(
            "[VIBEGE] Panic: {message} at {loc}",
            message = report.panic_message,
            loc = report.location.as_deref().unwrap_or("unknown"),
        );
        eprintln!("[VIBEGE] Crash report written to: {dir:?}");
        eprintln!("[VIBEGE] Backtrace:\n{bt}", bt = report.backtrace);

        // Call the previous hook for OS-level handling
        previous_hook(panic_info);
    }));
}

/// Sets the directory where crash reports are written.
pub fn set_crash_report_dir(path: PathBuf) {
    let _ = CRASH_REPORT_DIR.set(path);
}

fn write_crash_report(
    dir: &PathBuf,
    report: &CrashReport,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(dir)?;

    let timestamp = report.timestamp.replace([':', '.'], "-");
    let filename = format!("crash_{timestamp}.json");
    let filepath = dir.join(&filename);

    let json = serde_json::to_string_pretty(report)?;
    let mut file = fs::File::create(&filepath)?;
    file.write_all(json.as_bytes())?;

    tracing::error!(path = %filepath.display(), "Crash report written");
    Ok(())
}

fn chrono_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}", d.as_secs_f64()))
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_panic_hook_is_idempotent() {
        install_panic_hook();
        install_panic_hook(); // should not panic or double-install
    }

    #[test]
    fn test_crash_report_serialization() {
        let report = CrashReport {
            timestamp: "0d00h00m00s.000000000ns".to_string(),
            version: "0.1.0".to_string(),
            panic_message: "test panic".to_string(),
            backtrace: "stack backtrace here".to_string(),
            uptime_secs: 42.0,
            location: Some("src/main.rs:10:5".to_string()),
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("test panic"));
        assert!(json.contains("src/main.rs"));
    }

    #[test]
    fn test_write_crash_report() {
        let dir = tempfile::tempdir().unwrap();
        let report = CrashReport {
            timestamp: "test_timestamp".to_string(),
            version: "0.1.0".to_string(),
            panic_message: "write test".to_string(),
            backtrace: "bt".to_string(),
            uptime_secs: 1.0,
            location: None,
        };

        assert!(write_crash_report(&dir.path().to_path_buf(), &report).is_ok());

        // Verify file was written
        let entries = fs::read_dir(dir.path()).unwrap();
        let count = entries.count();
        assert_eq!(count, 1, "Should have written one crash file");
    }

    #[test]
    fn test_set_crash_report_dir() {
        let dir = tempfile::tempdir().unwrap();
        set_crash_report_dir(dir.path().to_path_buf());
        // Should not panic on subsequent sets
        set_crash_report_dir(dir.path().to_path_buf());
    }
}
