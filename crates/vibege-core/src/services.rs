//! Service registry — formal initialization and shutdown ordering for runtime subsystems.
//!
//! Services declare dependencies. The registry initializes them in dependency order
//! and shuts them down in reverse order. This prevents init-order bugs and ensures
//! clean teardown.

use std::collections::HashMap;
use std::time::Instant;

use crate::error::{ErrorCode, Result, RuntimeError};

/// Status of a registered service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Pending,
    Initializing,
    Running,
    Failed,
    ShuttingDown,
    Stopped,
}

impl ServiceStatus {
    pub fn label(&self) -> &'static str {
        match self {
            ServiceStatus::Pending => "pending",
            ServiceStatus::Initializing => "initializing",
            ServiceStatus::Running => "running",
            ServiceStatus::Failed => "failed",
            ServiceStatus::ShuttingDown => "shutting_down",
            ServiceStatus::Stopped => "stopped",
        }
    }
}

/// A handle to a registered service for dependency declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceId(usize);

/// Initialization callback. Returns Ok(()) on success.
pub type InitFn = Box<dyn FnOnce() -> Result<()> + Send>;
/// Shutdown callback. Called during teardown.
pub type ShutdownFn = Box<dyn FnOnce() -> Result<()> + Send>;

struct ServiceEntry {
    id: ServiceId,
    name: &'static str,
    status: ServiceStatus,
    deps: Vec<ServiceId>,
    init: Option<InitFn>,
    shutdown: Option<ShutdownFn>,
    started_at: Option<Instant>,
}

/// Registry for runtime services with ordered initialization and shutdown.
pub struct ServiceRegistry {
    services: Vec<ServiceEntry>,
    init_order: Vec<ServiceId>,
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            init_order: Vec::new(),
        }
    }

    /// Register a service with optional init and shutdown callbacks.
    pub fn register(
        &mut self,
        name: &'static str,
        init: Option<InitFn>,
        shutdown: Option<ShutdownFn>,
    ) -> ServiceId {
        let id = ServiceId(self.services.len());
        self.services.push(ServiceEntry {
            id,
            name,
            status: ServiceStatus::Pending,
            deps: Vec::new(),
            init,
            shutdown,
            started_at: None,
        });
        id
    }

    /// Declare that `service` depends on `dependency`.
    pub fn depends_on(&mut self, service: ServiceId, dependency: ServiceId) {
        if let Some(entry) = self.services.iter_mut().find(|e| e.id == service) {
            entry.deps.push(dependency);
        }
    }

    /// Initialize all services in dependency order.
    /// Returns the ordered list of service IDs that were initialized.
    pub fn initialize(
        &mut self,
        diagnostics: &crate::diagnostics::Diagnostics,
    ) -> Result<Vec<&'static str>> {
        let order = self.compute_init_order()?;
        let mut initialized = Vec::new();

        for &id in &order {
            let (name, has_init) = {
                let entry = &self.services[id.0];
                (entry.name, entry.init.is_some())
            };
            if has_init {
                let entry = &mut self.services[id.0];
                entry.status = ServiceStatus::Initializing;
                entry.started_at = Some(Instant::now());

                let init_fn = entry.init.take().unwrap();
                match init_fn() {
                    Ok(()) => {
                        entry.status = ServiceStatus::Running;
                        diagnostics.register_simple(name, true, "initialized".to_string());
                        initialized.push(name);
                        tracing::info!(service = name, "Service initialized");
                    }
                    Err(e) => {
                        entry.status = ServiceStatus::Failed;
                        diagnostics.register_simple(name, false, format!("init failed: {e}"));
                        tracing::error!(service = name, error = %e, "Service initialization failed");
                        return Err(RuntimeError::with_cause(
                            ErrorCode::INIT_SUBSYSTEM_FAILED,
                            format!("Service '{name}' failed to initialize"),
                            e,
                        ));
                    }
                }
            } else {
                let entry = &mut self.services[id.0];
                entry.status = ServiceStatus::Running;
                entry.started_at = Some(Instant::now());
                initialized.push(name);
            }
        }

        self.init_order = order;
        Ok(initialized)
    }

    /// Shut down all services in reverse initialization order.
    pub fn shutdown(&mut self) -> Vec<(&'static str, Result<()>)> {
        let mut results = Vec::new();
        for &id in self.init_order.iter().rev() {
            let entry = &mut self.services[id.0];
            entry.status = ServiceStatus::ShuttingDown;

            if let Some(shutdown_fn) = entry.shutdown.take() {
                let result = shutdown_fn();
                match &result {
                    Ok(()) => {
                        entry.status = ServiceStatus::Stopped;
                        tracing::info!(service = entry.name, "Service stopped");
                    }
                    Err(e) => {
                        entry.status = ServiceStatus::Failed;
                        tracing::error!(service = entry.name, error = %e, "Service shutdown error");
                    }
                }
                results.push((entry.name, result));
            } else {
                entry.status = ServiceStatus::Stopped;
            }
        }
        results
    }

    /// Returns the status of a service by ID.
    pub fn status(&self, id: ServiceId) -> ServiceStatus {
        self.services
            .get(id.0)
            .map(|e| e.status)
            .unwrap_or(ServiceStatus::Stopped)
    }

    /// Returns the status of a service by name.
    pub fn status_by_name(&self, name: &str) -> Option<ServiceStatus> {
        self.services
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.status)
    }

    /// Returns names of all services with their current status.
    pub fn all_statuses(&self) -> Vec<(&'static str, ServiceStatus)> {
        self.services.iter().map(|e| (e.name, e.status)).collect()
    }

    /// Returns the number of registered services.
    pub fn len(&self) -> usize {
        self.services.len()
    }

    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }

    /// Topological sort of services by dependency order.
    /// Kahn's algorithm. Returns error if a cycle is detected.
    fn compute_init_order(&self) -> Result<Vec<ServiceId>> {
        let n = self.services.len();
        let mut in_degree = vec![0usize; n];
        let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();

        for entry in &self.services {
            for &dep in &entry.deps {
                adj.entry(dep.0).or_default().push(entry.id.0);
                in_degree[entry.id.0] += 1;
            }
        }

        let mut queue: Vec<usize> = in_degree
            .iter()
            .enumerate()
            .filter(|(_, deg)| **deg == 0)
            .map(|(i, _)| i)
            .collect();

        let mut order = Vec::new();
        while let Some(idx) = queue.pop() {
            order.push(ServiceId(idx));
            if let Some(neighbors) = adj.remove(&idx) {
                for &next in &neighbors {
                    in_degree[next] -= 1;
                    if in_degree[next] == 0 {
                        queue.push(next);
                    }
                }
            }
        }

        if order.len() != n {
            return Err(RuntimeError::new(
                ErrorCode::INIT_FAILED,
                "Service dependency cycle detected — cannot determine init order",
            ));
        }

        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Diagnostics;
    use crate::error::{ErrorCode, RuntimeError};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_empty_registry() {
        let reg = ServiceRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_register_service() {
        let mut reg = ServiceRegistry::new();
        let id = reg.register("test", None, None);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.status(id), ServiceStatus::Pending);
    }

    #[test]
    fn test_initialize_single_service() {
        let mut reg = ServiceRegistry::new();
        let diag = Diagnostics::new();
        reg.register("renderer", Some(Box::new(|| Ok(()))), None);
        let result = reg.initialize(&diag);
        assert!(result.is_ok());
        assert_eq!(reg.status_by_name("renderer"), Some(ServiceStatus::Running));
    }

    #[test]
    fn test_initialize_failure() {
        let mut reg = ServiceRegistry::new();
        let diag = Diagnostics::new();
        reg.register(
            "broken",
            Some(Box::new(|| {
                Err(RuntimeError::new(ErrorCode::INIT_FAILED, "oops"))
            })),
            None,
        );
        let result = reg.initialize(&diag);
        assert!(result.is_err());
        assert_eq!(reg.status_by_name("broken"), Some(ServiceStatus::Failed));
    }

    #[test]
    fn test_init_order_respects_dependencies() {
        let mut reg = ServiceRegistry::new();
        let diag = Diagnostics::new();
        let order = Arc::new(AtomicUsize::new(0));
        let o1 = Arc::clone(&order);
        let audio = reg.register(
            "audio",
            Some(Box::new(move || {
                assert_eq!(o1.load(Ordering::SeqCst), 0, "audio must init first");
                o1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })),
            None,
        );
        let o2 = Arc::clone(&order);
        let renderer = reg.register(
            "renderer",
            Some(Box::new(move || {
                assert_eq!(
                    o2.load(Ordering::SeqCst),
                    1,
                    "renderer must init after audio"
                );
                o2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })),
            None,
        );
        reg.depends_on(renderer, audio);
        let result = reg.initialize(&diag);
        assert!(result.is_ok());
        assert_eq!(order.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_shutdown_reverse_order() {
        let mut reg = ServiceRegistry::new();
        let diag = Diagnostics::new();
        let order = Arc::new(AtomicUsize::new(0));
        let o1 = Arc::clone(&order);
        let a = reg.register(
            "a",
            None,
            Some(Box::new(move || {
                assert_eq!(o1.load(Ordering::SeqCst), 1, "a must shutdown second");
                o1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })),
        );
        let o2 = Arc::clone(&order);
        let b = reg.register(
            "b",
            None,
            Some(Box::new(move || {
                assert_eq!(o2.load(Ordering::SeqCst), 0, "b must shutdown first");
                o2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })),
        );
        reg.depends_on(b, a);
        reg.initialize(&diag).ok();
        let results = reg.shutdown();
        assert_eq!(results.len(), 2);
        assert_eq!(order.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_cycle_detection() {
        let mut reg = ServiceRegistry::new();
        let diag = Diagnostics::new();
        let a = reg.register("a", None, None);
        let b = reg.register("b", None, None);
        reg.depends_on(a, b);
        reg.depends_on(b, a);
        let result = reg.initialize(&diag);
        assert!(result.is_err());
    }

    #[test]
    fn test_all_statuses() {
        let mut reg = ServiceRegistry::new();
        reg.register("a", None, None);
        reg.register("b", None, None);
        let statuses = reg.all_statuses();
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].1, ServiceStatus::Pending);
    }

    #[test]
    fn test_service_status_label() {
        assert_eq!(ServiceStatus::Pending.label(), "pending");
        assert_eq!(ServiceStatus::Running.label(), "running");
        assert_eq!(ServiceStatus::Failed.label(), "failed");
        assert_eq!(ServiceStatus::Stopped.label(), "stopped");
    }
}
