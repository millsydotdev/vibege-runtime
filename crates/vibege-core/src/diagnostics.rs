//! Runtime diagnostics — health checks, subsystem status, and metrics reporting.
//!
//! Every subsystem reports its health via a [`HealthReport`]. The diagnostics
//! system aggregates these into a comprehensive runtime health snapshot.

use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Health status of a single subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
    NotStarted,
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    pub fn label(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded(_) => "degraded",
            HealthStatus::Unhealthy(_) => "unhealthy",
            HealthStatus::NotStarted => "not_started",
        }
    }
}

/// A snapshot of a single subsystem's health.
#[derive(Debug, Clone)]
pub struct HealthReport {
    pub subsystem: &'static str,
    pub status: HealthStatus,
    pub uptime_secs: f64,
    pub detail: String,
}

/// Aggregate snapshot of the entire runtime's health.
#[derive(Debug, Clone)]
pub struct RuntimeHealth {
    pub reports: Vec<HealthReport>,
    pub overall: HealthStatus,
    pub uptime_secs: f64,
    pub started_at: String,
}

/// A health-check callback for a single subsystem.
pub type HealthCheck = Arc<dyn Fn() -> HealthReport + Send + Sync>;

/// Central diagnostics collector.
///
/// Subsystems register health-check callbacks via [`Diagnostics::register`].
/// The runtime calls [`Diagnostics::report`] to get an aggregate health snapshot.
pub struct Diagnostics {
    started_at: Instant,
    checks: Mutex<Vec<HealthCheck>>,
}

impl Diagnostics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started_at: Instant::now(),
            checks: Mutex::new(Vec::new()),
        })
    }

    /// Register a health-check callback for a subsystem.
    pub fn register(&self, _subsystem: &'static str, check: HealthCheck) {
        if let Ok(mut checks) = self.checks.lock() {
            checks.push(check);
        }
    }

    /// Helper: register a simple healthy/unhealthy check.
    pub fn register_simple(&self, subsystem: &'static str, healthy: bool, detail: String) {
        let d = detail.clone();
        let started = Instant::now();
        let check: HealthCheck = Arc::new(move || HealthReport {
            subsystem,
            status: if healthy {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy(d.clone())
            },
            uptime_secs: started.elapsed().as_secs_f64(),
            detail: d.clone(),
        });
        self.register(subsystem, check);
    }

    /// Collect health reports from all registered subsystems.
    pub fn report(&self) -> RuntimeHealth {
        let uptime = self.started_at.elapsed();
        let mut reports = Vec::new();
        if let Ok(checks) = self.checks.lock() {
            for check in checks.iter() {
                reports.push(check());
            }
        }
        let overall = if reports.iter().all(|r| r.status.is_healthy()) {
            HealthStatus::Healthy
        } else if reports
            .iter()
            .any(|r| matches!(r.status, HealthStatus::Unhealthy(_)))
        {
            HealthStatus::Unhealthy("One or more subsystems are unhealthy".into())
        } else {
            HealthStatus::Degraded("Some subsystems are degraded".into())
        };
        RuntimeHealth {
            reports,
            overall,
            uptime_secs: uptime.as_secs_f64(),
            started_at: format!("{}.{:09}s", uptime.as_secs(), uptime.subsec_nanos()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics_new() {
        let d = Diagnostics::new();
        let health = d.report();
        assert!(health.reports.is_empty());
        assert!(health.uptime_secs >= 0.0);
    }

    #[test]
    fn test_register_and_report_healthy() {
        let d = Diagnostics::new();
        d.register_simple("renderer", true, "running".into());
        let health = d.report();
        assert_eq!(health.reports.len(), 1);
        assert_eq!(health.reports[0].subsystem, "renderer");
        assert_eq!(health.reports[0].status, HealthStatus::Healthy);
    }

    #[test]
    fn test_register_and_report_unhealthy() {
        let d = Diagnostics::new();
        d.register_simple("audio", false, "device not found".into());
        let health = d.report();
        assert_eq!(health.reports.len(), 1);
        assert!(matches!(
            health.reports[0].status,
            HealthStatus::Unhealthy(_)
        ));
    }

    #[test]
    fn test_overall_healthy() {
        let d = Diagnostics::new();
        d.register_simple("a", true, "ok".into());
        d.register_simple("b", true, "ok".into());
        let health = d.report();
        assert_eq!(health.overall, HealthStatus::Healthy);
    }

    #[test]
    fn test_overall_unhealthy() {
        let d = Diagnostics::new();
        d.register_simple("a", true, "ok".into());
        d.register_simple("b", false, "failed".into());
        let health = d.report();
        assert!(matches!(health.overall, HealthStatus::Unhealthy(_)));
    }

    #[test]
    fn test_health_status_labels() {
        assert_eq!(HealthStatus::Healthy.label(), "healthy");
        assert_eq!(HealthStatus::Degraded("".into()).label(), "degraded");
        assert_eq!(HealthStatus::Unhealthy("".into()).label(), "unhealthy");
        assert_eq!(HealthStatus::NotStarted.label(), "not_started");
    }

    #[test]
    fn test_is_healthy() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(!HealthStatus::Unhealthy("".into()).is_healthy());
    }
}
