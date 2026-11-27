//! Lifecycle pool for isolated service modules managed by `evohime-supervisor`.
//!
//! Each extracted Core service (`evals`, `media-voice`, `research`,
//! `tools-workspace`, `tools-exec`, `model-adapters`, etc.) can be started,
//! stopped, and hot-restarted individually without restarting `evohime-core.exe`
//! or `EvoHime.exe`.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Runtime state of an isolated service module inside [`ServicePool`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedServiceState {
    /// Service is registered and not currently running.
    Stopped,
    /// Service process is running.
    Running {
        /// Process identifier reported by the OS.
        pid: u32,
    },
    /// Service is undergoing a targeted hot-restart after an update.
    Restarting,
    /// Service failed to start or exited unexpectedly.
    Failed {
        /// Consecutive failure count for backoff decisions.
        consecutive_failures: u32,
        /// Diagnostic summary without secrets.
        reason: String,
    },
}

/// Registration entry for one isolated service module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedServiceEntry {
    /// Canonical module identifier (e.g. `tools-workspace`, `model-adapters`).
    pub service_id: String,
    /// Path to the service executable.
    pub executable: PathBuf,
    /// Current lifecycle state.
    pub state: ManagedServiceState,
    /// Number of completed targeted hot-restarts.
    pub restart_count: u64,
}

/// Registry and state tracker for isolated service modules.
#[derive(Debug, Default)]
pub struct ServicePool {
    services: BTreeMap<String, ManagedServiceEntry>,
}

impl ServicePool {
    /// Creates an empty [`ServicePool`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or updates a service executable path in the pool.
    pub fn register(&mut self, service_id: impl Into<String>, executable: PathBuf) {
        let service_id = service_id.into();
        self.services
            .entry(service_id.clone())
            .and_modify(|entry| entry.executable = executable.clone())
            .or_insert(ManagedServiceEntry {
                service_id,
                executable,
                state: ManagedServiceState::Stopped,
                restart_count: 0,
            });
    }

    /// Marks a service as running with `pid`.
    pub fn mark_running(&mut self, service_id: &str, pid: u32) -> Result<(), String> {
        let entry = self
            .services
            .get_mut(service_id)
            .ok_or_else(|| format!("unknown service module: {service_id}"))?;
        entry.state = ManagedServiceState::Running { pid };
        Ok(())
    }

    /// Transitions only `service_id` through a targeted hot-restart without
    /// affecting any other running services or Core.
    pub fn begin_hot_restart(&mut self, service_id: &str) -> Result<PathBuf, String> {
        let entry = self
            .services
            .get_mut(service_id)
            .ok_or_else(|| format!("unknown service module: {service_id}"))?;
        entry.state = ManagedServiceState::Restarting;
        entry.restart_count = entry.restart_count.saturating_add(1);
        Ok(entry.executable.clone())
    }

    /// Marks a service as failed, incrementing its failure counter.
    pub fn mark_failed(
        &mut self,
        service_id: &str,
        reason: impl Into<String>,
    ) -> Result<(), String> {
        let entry = self
            .services
            .get_mut(service_id)
            .ok_or_else(|| format!("unknown service module: {service_id}"))?;
        let prev = match entry.state {
            ManagedServiceState::Failed {
                consecutive_failures,
                ..
            } => consecutive_failures,
            _ => 0,
        };
        entry.state = ManagedServiceState::Failed {
            consecutive_failures: prev.saturating_add(1),
            reason: reason.into(),
        };
        Ok(())
    }

    /// Returns the entry for `service_id`, if registered.
    pub fn get(&self, service_id: &str) -> Option<&ManagedServiceEntry> {
        self.services.get(service_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targeted_hot_restart_only_affects_target_service() {
        let mut pool = ServicePool::new();
        pool.register(
            "tools-workspace",
            PathBuf::from("evohime-tools-workspace.exe"),
        );
        pool.register(
            "model-adapters",
            PathBuf::from("evohime-model-adapters.exe"),
        );

        pool.mark_running("tools-workspace", 101)
            .expect("mark running");
        pool.mark_running("model-adapters", 202)
            .expect("mark running");

        let exe = pool
            .begin_hot_restart("tools-workspace")
            .expect("begin hot restart");
        assert_eq!(exe, PathBuf::from("evohime-tools-workspace.exe"));

        assert_eq!(
            pool.get("tools-workspace").expect("entry").state,
            ManagedServiceState::Restarting
        );
        assert_eq!(pool.get("tools-workspace").expect("entry").restart_count, 1);
        assert_eq!(
            pool.get("model-adapters").expect("entry").state,
            ManagedServiceState::Running { pid: 202 }
        );
    }
}
