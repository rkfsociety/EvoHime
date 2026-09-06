use super::*;

#[derive(Clone)]
pub struct EventJournal {
    pub(crate) database: Arc<Mutex<LocalDatabase>>,
    pub(crate) database_path: Arc<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableReplayBatch {
    pub events: Vec<EventRecord>,
    pub gap_detected: bool,
    pub first_available_sequence: Option<i64>,
    pub last_sequence: i64,
}

pub(crate) fn default_build_policy() -> crate::scope::BuildScope {
    crate::scope::BuildScope {
        allowed_paths: Vec::new(),
        allowed_operations: vec!["write".into(), "create".into()],
        expected_outputs: Vec::new(),
        protected_paths: vec![".git".into(), ".evohime".into()],
        allowed_file_types: Vec::new(),
        max_files_changed: 20,
        max_bytes_changed: 2 * 1024 * 1024,
        allow_create: true,
        allow_delete: false,
        allow_rename: false,
        baseline_snapshot_id: None,
        acceptance_criteria: String::new(),
        risk_class: "medium".into(),
        timeout_ms: 30_000,
    }
}

pub(crate) fn harden_build_policy(
    mut policy: crate::scope::BuildScope,
) -> crate::scope::BuildScope {
    for required in [".git", ".evohime"] {
        if !policy.protected_paths.iter().any(|path| path == required) {
            policy.protected_paths.push(required.into());
        }
    }
    policy
}

pub(crate) fn safe_file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("database-backup")
        .chars()
        .take(128)
        .collect()
}

pub(crate) fn safe_file_stem(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("events")
        .chars()
        .filter(|value| value.is_ascii_alphanumeric() || *value == '-' || *value == '_')
        .take(64)
        .collect::<String>()
        .trim()
        .to_owned()
}

pub(crate) fn error_category(error: &str) -> &'static str {
    if error.contains("checksum") {
        "checksum"
    } else if error.contains("schema") {
        "schema"
    } else if error.contains("approval") {
        "approval"
    } else if error.contains("destination") {
        "destination"
    } else {
        "storage"
    }
}

/// Параметры одной метрики инструмента, записываемой в durable journal.
#[derive(Debug, Clone, Copy)]
pub struct ToolMetric<'a> {
    pub task_id: &'a str,
    pub tool_name: &'a str,
    pub iteration: usize,
    pub ok: bool,
    pub failure_kind: Option<&'a str>,
    pub recovery_hint: bool,
    pub escalated: bool,
}

/// Параметры перехода durable recovery state machine.
#[derive(Debug, Clone, Copy)]
pub struct RecoveryTransition<'a> {
    pub run_id: &'a str,
    pub state: RecoveryState,
    pub effect_id: &'a str,
    pub idempotency_key: &'a str,
    pub verifier: &'a str,
    pub evidence_json: &'a [u8],
    pub decision: &'a str,
}

/// Данные session-only заметки, которые никогда не становятся persistent memory.
#[derive(Debug, Clone, Copy)]
pub struct SessionMemoryNote<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub scope: evohime_local_storage::domains::memory::MemoryScope,
    pub scope_id: &'a str,
    pub kind: &'a str,
    pub statement: &'a str,
    pub created_at: &'a str,
    pub expires_at: &'a str,
}

#[path = "core_journal_children.rs"]
mod children;
#[path = "core_journal_conversation.rs"]
mod conversation;
#[path = "core_journal_core.rs"]
mod core;
#[path = "core_journal_ledger.rs"]
mod ledger;
#[path = "core_journal_memory.rs"]
mod memory;
#[path = "core_journal_provenance.rs"]
mod provenance;
#[path = "core_journal_tasks.rs"]
mod tasks;
