use super::*;

mod database_pool;
use database_pool::PreparedDatabasePool;
pub(crate) use database_pool::PreparedDatabaseLease;

pub(crate) const EVENT_JOURNAL_DATABASE_POOL_SIZE: usize = 4;

/// Durable SQLite-backed journal shared by the Core subsystems.
#[derive(Clone)]
pub struct EventJournal {
    pub(crate) database: Arc<Mutex<LocalDatabase>>,
    pub(crate) database_path: Arc<std::path::PathBuf>,
    pub(crate) database_pool: Arc<PreparedDatabasePool>,
    pub(crate) writer: Arc<std::sync::mpsc::SyncSender<JournalWrite>>,
    #[cfg(test)]
    pub(crate) test_fail_after_primary: Arc<std::sync::atomic::AtomicBool>,
}

pub(crate) type JournalWriteFn =
    Box<dyn FnOnce(&mut LocalDatabase) -> Result<i64, StorageError> + Send + 'static>;
pub(crate) struct JournalWrite(
    pub JournalWriteFn,
    pub std::sync::mpsc::Sender<Result<i64, StorageError>>,
);

/// Bounded replay page with sequence-gap metadata for an event consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableReplayBatch {
    /// Events available in this replay page.
    pub events: Vec<EventRecord>,
    /// Whether the requested cursor predates the retained event history.
    pub gap_detected: bool,
    /// Earliest retained sequence, when the journal contains any events.
    pub first_available_sequence: Option<i64>,
    /// Latest sequence included in the journal snapshot.
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
    /// Task that owns the tool call.
    pub task_id: &'a str,
    /// Stable tool identifier.
    pub tool_name: &'a str,
    /// Agent iteration in which the tool was invoked.
    pub iteration: usize,
    /// Whether the tool call completed successfully.
    pub ok: bool,
    /// Normalized failure category, when the call failed.
    pub failure_kind: Option<&'a str>,
    /// Whether recovery guidance was produced.
    pub recovery_hint: bool,
    /// Whether repeated failure escalation was activated.
    pub escalated: bool,
}

/// Параметры перехода durable recovery state machine.
#[derive(Debug, Clone, Copy)]
pub struct RecoveryTransition<'a> {
    /// Run whose recovery state is transitioning.
    pub run_id: &'a str,
    /// New durable recovery state.
    pub state: RecoveryState,
    /// Effect whose outcome is being reconciled.
    pub effect_id: &'a str,
    /// Idempotency key associated with the effect.
    pub idempotency_key: &'a str,
    /// Verifier used to establish the effect outcome.
    pub verifier: &'a str,
    /// Serialized evidence used by the recovery decision.
    pub evidence_json: &'a [u8],
    /// Decision recorded by the recovery transition.
    pub decision: &'a str,
}

/// Данные session-only заметки, которые никогда не становятся persistent memory.
#[derive(Debug, Clone, Copy)]
pub struct SessionMemoryNote<'a> {
    /// Stable identifier for the note.
    pub id: &'a str,
    /// Session that owns this ephemeral note.
    pub session_id: &'a str,
    /// Scope in which the note is visible.
    pub scope: evohime_local_storage::domains::memory::MemoryScope,
    /// Identifier of the scope owner.
    pub scope_id: &'a str,
    /// Category assigned to the note.
    pub kind: &'a str,
    /// Note contents, which are not promoted to persistent memory.
    pub statement: &'a str,
    /// Creation timestamp.
    pub created_at: &'a str,
    /// Expiration timestamp after which the note is purged.
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
