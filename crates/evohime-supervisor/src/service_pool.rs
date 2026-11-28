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
    #[allow(dead_code)]
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

/// Canonical image names of the isolated service-module executables that Core
/// spawns as its own children and that the supervisor is allowed to observe
/// and hot-restart. Nothing outside this list may ever be terminated.
const SERVICE_MODULE_EXECUTABLES: [&str; 13] = [
    "evohime-evals.exe",
    "evohime-media-voice.exe",
    "evohime-research.exe",
    "evohime-remote.exe",
    "evohime-ambient.exe",
    "evohime-computer-use.exe",
    "evohime-workflow.exe",
    "evohime-rag-memory.exe",
    "evohime-tools-workspace.exe",
    "evohime-tools-exec.exe",
    "evohime-model-adapters.exe",
    "evohime-model-router.exe",
    "evohime-verify.exe",
];

/// Returns `true` only for the isolated service-module image names
/// (case-insensitive, exact match including the `.exe` suffix). Everything
/// else — including `EvoHime.exe`, `evohime-core.exe`, `evohime-supervisor.exe`,
/// `evohime-transaction.exe`, `eva.exe` and any renamed or suffixed copy — is
/// rejected, so the supervisor can never target those processes.
pub fn is_service_module_executable(image_name: &str) -> bool {
    let normalized = image_name.to_ascii_lowercase();
    SERVICE_MODULE_EXECUTABLES.contains(&normalized.as_str())
}

/// Maps a service-module image name to its canonical service identifier
/// (`evohime-tools-workspace.exe` → `tools-workspace`). Returns `None` for
/// anything that is not a service-module executable.
pub fn service_id_for_image_name(image_name: &str) -> Option<String> {
    let normalized = image_name.to_ascii_lowercase();
    let stem = normalized.strip_suffix(".exe")?;
    let id = stem.strip_prefix("evohime-")?;
    if is_service_module_executable(&normalized) {
        Some(id.to_owned())
    } else {
        None
    }
}

/// Decides whether a service-module binary on disk is newer than the moment
/// the observed process started, i.e. the executable was replaced while the
/// process kept running the previous version (an interrupted update). Equal
/// timestamps count as current: only a strictly newer binary is stale.
pub fn service_process_is_stale(exe_mtime_ms: u64, process_start_ms: u64) -> bool {
    exe_mtime_ms > process_start_ms
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
    #[allow(dead_code)]
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

    /// Registers a freshly observed service-module process (if the service is
    /// new) and reflects its liveness in the pool.
    ///
    /// When the binary on disk is newer than the process start time, the
    /// process is stale: the service transitions to
    /// [`ManagedServiceState::Restarting`] (idempotently — repeated
    /// observations of the same stale process do not inflate
    /// `restart_count`) and the caller is told to terminate exactly this
    /// process. Core then lazily respawns the service with the fresh binary
    /// and a later observation marks it `Running` again.
    ///
    /// Returns `Some(service_id)` when the observed process is stale and must
    /// be hot-restarted, `None` when it is current. Unknown image names are
    /// ignored.
    pub fn observe_service_process(
        &mut self,
        image_name: &str,
        pid: u32,
        executable: PathBuf,
        exe_mtime_ms: u64,
        process_start_ms: u64,
    ) -> Option<String> {
        let service_id = service_id_for_image_name(image_name)?;
        self.register(service_id.clone(), executable);
        if service_process_is_stale(exe_mtime_ms, process_start_ms) {
            let entry = self.services.get_mut(&service_id)?;
            if !matches!(entry.state, ManagedServiceState::Restarting) {
                entry.state = ManagedServiceState::Restarting;
                entry.restart_count = entry.restart_count.saturating_add(1);
            }
            return Some(service_id);
        }
        let _ = self.mark_running(&service_id, pid);
        None
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

    #[test]
    fn recognizes_only_service_module_image_names() {
        for name in [
            "evohime-evals.exe",
            "evohime-media-voice.exe",
            "evohime-research.exe",
            "evohime-remote.exe",
            "evohime-ambient.exe",
            "evohime-computer-use.exe",
            "evohime-workflow.exe",
            "evohime-rag-memory.exe",
            "evohime-tools-workspace.exe",
            "evohime-tools-exec.exe",
            "evohime-model-adapters.exe",
            "evohime-model-router.exe",
            "evohime-verify.exe",
        ] {
            assert!(is_service_module_executable(name), "{name} must match");
            assert!(
                is_service_module_executable(&name.to_ascii_uppercase()),
                "{name} must match case-insensitively"
            );
        }
        for name in [
            "EvoHime.exe",
            "evohime-core.exe",
            "evohime-supervisor.exe",
            "evohime-transaction.exe",
            "eva.exe",
            "evohime-evals.exe.bak",
            "evohime-evals-old.exe",
            "evohime-listener.exe",
            "evohime-hive-daemon.exe",
            "notepad.exe",
            "",
        ] {
            assert!(!is_service_module_executable(name), "{name} must not match");
        }
    }

    #[test]
    fn service_id_comes_from_image_name_for_known_services_only() {
        assert_eq!(
            service_id_for_image_name("EVOHIME-TOOLS-WORKSPACE.EXE"),
            Some("tools-workspace".to_owned())
        );
        assert_eq!(
            service_id_for_image_name("evohime-model-router.exe"),
            Some("model-router".to_owned())
        );
        assert_eq!(service_id_for_image_name("evohime-core.exe"), None);
        assert_eq!(service_id_for_image_name("eva.exe"), None);
        assert_eq!(service_id_for_image_name("evohime-evals.exe.bak"), None);
    }

    #[test]
    fn stale_detection_requires_binary_newer_than_process_start() {
        // Бинарник заменён после старта процесса — процесс устарел.
        assert!(service_process_is_stale(2_000, 1_000));
        // Равные метки: процесс запущен ровно с текущей версией.
        assert!(!service_process_is_stale(1_000, 1_000));
        // Бинарник старее процесса — обновления не было.
        assert!(!service_process_is_stale(500, 1_000));
    }

    #[test]
    fn fresh_service_process_is_registered_and_marked_running() {
        let mut pool = ServicePool::new();
        let stale = pool.observe_service_process(
            "evohime-research.exe",
            4242,
            PathBuf::from("C:/install/evohime-research.exe"),
            1_000,
            2_000,
        );
        assert_eq!(stale, None);
        let entry = pool.get("research").expect("service registered");
        assert_eq!(entry.state, ManagedServiceState::Running { pid: 4242 });
        assert_eq!(entry.restart_count, 0);
    }

    #[test]
    fn stale_service_process_transitions_to_restarting_idempotently() {
        let mut pool = ServicePool::new();
        // Первый тик: бинарник новее процесса — нужен hot-restart.
        let first = pool.observe_service_process(
            "evohime-research.exe",
            4242,
            PathBuf::from("C:/install/evohime-research.exe"),
            3_000,
            2_000,
        );
        assert_eq!(first, Some("research".to_owned()));
        assert_eq!(
            pool.get("research").expect("entry").state,
            ManagedServiceState::Restarting
        );
        assert_eq!(pool.get("research").expect("entry").restart_count, 1);

        // Повторное наблюдение того же устаревшего процесса (терминация ещё
        // не удалась или новый процесс ещё не поднялся) не наращивает счётчик.
        let repeat = pool.observe_service_process(
            "evohime-research.exe",
            4242,
            PathBuf::from("C:/install/evohime-research.exe"),
            3_000,
            2_000,
        );
        assert_eq!(repeat, Some("research".to_owned()));
        assert_eq!(pool.get("research").expect("entry").restart_count, 1);
        assert_eq!(
            pool.get("research").expect("entry").state,
            ManagedServiceState::Restarting
        );

        // Ядро лениво перезапустило сервис с новым бинарником: новое
        // наблюдение снова помечает его Running.
        let restarted = pool.observe_service_process(
            "evohime-research.exe",
            5151,
            PathBuf::from("C:/install/evohime-research.exe"),
            3_000,
            4_000,
        );
        assert_eq!(restarted, None);
        assert_eq!(
            pool.get("research").expect("entry").state,
            ManagedServiceState::Running { pid: 5151 }
        );
        assert_eq!(pool.get("research").expect("entry").restart_count, 1);
    }

    #[test]
    fn observing_non_service_image_names_is_ignored() {
        let mut pool = ServicePool::new();
        for name in [
            "EvoHime.exe",
            "evohime-core.exe",
            "evohime-supervisor.exe",
            "evohime-transaction.exe",
            "eva.exe",
        ] {
            let stale = pool.observe_service_process(
                name,
                7,
                PathBuf::from(format!("C:/install/{name}")),
                9_999,
                1,
            );
            assert_eq!(stale, None, "{name} must never be targeted");
        }
        assert!(
            pool.get("").is_none(),
            "unknown image names must not register services"
        );
    }
}
