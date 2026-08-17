use std::{
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub mod artifact_store;
pub mod backup;
pub mod capability_selection_store;
pub mod capability_store;
pub mod child_store;
pub mod context_command_store;
pub mod context_ledger_store;
pub mod feedback_store;
pub mod memory_store;
pub mod model_limit_store;
pub mod reconciliation_verifier;
pub mod research_store;
pub mod scratchpad_store;

pub use backup::{
    BackupObjectSummary, BackupPreview, BackupProgress, BackupProgressPhase, BackupResult,
    RestoreResult, BACKUP_FORMAT_VERSION,
};

pub const SCHEMA_VERSION: u32 = 22;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON operation failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported database schema version {0}")]
    UnsupportedSchema(u32),
    #[error(
        "optimistic version conflict for {entity} {id}: expected {expected}, current {current}"
    )]
    VersionConflict {
        entity: &'static str,
        id: String,
        expected: i64,
        current: i64,
    },
    #[error("request {client_id}/{request_id} was already used with another command")]
    DeduplicationConflict {
        client_id: String,
        request_id: String,
    },
    #[error("adding dependency {from_id} -> {to_id} would create a cycle")]
    DependencyCycle { from_id: String, to_id: String },
    #[error("invalid run effect transition: {0}")]
    InvalidRunEffect(String),
    #[error("invalid recovery transition: {0}")]
    InvalidRecovery(String),
    #[error("backup operation failed: {0}")]
    Backup(String),
    #[error("backup format is invalid: {0}")]
    BackupFormat(String),
    #[error("backup checksum mismatch: expected {expected}, got {actual}")]
    BackupChecksumMismatch { expected: String, actual: String },
    #[error("backup schema mismatch: expected {expected}, got {actual}")]
    BackupSchemaMismatch { expected: u32, actual: u32 },
    #[error("backup is too large: {0} bytes")]
    BackupTooLarge(u64),
    #[error("backup destination already exists: {0}")]
    BackupDestinationExists(String),
    #[error("backup operation was cancelled")]
    BackupCancelled,
    /// Нарушение контракта плана 01: scratchpad или artifact store.
    #[error("context operation failed: {0}")]
    Context(String),
}

pub struct LocalDatabase {
    path: PathBuf,
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub sequence_id: i64,
    pub task_id: String,
    pub event_type: String,
    pub payload: Vec<u8>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolMetricRecord {
    pub id: i64,
    pub task_id: String,
    pub tool_name: String,
    pub iteration: i64,
    pub ok: bool,
    pub failure_kind: Option<String>,
    pub recovery_hint: bool,
    pub escalated: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub title: String,
    pub workspace_path: String,
    pub source_ref: Option<String>,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPolicyRecord {
    pub project_id: String,
    pub policy_json: Vec<u8>,
    pub version: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemRecord {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: String,
    pub source_ref: Option<String>,
    pub acceptance_criteria: String,
    pub non_goals: String,
    pub status: String,
    pub priority: i64,
    pub estimate: Option<i64>,
    pub complexity: Option<String>,
    pub attempt_count: i64,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub source_ref: String,
    pub acceptance_criteria: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceRecord {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRecord {
    pub id: String,
    pub run_id: String,
    pub workspace_hash: String,
    pub payload: Vec<u8>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleRef {
    pub id: String,
    pub version: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRef {
    pub id: String,
    pub version: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySnapshot {
    pub schema_version: u32,
    pub policy_version: u32,
    pub effective_permissions_hash: String,
    pub canonical_json: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRouteSnapshot {
    pub requested_route: String,
    pub resolved_provider: String,
    pub resolved_model: String,
    pub route_policy_version: u32,
    pub canonical_json: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSnapshots {
    pub role_ref: RoleRef,
    pub skill_ref: SkillRef,
    pub policy: PolicySnapshot,
    pub model_route: ModelRouteSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub id: String,
    pub work_item_id: String,
    pub status: String,
    pub policy_snapshot: Vec<u8>,
    pub role_snapshot: Vec<u8>,
    pub skill_snapshot: Vec<u8>,
    pub model_route_snapshot: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCheckpointRecord {
    pub run_id: String,
    pub checkpoint_id: String,
    pub stage: String,
    pub node_id: String,
    pub attempt: u32,
    pub input_hash: String,
    pub state_json: Vec<u8>,
    pub pending_effects_json: Vec<u8>,
    pub committed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEffectRecord {
    pub effect_id: String,
    pub run_id: String,
    pub node_id: String,
    pub kind: String,
    pub idempotency_key: String,
    pub immutable_intent_hash: String,
    pub state: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub result_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredRunRecord {
    pub run_id: String,
    pub work_item_id: String,
    pub effect_id: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLeaseRecord {
    pub run_id: String,
    pub lease_id: String,
    pub owner_id: String,
    pub generation: u64,
    pub lease_expires_at: String,
    pub heartbeat_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReconciliationRecord {
    pub effect_id: String,
    pub state: String,
    pub verifier: String,
    pub evidence_json: Vec<u8>,
    pub reconciled_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryState {
    Recovering,
    Reconciling,
    Resumable,
    Blocked,
    WaitingApproval,
    Failed,
}

impl RecoveryState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Recovering => "RECOVERING",
            Self::Reconciling => "RECONCILING",
            Self::Resumable => "RESUMABLE",
            Self::Blocked => "BLOCKED",
            Self::WaitingApproval => "WAITING_APPROVAL",
            Self::Failed => "FAILED",
        }
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Resumable | Self::Blocked | Self::WaitingApproval | Self::Failed
        )
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "RECOVERING" => Ok(Self::Recovering),
            "RECONCILING" => Ok(Self::Reconciling),
            "RESUMABLE" => Ok(Self::Resumable),
            "BLOCKED" => Ok(Self::Blocked),
            "WAITING_APPROVAL" => Ok(Self::WaitingApproval),
            "FAILED" => Ok(Self::Failed),
            other => Err(StorageError::InvalidRecovery(format!(
                "unknown recovery state {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecoveryRecord {
    pub id: i64,
    pub run_id: String,
    pub state: RecoveryState,
    pub effect_id: String,
    pub idempotency_key: String,
    pub verifier: String,
    pub evidence_json: Vec<u8>,
    pub decision: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsTableCount {
    pub table: String,
    pub rows: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsEventCount {
    pub event_type: String,
    pub rows: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsSummary {
    pub table_counts: Vec<DiagnosticsTableCount>,
    pub event_counts: Vec<DiagnosticsEventCount>,
    pub total_events: i64,
    pub event_types_truncated: bool,
}

pub const MAX_DIAGNOSTICS_EVENT_TYPES: usize = 128;

/// Bounded, read-only recovery health facts. Distinct from
/// `recover_unknown_effects`, which mutates run/effect state; this snapshot
/// performs only SELECTs so it is safe for diagnostic use (e.g. Core Doctor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryHealthSnapshot {
    pub unknown_effects: i64,
    pub lease_expired: bool,
    pub resumable_runs: i64,
}

impl LocalDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::open_internal(path.as_ref(), false)
    }

    fn open_internal(path: &Path, fail_migration: bool) -> Result<Self, StorageError> {
        let path = path.to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let existed = path.exists();
        let connection = Connection::open(&path)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let version = Self::read_schema_version(&connection)?;
        if version > SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema(version));
        }
        if version < SCHEMA_VERSION {
            if existed {
                fs::copy(&path, path.with_extension("db.bak"))?;
            }
            if let Err(error) = Self::migrate(&connection, version, fail_migration) {
                drop(connection);
                fs::copy(path.with_extension("db.bak"), &path)?;
                return Err(error);
            }
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        evohime_receipts::runtime::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        Ok(Self { path, connection })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Exposes the shared migrated connection so bounded, migration-neutral
    /// contracts (e.g. `research_store`, `memory_store`) can persist against
    /// the real application database instead of a private test connection.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Exclusive access for short Core-owned transactions that publish a
    /// complete workspace index/vector generation atomically.
    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    pub fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(Self::read_schema_version(&self.connection)?)
    }

    pub fn has_events_table(&self) -> Result<bool, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'events' LIMIT 1",
                [],
                |row| row.get::<_, i32>(0),
            )
            .optional()?
            .is_some())
    }

    pub fn create_project(
        &self,
        id: &str,
        title: &str,
        workspace_path: &str,
        source_ref: Option<&str>,
    ) -> Result<ProjectRecord, StorageError> {
        self.connection.execute(
            "INSERT INTO projects(id, title, workspace_path, source_ref) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title,
             workspace_path = excluded.workspace_path, source_ref = excluded.source_ref",
            rusqlite::params![id, title, workspace_path, source_ref],
        )?;
        self.get_project(id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, title, workspace_path, source_ref, version FROM projects WHERE id = ?1",
        )?;
        Ok(statement
            .query_row([id], |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    workspace_path: row.get(2)?,
                    source_ref: row.get(3)?,
                    version: row.get(4)?,
                })
            })
            .optional()?)
    }

    pub fn get_project_policy(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectPolicyRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT project_id, policy_json, version, updated_at FROM project_policies WHERE project_id = ?1",
                [project_id],
                |row| {
                    Ok(ProjectPolicyRecord {
                        project_id: row.get(0)?,
                        policy_json: row.get(1)?,
                        version: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn upsert_project_policy(
        &self,
        project_id: &str,
        policy_json: &[u8],
        expected_version: Option<i64>,
    ) -> Result<ProjectPolicyRecord, StorageError> {
        let current = self.get_project_policy(project_id)?;
        match (current, expected_version) {
            (Some(record), Some(expected)) if record.version != expected => {
                return Err(StorageError::VersionConflict {
                    entity: "project_policy",
                    id: project_id.into(),
                    expected,
                    current: record.version,
                });
            }
            _ => {}
        }
        self.connection.execute(
            "INSERT INTO project_policies(project_id, policy_json, version) VALUES (?1, ?2, 1)
             ON CONFLICT(project_id) DO UPDATE SET policy_json = excluded.policy_json, version = project_policies.version + 1,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            rusqlite::params![project_id, policy_json],
        )?;
        self.get_project_policy(project_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn create_work_item(&self, item: &WorkItemRecord) -> Result<WorkItemRecord, StorageError> {
        self.connection.execute(
            "INSERT INTO work_items(id, project_id, parent_id, title, description, source_ref,
             acceptance_criteria, non_goals, status, priority, estimate, complexity, attempt_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                item.id,
                item.project_id,
                item.parent_id,
                item.title,
                item.description,
                item.source_ref,
                item.acceptance_criteria,
                item.non_goals,
                item.status,
                item.priority,
                item.estimate,
                item.complexity,
                item.attempt_count
            ],
        )?;
        self.get_work_item(&item.id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn import_prd(
        &self,
        provenance_id: &str,
        project_id: &str,
        origin: &str,
        version: &str,
        source_text: &str,
        tasks: &[ImportedTask],
    ) -> Result<Vec<WorkItemRecord>, StorageError> {
        let payload = serde_json::to_vec(&serde_json::json!({
            "version": version,
            "source_text": source_text,
        }))?;
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO provenance(id, kind, source, payload) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![provenance_id, "prd_import", origin, payload],
        )?;
        for task in tasks {
            transaction.execute(
                "INSERT INTO work_items(id, project_id, title, description, source_ref,
                 acceptance_criteria, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'backlog')",
                rusqlite::params![
                    task.id,
                    project_id,
                    task.title,
                    task.description,
                    task.source_ref,
                    task.acceptance_criteria,
                ],
            )?;
        }
        transaction.commit()?;
        tasks
            .iter()
            .map(|task| {
                self.get_work_item(&task.id)?
                    .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
            })
            .collect()
    }

    pub fn get_provenance(&self, id: &str) -> Result<Option<ProvenanceRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, kind, source, payload FROM provenance WHERE id = ?1",
                [id],
                |row| {
                    Ok(ProvenanceRecord {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        source: row.get(2)?,
                        payload: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_snapshot(
        &self,
        id: &str,
        run_id: &str,
        workspace_hash: &str,
        payload: &[u8],
    ) -> Result<SnapshotRecord, StorageError> {
        self.connection.execute(
            "INSERT INTO snapshots(id, run_id, workspace_hash, payload) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, run_id, workspace_hash, payload],
        )?;
        self.get_snapshot(id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get_snapshot(&self, id: &str) -> Result<Option<SnapshotRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, run_id, workspace_hash, payload, created_at FROM snapshots WHERE id = ?1",
                [id],
                |row| {
                    Ok(SnapshotRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        workspace_hash: row.get(2)?,
                        payload: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn latest_snapshot_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<SnapshotRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT s.id, s.run_id, s.workspace_hash, s.payload, s.created_at
             FROM snapshots s JOIN runs r ON r.id = s.run_id
             WHERE r.work_item_id = ?1 ORDER BY s.created_at DESC, s.id DESC LIMIT 1",
        )?;
        Ok(statement
            .query_row([task_id], |row| {
                Ok(SnapshotRecord {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    workspace_hash: row.get(2)?,
                    payload: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .optional()?)
    }

    pub fn get_work_item(&self, id: &str) -> Result<Option<WorkItemRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, project_id, parent_id, title, description, source_ref,
             acceptance_criteria, non_goals, status, priority, estimate, complexity,
             attempt_count, version FROM work_items WHERE id = ?1",
        )?;
        Ok(statement
            .query_row([id], |row| {
                Ok(WorkItemRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    source_ref: row.get(5)?,
                    acceptance_criteria: row.get(6)?,
                    non_goals: row.get(7)?,
                    status: row.get(8)?,
                    priority: row.get(9)?,
                    estimate: row.get(10)?,
                    complexity: row.get(11)?,
                    attempt_count: row.get(12)?,
                    version: row.get(13)?,
                })
            })
            .optional()?)
    }

    pub fn list_work_items(&self, project_id: &str) -> Result<Vec<WorkItemRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, project_id, parent_id, title, description, source_ref,
             acceptance_criteria, non_goals, status, priority, estimate, complexity,
             attempt_count, version FROM work_items
             WHERE project_id = ?1 ORDER BY priority DESC, id ASC",
        )?;
        let rows = statement.query_map([project_id], |row| {
            Ok(WorkItemRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                parent_id: row.get(2)?,
                title: row.get(3)?,
                description: row.get(4)?,
                source_ref: row.get(5)?,
                acceptance_criteria: row.get(6)?,
                non_goals: row.get(7)?,
                status: row.get(8)?,
                priority: row.get(9)?,
                estimate: row.get(10)?,
                complexity: row.get(11)?,
                attempt_count: row.get(12)?,
                version: row.get(13)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_dependencies(
        &self,
        project_id: &str,
    ) -> Result<Vec<(String, String, String)>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT e.from_work_item_id, e.to_work_item_id, e.kind
             FROM work_item_edges e
             JOIN work_items f ON f.id = e.from_work_item_id
             WHERE f.project_id = ?1
             ORDER BY e.from_work_item_id ASC, e.to_work_item_id ASC, e.kind ASC",
        )?;
        let rows = statement.query_map([project_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn next_ready(&self, project_id: &str) -> Result<Option<WorkItemRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT w.id, w.project_id, w.parent_id, w.title, w.description, w.source_ref,
             w.acceptance_criteria, w.non_goals, w.status, w.priority, w.estimate, w.complexity,
             w.attempt_count, w.version
             FROM work_items w
             WHERE w.project_id = ?1 AND w.status IN ('backlog', 'ready')
             AND NOT EXISTS (
                 SELECT 1 FROM work_item_edges e
                 JOIN work_items dependency ON dependency.id = e.to_work_item_id
                 WHERE e.from_work_item_id = w.id AND dependency.status <> 'done'
             )
             ORDER BY CASE WHEN w.status = 'ready' THEN 0 ELSE 1 END,
                      w.priority DESC, w.id ASC LIMIT 1",
        )?;
        Ok(statement
            .query_row([project_id], |row| {
                Ok(WorkItemRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    source_ref: row.get(5)?,
                    acceptance_criteria: row.get(6)?,
                    non_goals: row.get(7)?,
                    status: row.get(8)?,
                    priority: row.get(9)?,
                    estimate: row.get(10)?,
                    complexity: row.get(11)?,
                    attempt_count: row.get(12)?,
                    version: row.get(13)?,
                })
            })
            .optional()?)
    }

    pub fn update_work_item_status(
        &self,
        id: &str,
        expected_version: i64,
        status: &str,
    ) -> Result<WorkItemRecord, StorageError> {
        let changed = self.connection.execute(
            "UPDATE work_items SET status = ?1, version = version + 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?2 AND version = ?3",
            rusqlite::params![status, id, expected_version],
        )?;
        if changed == 0 {
            let current = self
                .get_work_item(id)?
                .map(|item| item.version)
                .unwrap_or(-1);
            return Err(StorageError::VersionConflict {
                entity: "work_item",
                id: id.into(),
                expected: expected_version,
                current,
            });
        }
        self.get_work_item(id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn add_dependency(
        &self,
        from_id: &str,
        to_id: &str,
        kind: &str,
    ) -> Result<(), StorageError> {
        if from_id == to_id {
            return Err(StorageError::DependencyCycle {
                from_id: from_id.into(),
                to_id: to_id.into(),
            });
        }
        let mut pending = vec![to_id.to_owned()];
        let mut visited = std::collections::HashSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if current == from_id {
                return Err(StorageError::DependencyCycle {
                    from_id: from_id.into(),
                    to_id: to_id.into(),
                });
            }
            let mut statement = self.connection.prepare(
                "SELECT to_work_item_id FROM work_item_edges WHERE from_work_item_id = ?1",
            )?;
            let rows = statement.query_map([current], |row| row.get::<_, String>(0))?;
            pending.extend(rows.collect::<Result<Vec<_>, _>>()?);
        }
        self.connection.execute(
            "INSERT INTO work_item_edges(from_work_item_id, to_work_item_id, kind) VALUES (?1, ?2, ?3)",
            rusqlite::params![from_id, to_id, kind],
        )?;
        Ok(())
    }

    pub fn record_deduplicated(
        &self,
        client_id: &str,
        request_id: &str,
        command_hash: &str,
        result: &[u8],
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let existing = self.connection.query_row(
            "SELECT command_hash, result FROM command_dedup WHERE client_id = ?1 AND request_id = ?2",
            rusqlite::params![client_id, request_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        ).optional()?;
        if let Some((stored_hash, stored_result)) = existing {
            if stored_hash == command_hash {
                return Ok(Some(stored_result));
            }
            return Err(StorageError::DeduplicationConflict {
                client_id: client_id.into(),
                request_id: request_id.into(),
            });
        }
        if result.is_empty() {
            return Ok(None);
        }
        self.connection.execute(
            "INSERT INTO command_dedup(client_id, request_id, command_hash, result) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![client_id, request_id, command_hash, result],
        )?;
        Ok(None)
    }

    pub fn create_run(&self, run: &RunRecord) -> Result<RunRecord, StorageError> {
        self.connection.execute(
            "INSERT INTO runs(id, work_item_id, status, policy_snapshot, role_snapshot,
             skill_snapshot, model_route_snapshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                run.id,
                run.work_item_id,
                run.status,
                run.policy_snapshot,
                run.role_snapshot,
                run.skill_snapshot,
                run.model_route_snapshot
            ],
        )?;
        self.get_run(&run.id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get_run(&self, id: &str) -> Result<Option<RunRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, work_item_id, status, policy_snapshot, role_snapshot,
             skill_snapshot, model_route_snapshot FROM runs WHERE id = ?1",
        )?;
        Ok(statement
            .query_row([id], |row| {
                Ok(RunRecord {
                    id: row.get(0)?,
                    work_item_id: row.get(1)?,
                    status: row.get(2)?,
                    policy_snapshot: row.get(3)?,
                    role_snapshot: row.get(4)?,
                    skill_snapshot: row.get(5)?,
                    model_route_snapshot: row.get(6)?,
                })
            })
            .optional()?)
    }

    pub fn create_run_with_snapshots(
        &self,
        id: &str,
        work_item_id: &str,
        status: &str,
        snapshots: &RunSnapshots,
    ) -> Result<RunRecord, StorageError> {
        let run = RunRecord {
            id: id.into(),
            work_item_id: work_item_id.into(),
            status: status.into(),
            policy_snapshot: serde_json::to_vec(&snapshots.policy)?,
            role_snapshot: serde_json::to_vec(&snapshots.role_ref)?,
            skill_snapshot: serde_json::to_vec(&snapshots.skill_ref)?,
            model_route_snapshot: serde_json::to_vec(&snapshots.model_route)?,
        };
        self.create_run(&run)
    }

    pub fn get_run_snapshots(&self, id: &str) -> Result<Option<RunSnapshots>, StorageError> {
        let Some(run) = self.get_run(id)? else {
            return Ok(None);
        };
        Ok(Some(RunSnapshots {
            role_ref: serde_json::from_slice(&run.role_snapshot)?,
            skill_ref: serde_json::from_slice(&run.skill_snapshot)?,
            policy: serde_json::from_slice(&run.policy_snapshot)?,
            model_route: serde_json::from_slice(&run.model_route_snapshot)?,
        }))
    }

    pub fn create_run_if_absent(&self, run: &RunRecord) -> Result<RunRecord, StorageError> {
        self.connection.execute(
            "INSERT OR IGNORE INTO runs(id, work_item_id, status, policy_snapshot, role_snapshot,
             skill_snapshot, model_route_snapshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                run.id,
                run.work_item_id,
                run.status,
                run.policy_snapshot,
                run.role_snapshot,
                run.skill_snapshot,
                run.model_route_snapshot
            ],
        )?;
        self.get_run(&run.id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn create_checkpoint(
        &self,
        checkpoint: &RunCheckpointRecord,
    ) -> Result<RunCheckpointRecord, StorageError> {
        self.connection.execute(
            "INSERT INTO run_checkpoints(run_id, checkpoint_id, stage, node_id, attempt, input_hash,
             state_json, pending_effects_json, committed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                checkpoint.run_id,
                checkpoint.checkpoint_id,
                checkpoint.stage,
                checkpoint.node_id,
                checkpoint.attempt,
                checkpoint.input_hash,
                checkpoint.state_json,
                checkpoint.pending_effects_json,
                checkpoint.committed_at
            ],
        )?;
        Ok(checkpoint.clone())
    }

    pub fn latest_checkpoint(
        &self,
        run_id: &str,
    ) -> Result<Option<RunCheckpointRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT run_id, checkpoint_id, stage, node_id, attempt, input_hash, state_json,
             pending_effects_json, committed_at FROM run_checkpoints
             WHERE run_id = ?1 ORDER BY rowid DESC LIMIT 1",
        )?;
        Ok(statement
            .query_row([run_id], |row| {
                Ok(RunCheckpointRecord {
                    run_id: row.get(0)?,
                    checkpoint_id: row.get(1)?,
                    stage: row.get(2)?,
                    node_id: row.get(3)?,
                    attempt: row.get(4)?,
                    input_hash: row.get(5)?,
                    state_json: row.get(6)?,
                    pending_effects_json: row.get(7)?,
                    committed_at: row.get(8)?,
                })
            })
            .optional()?)
    }

    pub fn prepare_run_effect(
        &self,
        run: &RunRecord,
        checkpoint: &RunCheckpointRecord,
        effect: &RunEffectRecord,
    ) -> Result<RunEffectRecord, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO runs(id, work_item_id, status, policy_snapshot, role_snapshot,
             skill_snapshot, model_route_snapshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                run.id,
                run.work_item_id,
                run.status,
                run.policy_snapshot,
                run.role_snapshot,
                run.skill_snapshot,
                run.model_route_snapshot
            ],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO run_checkpoints(run_id, checkpoint_id, stage, node_id, attempt,
             input_hash, state_json, pending_effects_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                checkpoint.run_id,
                checkpoint.checkpoint_id,
                checkpoint.stage,
                checkpoint.node_id,
                checkpoint.attempt,
                checkpoint.input_hash,
                checkpoint.state_json,
                checkpoint.pending_effects_json
            ],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO run_effects(effect_id, run_id, node_id, kind, idempotency_key,
             immutable_intent_hash, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                effect.effect_id,
                effect.run_id,
                effect.node_id,
                effect.kind,
                effect.idempotency_key,
                effect.immutable_intent_hash,
                effect.state
            ],
        )?;
        transaction.commit()?;
        self.get_run_effect(&effect.effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get_run_effect(&self, effect_id: &str) -> Result<Option<RunEffectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT effect_id, run_id, node_id, kind, idempotency_key, immutable_intent_hash,
             state, started_at, completed_at, result_hash FROM run_effects WHERE effect_id = ?1",
        )?;
        Ok(statement
            .query_row([effect_id], |row| {
                Ok(RunEffectRecord {
                    effect_id: row.get(0)?,
                    run_id: row.get(1)?,
                    node_id: row.get(2)?,
                    kind: row.get(3)?,
                    idempotency_key: row.get(4)?,
                    immutable_intent_hash: row.get(5)?,
                    state: row.get(6)?,
                    started_at: row.get(7)?,
                    completed_at: row.get(8)?,
                    result_hash: row.get(9)?,
                })
            })
            .optional()?)
    }

    pub fn mark_effect_executing(&self, effect_id: &str) -> Result<RunEffectRecord, StorageError> {
        self.connection.execute(
            "UPDATE run_effects SET state = 'executing', started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE effect_id = ?1 AND state = 'prepared'",
            [effect_id],
        )?;
        self.get_run_effect(effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn complete_run_effect(
        &self,
        effect_id: &str,
        success: bool,
        result_hash: Option<&str>,
    ) -> Result<RunEffectRecord, StorageError> {
        let state = if success {
            "completed_success"
        } else {
            "completed_failure"
        };
        self.connection.execute(
            "UPDATE run_effects SET state = ?1, completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), result_hash = ?2
             WHERE effect_id = ?3 AND state = 'executing'",
            rusqlite::params![state, result_hash, effect_id],
        )?;
        self.get_run_effect(effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn acquire_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
        ttl_seconds: u64,
    ) -> Result<RunLeaseRecord, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO run_leases(run_id, lease_id, owner_id, generation, lease_expires_at, heartbeat_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now', '+' || ?5 || ' seconds'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            rusqlite::params![run_id, lease_id, owner_id, generation, ttl_seconds as i64],
        )?;
        let updated = transaction.execute(
            "UPDATE run_leases SET lease_id = ?1, owner_id = ?2, generation = ?3,
             lease_expires_at = datetime('now', '+' || ?4 || ' seconds'),
             heartbeat_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE run_id = ?5 AND (lease_id = ?1 OR lease_expires_at <= datetime('now'))",
            rusqlite::params![lease_id, owner_id, generation, ttl_seconds as i64, run_id],
        )?;
        if updated == 0 {
            return Err(StorageError::InvalidRunEffect(
                "run lease is held by another owner".into(),
            ));
        }
        transaction.commit()?;
        self.get_run_lease(run_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get_run_lease(&self, run_id: &str) -> Result<Option<RunLeaseRecord>, StorageError> {
        Ok(self.connection.query_row(
            "SELECT run_id, lease_id, owner_id, generation, lease_expires_at, heartbeat_at FROM run_leases WHERE run_id = ?1",
            [run_id],
            |row| Ok(RunLeaseRecord { run_id: row.get(0)?, lease_id: row.get(1)?, owner_id: row.get(2)?, generation: row.get(3)?, lease_expires_at: row.get(4)?, heartbeat_at: row.get(5)? }),
        ).optional()?)
    }

    pub fn heartbeat_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
        ttl_seconds: u64,
    ) -> Result<RunLeaseRecord, StorageError> {
        let changed = self.connection.execute(
            "UPDATE run_leases SET lease_expires_at = datetime('now', '+' || ?1 || ' seconds'), heartbeat_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE run_id = ?2 AND lease_id = ?3 AND owner_id = ?4 AND generation = ?5 AND lease_expires_at > datetime('now')",
            rusqlite::params![ttl_seconds as i64, run_id, lease_id, owner_id, generation],
        )?;
        if changed == 0 {
            return Err(StorageError::InvalidRunEffect(
                "run lease heartbeat rejected".into(),
            ));
        }
        self.get_run_lease(run_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn release_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM run_leases WHERE run_id = ?1 AND lease_id = ?2 AND owner_id = ?3 AND generation = ?4",
            rusqlite::params![run_id, lease_id, owner_id, generation],
        )?;
        Ok(())
    }

    pub fn prepare_agent_run_effect(
        &self,
        effect: &RunEffectRecord,
        task_id: &str,
    ) -> Result<RunEffectRecord, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO agent_run_effects(
                effect_id, run_id, task_id, node_id, kind, idempotency_key,
                immutable_intent_hash, state
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                effect.effect_id,
                effect.run_id,
                task_id,
                effect.node_id,
                effect.kind,
                effect.idempotency_key,
                effect.immutable_intent_hash,
                effect.state,
            ],
        )?;
        transaction.commit()?;
        self.get_agent_run_effect(&effect.effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get_agent_run_effect(
        &self,
        effect_id: &str,
    ) -> Result<Option<RunEffectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT effect_id, run_id, node_id, kind, idempotency_key,
             immutable_intent_hash, state, started_at, completed_at, result_hash
             FROM agent_run_effects WHERE effect_id = ?1",
        )?;
        Ok(statement
            .query_row([effect_id], |row| {
                Ok(RunEffectRecord {
                    effect_id: row.get(0)?,
                    run_id: row.get(1)?,
                    node_id: row.get(2)?,
                    kind: row.get(3)?,
                    idempotency_key: row.get(4)?,
                    immutable_intent_hash: row.get(5)?,
                    state: row.get(6)?,
                    started_at: row.get(7)?,
                    completed_at: row.get(8)?,
                    result_hash: row.get(9)?,
                })
            })
            .optional()?)
    }

    pub fn mark_agent_effect_executing(
        &self,
        effect_id: &str,
    ) -> Result<RunEffectRecord, StorageError> {
        self.connection.execute(
            "UPDATE agent_run_effects SET state = 'executing',
             started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE effect_id = ?1 AND state = 'prepared'",
            [effect_id],
        )?;
        self.get_agent_run_effect(effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn complete_agent_run_effect(
        &self,
        effect_id: &str,
        success: bool,
        result_hash: Option<&str>,
    ) -> Result<RunEffectRecord, StorageError> {
        let state = if success {
            "completed_success"
        } else {
            "completed_failure"
        };
        self.connection.execute(
            "UPDATE agent_run_effects SET state = ?1,
             completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), result_hash = ?2
             WHERE effect_id = ?3 AND state = 'executing'",
            rusqlite::params![state, result_hash, effect_id],
        )?;
        self.get_agent_run_effect(effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn acquire_agent_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
        ttl_seconds: u64,
    ) -> Result<RunLeaseRecord, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO agent_run_leases(
                run_id, lease_id, owner_id, generation, lease_expires_at, heartbeat_at
             ) VALUES (?1, ?2, ?3, ?4, datetime('now', '+' || ?5 || ' seconds'),
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            rusqlite::params![run_id, lease_id, owner_id, generation, ttl_seconds as i64],
        )?;
        let updated = transaction.execute(
            "UPDATE agent_run_leases SET lease_id = ?1, owner_id = ?2, generation = ?3,
             lease_expires_at = datetime('now', '+' || ?4 || ' seconds'),
             heartbeat_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE run_id = ?5 AND (lease_id = ?1 OR lease_expires_at <= datetime('now'))",
            rusqlite::params![lease_id, owner_id, generation, ttl_seconds as i64, run_id],
        )?;
        if updated == 0 {
            return Err(StorageError::InvalidRunEffect(
                "agent run lease is held by another owner".into(),
            ));
        }
        transaction.commit()?;
        self.get_agent_run_lease(run_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get_agent_run_lease(
        &self,
        run_id: &str,
    ) -> Result<Option<RunLeaseRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT run_id, lease_id, owner_id, generation, lease_expires_at, heartbeat_at
                 FROM agent_run_leases WHERE run_id = ?1",
                [run_id],
                |row| {
                    Ok(RunLeaseRecord {
                        run_id: row.get(0)?,
                        lease_id: row.get(1)?,
                        owner_id: row.get(2)?,
                        generation: row.get(3)?,
                        lease_expires_at: row.get(4)?,
                        heartbeat_at: row.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn heartbeat_agent_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
        ttl_seconds: u64,
    ) -> Result<RunLeaseRecord, StorageError> {
        let changed = self.connection.execute(
            "UPDATE agent_run_leases SET lease_expires_at = datetime('now', '+' || ?1 || ' seconds'),
             heartbeat_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE run_id = ?2 AND lease_id = ?3 AND owner_id = ?4 AND generation = ?5
             AND lease_expires_at > datetime('now')",
            rusqlite::params![ttl_seconds as i64, run_id, lease_id, owner_id, generation],
        )?;
        if changed == 0 {
            return Err(StorageError::InvalidRunEffect(
                "agent run lease heartbeat rejected".into(),
            ));
        }
        self.get_agent_run_lease(run_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn release_agent_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM agent_run_leases
             WHERE run_id = ?1 AND lease_id = ?2 AND owner_id = ?3 AND generation = ?4",
            rusqlite::params![run_id, lease_id, owner_id, generation],
        )?;
        Ok(())
    }

    pub fn reconcile_agent_run_effect(
        &self,
        effect_id: &str,
        success: bool,
        verifier: &str,
        evidence_json: &[u8],
    ) -> Result<RunReconciliationRecord, StorageError> {
        let state = if success {
            "reconciled_success"
        } else {
            "reconciled_blocked"
        };
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR REPLACE INTO agent_run_reconciliations(
                effect_id, state, verifier, evidence_json
             ) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![effect_id, state, verifier, evidence_json],
        )?;
        if success {
            transaction.execute(
                "UPDATE agent_run_effects SET state = 'completed_success',
                 result_hash = ?1 WHERE effect_id = ?2 AND state = 'unknown'",
                rusqlite::params![verifier, effect_id],
            )?;
        }
        transaction.commit()?;
        self.connection
            .query_row(
                "SELECT effect_id, state, verifier, evidence_json, reconciled_at
                 FROM agent_run_reconciliations WHERE effect_id = ?1",
                [effect_id],
                |row| {
                    Ok(RunReconciliationRecord {
                        effect_id: row.get(0)?,
                        state: row.get(1)?,
                        verifier: row.get(2)?,
                        evidence_json: row.get(3)?,
                        reconciled_at: row.get(4)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn reconcile_run_effect(
        &self,
        effect_id: &str,
        success: bool,
        verifier: &str,
        evidence_json: &[u8],
    ) -> Result<RunReconciliationRecord, StorageError> {
        let state = if success {
            "reconciled_success"
        } else {
            "reconciled_blocked"
        };
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR REPLACE INTO run_reconciliations(effect_id, state, verifier, evidence_json) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![effect_id, state, verifier, evidence_json],
        )?;
        if success {
            transaction.execute("UPDATE run_effects SET state = 'completed_success', result_hash = ?1 WHERE effect_id = ?2 AND state = 'unknown'", rusqlite::params![verifier, effect_id])?;
        }
        transaction.commit()?;
        self.connection.query_row(
            "SELECT effect_id, state, verifier, evidence_json, reconciled_at FROM run_reconciliations WHERE effect_id = ?1",
            [effect_id],
            |row| Ok(RunReconciliationRecord { effect_id: row.get(0)?, state: row.get(1)?, verifier: row.get(2)?, evidence_json: row.get(3)?, reconciled_at: row.get(4)? }),
        ).map_err(Into::into)
    }

    pub fn transition_recovery(
        &self,
        run_id: &str,
        next: RecoveryState,
        effect_id: &str,
        idempotency_key: &str,
        verifier: &str,
        evidence_json: &[u8],
        decision: &str,
    ) -> Result<RunRecoveryRecord, StorageError> {
        const MAX_TEXT: usize = 256;
        const MAX_EVIDENCE_BYTES: usize = 64 * 1024;
        for (field, value) in [
            ("run_id", run_id),
            ("effect_id", effect_id),
            ("idempotency_key", idempotency_key),
            ("verifier", verifier),
            ("decision", decision),
        ] {
            if value.trim().is_empty() || value.chars().count() > MAX_TEXT {
                return Err(StorageError::InvalidRecovery(format!(
                    "{field} is empty or exceeds {MAX_TEXT} characters"
                )));
            }
        }
        if evidence_json.len() > MAX_EVIDENCE_BYTES {
            return Err(StorageError::InvalidRecovery(format!(
                "evidence exceeds {MAX_EVIDENCE_BYTES} bytes"
            )));
        }

        let transaction = self.connection.unchecked_transaction()?;
        let current = transaction
            .query_row(
                "SELECT id, run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision, created_at
                 FROM run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
                [run_id],
                |row| {
                    Ok(RunRecoveryRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        state: RecoveryState::parse(&row.get::<_, String>(2)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
                        effect_id: row.get(3)?,
                        idempotency_key: row.get(4)?,
                        verifier: row.get(5)?,
                        evidence_json: row.get(6)?,
                        decision: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()?;
        if let Some(record) = &current {
            if record.idempotency_key == idempotency_key {
                if record.state == next {
                    transaction.commit()?;
                    return Ok(record.clone());
                }
                return Err(StorageError::InvalidRecovery(format!(
                    "idempotency key {} was already used for {:?}",
                    idempotency_key, record.state
                )));
            }
        }
        let current_state = current.as_ref().map(|record| record.state);
        let valid = match (current_state, next) {
            (None, RecoveryState::Recovering) => true,
            (Some(RecoveryState::Recovering), RecoveryState::Reconciling) => true,
            (Some(RecoveryState::Reconciling), state) if state.is_terminal() => true,
            (Some(state), next) if state == next && state.is_terminal() => true,
            _ => false,
        };
        if !valid {
            return Err(StorageError::InvalidRecovery(format!(
                "cannot transition from {:?} to {:?}",
                current_state, next
            )));
        }

        transaction.execute(
            "INSERT INTO run_recovery(run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                run_id,
                next.as_str(),
                effect_id,
                idempotency_key,
                verifier,
                evidence_json,
                decision,
            ],
        )?;
        let work_item_id: String = transaction.query_row(
            "SELECT work_item_id FROM runs WHERE id = ?1",
            [run_id],
            |row| row.get(0),
        )?;
        let payload = serde_json::to_vec(&serde_json::json!({
            "run_id": run_id,
            "effect_id": effect_id,
            "idempotency_key": idempotency_key,
            "verifier": verifier,
            "evidence": serde_json::from_slice::<serde_json::Value>(evidence_json)
                .unwrap_or_else(|_| serde_json::json!({"raw_bytes": evidence_json})),
            "decision": decision,
            "state": next.as_str(),
        }))?;
        transaction.execute(
            "INSERT INTO events(task_id, event_type, payload) VALUES (?1, 'run.recovery.decision', ?2)",
            rusqlite::params![work_item_id, payload],
        )?;
        let record = transaction.query_row(
            "SELECT id, run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision, created_at
             FROM run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
            [run_id],
            |row| {
                Ok(RunRecoveryRecord {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    state: RecoveryState::parse(&row.get::<_, String>(2)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    effect_id: row.get(3)?,
                    idempotency_key: row.get(4)?,
                    verifier: row.get(5)?,
                    evidence_json: row.get(6)?,
                    decision: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn transition_agent_recovery(
        &self,
        run_id: &str,
        next: RecoveryState,
        effect_id: &str,
        idempotency_key: &str,
        verifier: &str,
        evidence_json: &[u8],
        decision: &str,
    ) -> Result<RunRecoveryRecord, StorageError> {
        const MAX_TEXT: usize = 256;
        const MAX_EVIDENCE_BYTES: usize = 64 * 1024;
        for (field, value) in [
            ("run_id", run_id),
            ("effect_id", effect_id),
            ("idempotency_key", idempotency_key),
            ("verifier", verifier),
            ("decision", decision),
        ] {
            if value.trim().is_empty() || value.chars().count() > MAX_TEXT {
                return Err(StorageError::InvalidRecovery(format!(
                    "{field} is empty or exceeds {MAX_TEXT} characters"
                )));
            }
        }
        if evidence_json.len() > MAX_EVIDENCE_BYTES {
            return Err(StorageError::InvalidRecovery(format!(
                "evidence exceeds {MAX_EVIDENCE_BYTES} bytes"
            )));
        }

        let transaction = self.connection.unchecked_transaction()?;
        let current = transaction
            .query_row(
                "SELECT id, run_id, state, effect_id, idempotency_key, verifier,
                 evidence_json, decision, created_at
                 FROM agent_run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
                [run_id],
                |row| {
                    Ok(RunRecoveryRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        state: RecoveryState::parse(&row.get::<_, String>(2)?)
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        effect_id: row.get(3)?,
                        idempotency_key: row.get(4)?,
                        verifier: row.get(5)?,
                        evidence_json: row.get(6)?,
                        decision: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()?;
        if let Some(record) = &current {
            if record.idempotency_key == idempotency_key {
                if record.state == next {
                    transaction.commit()?;
                    return Ok(record.clone());
                }
                return Err(StorageError::InvalidRecovery(format!(
                    "idempotency key {} was already used for {:?}",
                    idempotency_key, record.state
                )));
            }
        }
        let current_state = current.as_ref().map(|record| record.state);
        let valid = match (current_state, next) {
            (None, RecoveryState::Recovering) => true,
            (Some(RecoveryState::Recovering), RecoveryState::Reconciling) => true,
            (Some(RecoveryState::Reconciling), state) if state.is_terminal() => true,
            (Some(state), next) if state == next && state.is_terminal() => true,
            _ => false,
        };
        if !valid {
            return Err(StorageError::InvalidRecovery(format!(
                "cannot transition agent run from {:?} to {:?}",
                current_state, next
            )));
        }

        transaction.execute(
            "INSERT INTO agent_run_recovery(
                run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                run_id,
                next.as_str(),
                effect_id,
                idempotency_key,
                verifier,
                evidence_json,
                decision,
            ],
        )?;
        let task_id: String = transaction.query_row(
            "SELECT task_id FROM agent_run_effects WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )?;
        let payload = serde_json::to_vec(&serde_json::json!({
            "run_id": run_id,
            "effect_id": effect_id,
            "idempotency_key": idempotency_key,
            "verifier": verifier,
            "evidence": serde_json::from_slice::<serde_json::Value>(evidence_json)
                .unwrap_or_else(|_| serde_json::json!({"raw_bytes": evidence_json})),
            "decision": decision,
            "state": next.as_str(),
        }))?;
        transaction.execute(
            "INSERT INTO events(task_id, event_type, payload) VALUES (?1, 'run.recovery.decision', ?2)",
            rusqlite::params![task_id, payload],
        )?;
        let record = transaction.query_row(
            "SELECT id, run_id, state, effect_id, idempotency_key, verifier,
             evidence_json, decision, created_at
             FROM agent_run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
            [run_id],
            |row| {
                Ok(RunRecoveryRecord {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    state: RecoveryState::parse(&row.get::<_, String>(2)?)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    effect_id: row.get(3)?,
                    idempotency_key: row.get(4)?,
                    verifier: row.get(5)?,
                    evidence_json: row.get(6)?,
                    decision: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn latest_agent_recovery(
        &self,
        run_id: &str,
    ) -> Result<Option<RunRecoveryRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, run_id, state, effect_id, idempotency_key, verifier,
                 evidence_json, decision, created_at
                 FROM agent_run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
                [run_id],
                |row| {
                    Ok(RunRecoveryRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        state: RecoveryState::parse(&row.get::<_, String>(2)?)
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        effect_id: row.get(3)?,
                        idempotency_key: row.get(4)?,
                        verifier: row.get(5)?,
                        evidence_json: row.get(6)?,
                        decision: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn latest_recovery(&self, run_id: &str) -> Result<Option<RunRecoveryRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision, created_at
                 FROM run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
                [run_id],
                |row| {
                    Ok(RunRecoveryRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        state: RecoveryState::parse(&row.get::<_, String>(2)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
                        effect_id: row.get(3)?,
                        idempotency_key: row.get(4)?,
                        verifier: row.get(5)?,
                        evidence_json: row.get(6)?,
                        decision: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_run_status(&self, run_id: &str, status: &str) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE runs SET status = ?1 WHERE id = ?2",
            rusqlite::params![status, run_id],
        )?;
        Ok(())
    }

    pub fn recover_unknown_effects(&self) -> Result<Vec<RecoveredRunRecord>, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut statement = transaction.prepare(
            "SELECT e.run_id, r.work_item_id, e.effect_id, e.kind FROM run_effects e
             JOIN runs r ON r.id = e.run_id WHERE e.state = 'executing'",
        )?;
        let mut records = statement
            .query_map([], |row| {
                Ok(RecoveredRunRecord {
                    run_id: row.get(0)?,
                    work_item_id: row.get(1)?,
                    effect_id: row.get(2)?,
                    kind: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let mut agent_statement = transaction.prepare(
            "SELECT run_id, task_id, effect_id, kind FROM agent_run_effects
             WHERE state = 'executing'",
        )?;
        records.extend(
            agent_statement
                .query_map([], |row| {
                    Ok(RecoveredRunRecord {
                        run_id: row.get(0)?,
                        work_item_id: row.get(1)?,
                        effect_id: row.get(2)?,
                        kind: row.get(3)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?,
        );
        drop(agent_statement);
        for record in &records {
            if record.kind == "agent_task" {
                transaction.execute(
                    "DELETE FROM agent_run_leases WHERE run_id = ?1",
                    [&record.run_id],
                )?;
                transaction.execute(
                    "UPDATE agent_run_effects SET state = 'unknown',
                     completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE effect_id = ?1 AND state = 'executing'",
                    [&record.effect_id],
                )?;
            } else {
                transaction
                    .execute("DELETE FROM run_leases WHERE run_id = ?1", [&record.run_id])?;
                transaction.execute(
                    "UPDATE run_effects SET state = 'unknown', completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE effect_id = ?1 AND state = 'executing'",
                    [&record.effect_id],
                )?;
                transaction.execute(
                    "UPDATE runs SET status = 'blocked' WHERE id = ?1 AND status = 'running'",
                    [&record.run_id],
                )?;
            }
            let payload = serde_json::to_vec(&serde_json::json!({
                "run_id": record.run_id, "effect_id": record.effect_id,
                "reason": "recovery_unknown_effect"
            }))?;
            transaction.execute(
                "INSERT INTO events(task_id, event_type, payload) VALUES (?1, 'run.recovery.blocked', ?2)",
                rusqlite::params![record.work_item_id, payload],
            )?;
        }
        transaction.commit()?;
        Ok(records)
    }

    pub fn append_event(
        &self,
        task_id: &str,
        event_type: &str,
        payload: &[u8],
    ) -> Result<i64, StorageError> {
        self.connection.execute(
            "INSERT INTO events(task_id, event_type, payload) VALUES (?1, ?2, ?3)",
            rusqlite::params![task_id, event_type, payload],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn record_tool_metric(
        &self,
        task_id: &str,
        tool_name: &str,
        iteration: i64,
        ok: bool,
        failure_kind: Option<&str>,
        recovery_hint: bool,
        escalated: bool,
    ) -> Result<i64, StorageError> {
        self.connection.execute(
            "INSERT INTO run_tool_metrics(task_id, tool_name, iteration, ok, failure_kind, recovery_hint, escalated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                task_id,
                tool_name,
                iteration,
                ok as i64,
                failure_kind,
                recovery_hint as i64,
                escalated as i64
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn read_tool_metrics(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<ToolMetricRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, tool_name, iteration, ok, failure_kind, recovery_hint, escalated, created_at
             FROM run_tool_metrics WHERE task_id = ?1 ORDER BY id LIMIT ?2",
        )?;
        let rows = statement.query_map(rusqlite::params![task_id, limit as i64], |row| {
            Ok(ToolMetricRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                tool_name: row.get(2)?,
                iteration: row.get(3)?,
                ok: row.get::<_, i64>(4)? != 0,
                failure_kind: row.get(5)?,
                recovery_hint: row.get::<_, i64>(6)? != 0,
                escalated: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Reads the most recent `run_tool_metrics` rows across all tasks,
    /// newest first, bounded by `limit`. Used by Core Doctor log/metrics
    /// export; carries no secrets (tool names, outcomes, recovery hints).
    pub fn read_recent_tool_metrics(
        &self,
        limit: usize,
    ) -> Result<Vec<ToolMetricRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, tool_name, iteration, ok, failure_kind, recovery_hint, escalated, created_at
             FROM run_tool_metrics ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(rusqlite::params![limit as i64], |row| {
            Ok(ToolMetricRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                tool_name: row.get(2)?,
                iteration: row.get(3)?,
                ok: row.get::<_, i64>(4)? != 0,
                failure_kind: row.get(5)?,
                recovery_hint: row.get::<_, i64>(6)? != 0,
                escalated: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Highest sequence the journal has recorded, or zero when it is empty.
    pub fn latest_event_sequence(&self) -> Result<i64, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT COALESCE(MAX(sequence_id), 0) FROM events")?;
        Ok(statement.query_row([], |row| row.get(0))?)
    }

    pub fn read_events_after(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT sequence_id, task_id, event_type, payload, created_at
             FROM events WHERE sequence_id > ?1 ORDER BY sequence_id LIMIT ?2",
        )?;
        let limit = limit.min(i64::MAX as usize) as i64;
        let rows = statement.query_map(rusqlite::params![after_sequence, limit], |row| {
            Ok(EventRecord {
                sequence_id: row.get(0)?,
                task_id: row.get(1)?,
                event_type: row.get(2)?,
                payload: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn read_task_events(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT sequence_id, task_id, event_type, payload, created_at
             FROM events WHERE task_id = ?1 ORDER BY sequence_id DESC LIMIT ?2",
        )?;
        let limit = limit.min(i64::MAX as usize) as i64;
        let rows = statement.query_map(rusqlite::params![task_id, limit], |row| {
            Ok(EventRecord {
                sequence_id: row.get(0)?,
                task_id: row.get(1)?,
                event_type: row.get(2)?,
                payload: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut events = rows.collect::<Result<Vec<_>, _>>()?;
        events.reverse();
        Ok(events)
    }

    /// Returns completed review events newest first. Review ids are prefixed
    /// by the Core review contract, so normal agent task history is excluded.
    /// Completed reviews the history should show.
    ///
    /// Clearing the history appends a marker rather than deleting rows, so the
    /// query starts after the newest marker and older reviews stay in the
    /// journal for audit and export.
    pub fn read_review_events(&self, limit: usize) -> Result<Vec<EventRecord>, StorageError> {
        let floor: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(sequence_id), 0) FROM events WHERE event_type = 'review.history_cleared'",
            [],
            |row| row.get(0),
        )?;
        let mut statement = self.connection.prepare(
            "SELECT sequence_id, task_id, event_type, payload, created_at
             FROM events WHERE task_id LIKE 'review-%' AND event_type = 'task.completed'
               AND sequence_id > ?2
             ORDER BY sequence_id DESC LIMIT ?1",
        )?;
        let limit = limit.min(i64::MAX as usize) as i64;
        let rows = statement.query_map([limit, floor], |row| {
            Ok(EventRecord {
                sequence_id: row.get(0)?,
                task_id: row.get(1)?,
                event_type: row.get(2)?,
                payload: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn export_events_jsonl(&self, output: impl AsRef<Path>) -> Result<(), StorageError> {
        if let Some(parent) = output.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(output)?;
        let mut writer = BufWriter::new(file);
        for event in self.read_events_after(0, usize::MAX)? {
            let payload = serde_json::from_slice::<serde_json::Value>(&event.payload)
                .unwrap_or_else(|_| serde_json::json!({"raw_bytes": event.payload}));
            serde_json::to_writer(
                &mut writer,
                &serde_json::json!({
                    "sequence_id": event.sequence_id,
                    "task_id": event.task_id,
                    "event_type": event.event_type,
                    "payload": payload,
                    "created_at": event.created_at,
                }),
            )?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;
        Ok(())
    }

    /// Returns a bounded, read-only health/retention summary.
    ///
    /// The table list is fixed to the schema owned by this crate, while event
    /// types are capped so a noisy database cannot produce an unbounded
    /// response. This method performs only SELECTs and does not affect
    /// recovery state or retention data.
    pub fn read_diagnostics_summary(
        &self,
        max_event_types: usize,
    ) -> Result<DiagnosticsSummary, StorageError> {
        const TABLES: [&str; 24] = [
            "events",
            "projects",
            "work_items",
            "work_item_edges",
            "provenance",
            "runs",
            "command_dedup",
            "snapshots",
            "run_checkpoints",
            "run_effects",
            "project_policies",
            "run_leases",
            "run_reconciliations",
            "run_recovery",
            "agent_run_effects",
            "agent_run_leases",
            "agent_run_reconciliations",
            "agent_run_recovery",
            "workspace_index_runs",
            "workspace_documents",
            "document_chunks",
            "workspace_vector_indexes",
            "workspace_chunk_vectors",
            "rag_context_ledger",
        ];

        let mut table_counts = Vec::with_capacity(TABLES.len());
        for table in TABLES {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            let rows = self.connection.query_row(&sql, [], |row| row.get(0))?;
            table_counts.push(DiagnosticsTableCount {
                table: table.to_string(),
                rows,
            });
        }

        let total_events = self
            .connection
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        let limit = max_event_types.min(MAX_DIAGNOSTICS_EVENT_TYPES);
        let query_limit = limit.saturating_add(1).min(i64::MAX as usize) as i64;
        let mut statement = self.connection.prepare(
            "SELECT event_type, COUNT(*) AS rows
             FROM events GROUP BY event_type
             ORDER BY rows DESC, event_type ASC LIMIT ?1",
        )?;
        let rows = statement.query_map([query_limit], |row| {
            Ok(DiagnosticsEventCount {
                event_type: row.get(0)?,
                rows: row.get(1)?,
            })
        })?;
        let mut event_counts = rows.collect::<Result<Vec<_>, _>>()?;
        let event_types_truncated = event_counts.len() > limit;
        event_counts.truncate(limit);

        Ok(DiagnosticsSummary {
            table_counts,
            event_counts,
            total_events,
            event_types_truncated,
        })
    }

    /// Returns a bounded, read-only summary of recovery-relevant state.
    ///
    /// This performs only SELECTs; it does not transition run/effect state.
    /// Use `recover_unknown_effects` for the mutating recovery flow.
    pub fn read_recovery_health(&self) -> Result<RecoveryHealthSnapshot, StorageError> {
        let unknown_effects: i64 = self.connection.query_row(
            "SELECT
                 (SELECT COUNT(*) FROM run_effects WHERE state = 'unknown') +
                 (SELECT COUNT(*) FROM agent_run_effects WHERE state = 'unknown')",
            [],
            |row| row.get(0),
        )?;
        let lease_expired: bool = self.connection.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM run_leases
                 WHERE lease_expires_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             ) OR EXISTS(
                 SELECT 1 FROM agent_run_leases
                 WHERE lease_expires_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             )",
            [],
            |row| row.get(0),
        )?;
        let resumable_runs: i64 = self.connection.query_row(
            "SELECT
                 (SELECT COUNT(*) FROM runs WHERE status = 'blocked') +
                 (SELECT COUNT(*) FROM agent_run_recovery
                  WHERE state IN ('RESUMABLE', 'BLOCKED') AND id IN (
                      SELECT MAX(id) FROM agent_run_recovery GROUP BY run_id
                  ))",
            [],
            |row| row.get(0),
        )?;
        Ok(RecoveryHealthSnapshot {
            unknown_effects,
            lease_expired,
            resumable_runs,
        })
    }

    fn read_schema_version(connection: &Connection) -> Result<u32, rusqlite::Error> {
        connection.query_row("PRAGMA user_version", [], |row| row.get(0))
    }

    fn migrate(
        connection: &Connection,
        current: u32,
        fail_migration: bool,
    ) -> Result<(), StorageError> {
        let transaction = connection.unchecked_transaction()?;
        if current < 1 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS events (
                    sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    payload BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_events_task_sequence ON events(task_id, sequence_id);
                PRAGMA user_version = 1;",
            )?;
        }
        if fail_migration {
            return Err(rusqlite::Error::InvalidQuery.into());
        }
        if current < 2 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY, title TEXT NOT NULL, workspace_path TEXT NOT NULL,
                    source_ref TEXT, version INTEGER NOT NULL DEFAULT 1,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE IF NOT EXISTS work_items (
                    id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), parent_id TEXT REFERENCES work_items(id),
                    title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', source_ref TEXT,
                    acceptance_criteria TEXT NOT NULL DEFAULT '', non_goals TEXT NOT NULL DEFAULT '',
                    status TEXT NOT NULL CHECK(status IN ('backlog','ready','in_progress','done')),
                    priority INTEGER NOT NULL DEFAULT 0, estimate INTEGER, complexity TEXT,
                    attempt_count INTEGER NOT NULL DEFAULT 0, version INTEGER NOT NULL DEFAULT 1,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE IF NOT EXISTS work_item_edges (
                    from_work_item_id TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
                    to_work_item_id TEXT NOT NULL REFERENCES work_items(id) ON DELETE CASCADE,
                    kind TEXT NOT NULL, PRIMARY KEY(from_work_item_id, to_work_item_id, kind)
                );
                CREATE TABLE IF NOT EXISTS provenance (
                    id TEXT PRIMARY KEY, kind TEXT NOT NULL, source TEXT NOT NULL,
                    payload BLOB NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE IF NOT EXISTS runs (
                    id TEXT PRIMARY KEY, work_item_id TEXT NOT NULL REFERENCES work_items(id),
                    status TEXT NOT NULL, policy_snapshot BLOB NOT NULL, role_snapshot BLOB NOT NULL,
                    skill_snapshot BLOB NOT NULL, model_route_snapshot BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE IF NOT EXISTS command_dedup (
                    client_id TEXT NOT NULL, request_id TEXT NOT NULL, command_hash TEXT NOT NULL,
                    result BLOB NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    PRIMARY KEY(client_id, request_id)
                );
                PRAGMA user_version = 2;",
                )?;
        }
        if current < 3 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS snapshots (
                    id TEXT PRIMARY KEY, run_id TEXT NOT NULL, workspace_hash TEXT NOT NULL,
                    payload BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_snapshots_run ON snapshots(run_id);
                PRAGMA user_version = 3;",
            )?;
        }
        if current < 4 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS run_checkpoints (
                    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                    checkpoint_id TEXT PRIMARY KEY, stage TEXT NOT NULL, node_id TEXT NOT NULL,
                    attempt INTEGER NOT NULL, input_hash TEXT NOT NULL, state_json BLOB NOT NULL,
                    pending_effects_json BLOB NOT NULL,
                    committed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_run_checkpoints_run ON run_checkpoints(run_id, committed_at);
                CREATE TABLE IF NOT EXISTS run_effects (
                    effect_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                    node_id TEXT NOT NULL, kind TEXT NOT NULL, idempotency_key TEXT NOT NULL UNIQUE,
                    immutable_intent_hash TEXT NOT NULL, state TEXT NOT NULL,
                    started_at TEXT, completed_at TEXT, result_hash TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_run_effects_run ON run_effects(run_id);
                PRAGMA user_version = 4;",
                )?;
        }
        if current < 5 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS project_policies (
                    project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
                    policy_json BLOB NOT NULL,
                    version INTEGER NOT NULL DEFAULT 1,
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                PRAGMA user_version = 5;",
            )?;
        }
        if current < 6 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS run_leases (
                    run_id TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE,
                    lease_id TEXT NOT NULL UNIQUE,
                    owner_id TEXT NOT NULL,
                    generation INTEGER NOT NULL,
                    lease_expires_at TEXT NOT NULL,
                    heartbeat_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS run_reconciliations (
                    effect_id TEXT PRIMARY KEY REFERENCES run_effects(effect_id) ON DELETE CASCADE,
                    state TEXT NOT NULL,
                    verifier TEXT NOT NULL,
                    evidence_json BLOB NOT NULL,
                    reconciled_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                PRAGMA user_version = 6;",
            )?;
        }
        if current < 7 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS run_recovery (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                    state TEXT NOT NULL,
                    effect_id TEXT NOT NULL,
                    idempotency_key TEXT NOT NULL,
                    verifier TEXT NOT NULL,
                    evidence_json BLOB NOT NULL,
                    decision TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_run_recovery_run ON run_recovery(run_id, id);
                PRAGMA user_version = 7;",
            )?;
        }
        if current < 8 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS research_evidence (
                    id TEXT PRIMARY KEY NOT NULL,
                    source_kind TEXT NOT NULL,
                    source_ref TEXT NOT NULL,
                    redacted_excerpt TEXT NOT NULL,
                    source_hash TEXT NOT NULL,
                    fetched_at TEXT NOT NULL,
                    ttl_seconds INTEGER NOT NULL,
                    provenance_link TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_research_evidence_provenance ON research_evidence(provenance_link);
                CREATE TABLE IF NOT EXISTS memory_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    content TEXT NOT NULL,
                    provenance TEXT NOT NULL,
                    privacy TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    expires_at TEXT,
                    archived INTEGER NOT NULL,
                    forgotten INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_memory_entries_scope ON memory_entries(scope_kind, scope_id);
                PRAGMA user_version = 8;",
            )?;
        }
        if current < 9 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS capability_manifests (
                    id TEXT PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL,
                    version TEXT NOT NULL,
                    risk_class TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    manifest_json BLOB NOT NULL,
                    installed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_capability_manifests_kind ON capability_manifests(kind);
                PRAGMA user_version = 9;",
            )?;
        }
        if current < 10 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS child_handoffs (
                    handoff_id TEXT PRIMARY KEY NOT NULL,
                    task_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    from_role TEXT NOT NULL,
                    to_role TEXT NOT NULL,
                    sequence INTEGER NOT NULL,
                    envelope_json BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_child_handoffs_task ON child_handoffs(task_id);
                CREATE TABLE IF NOT EXISTS child_task_requests (
                    child_task_id TEXT PRIMARY KEY NOT NULL,
                    parent_task_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    request_json BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_child_task_requests_parent ON child_task_requests(parent_task_id);
                CREATE TABLE IF NOT EXISTS child_reports (
                    child_task_id TEXT PRIMARY KEY NOT NULL,
                    parent_task_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    confidence_percent INTEGER NOT NULL,
                    report_json BLOB NOT NULL,
                    accepted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_child_reports_parent ON child_reports(parent_task_id);
                PRAGMA user_version = 10;",
            )?;
        }
        if current < 11 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS run_tool_metrics (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    tool_name TEXT NOT NULL,
                    iteration INTEGER NOT NULL,
                    ok INTEGER NOT NULL,
                    failure_kind TEXT,
                    recovery_hint INTEGER NOT NULL,
                    escalated INTEGER NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_run_tool_metrics_task
                    ON run_tool_metrics(task_id, id);
                CREATE INDEX IF NOT EXISTS idx_run_tool_metrics_tool
                    ON run_tool_metrics(task_id, tool_name, id);
                PRAGMA user_version = 11;",
            )?;
        }
        if current < 12 {
            transaction.execute_batch(
                "ALTER TABLE memory_entries ADD COLUMN confirmations INTEGER NOT NULL DEFAULT 1;
                 ALTER TABLE memory_entries ADD COLUMN lesson_key TEXT;
                 CREATE INDEX IF NOT EXISTS idx_memory_entries_lesson
                    ON memory_entries(scope_kind, scope_id, lesson_key);
                 PRAGMA user_version = 12;",
            )?;
        }
        if current < 13 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS capability_selections (
                    task_id TEXT PRIMARY KEY NOT NULL,
                    origin TEXT NOT NULL,
                    manifest_name TEXT NOT NULL,
                    state_json BLOB NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                PRAGMA user_version = 13;",
            )?;
        }
        if current < 14 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS feedback_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    run_id TEXT NOT NULL,
                    task_id TEXT,
                    subject_ref TEXT,
                    signal TEXT NOT NULL,
                    correction TEXT,
                    rejection_reason TEXT,
                    outcome TEXT,
                    provenance TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_feedback_entries_run ON feedback_entries(run_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_feedback_entries_signal ON feedback_entries(signal);
                PRAGMA user_version = 14;",
                )?;
        }
        if current < 15 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS agent_run_effects (
                    effect_id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL UNIQUE,
                    task_id TEXT NOT NULL,
                    node_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    idempotency_key TEXT NOT NULL UNIQUE,
                    immutable_intent_hash TEXT NOT NULL,
                    state TEXT NOT NULL,
                    started_at TEXT,
                    completed_at TEXT,
                    result_hash TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_agent_run_effects_task
                    ON agent_run_effects(task_id, started_at);
                CREATE TABLE IF NOT EXISTS agent_run_leases (
                    run_id TEXT PRIMARY KEY REFERENCES agent_run_effects(run_id) ON DELETE CASCADE,
                    lease_id TEXT NOT NULL UNIQUE,
                    owner_id TEXT NOT NULL,
                    generation INTEGER NOT NULL,
                    lease_expires_at TEXT NOT NULL,
                    heartbeat_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS agent_run_reconciliations (
                    effect_id TEXT PRIMARY KEY REFERENCES agent_run_effects(effect_id) ON DELETE CASCADE,
                    state TEXT NOT NULL,
                    verifier TEXT NOT NULL,
                    evidence_json BLOB NOT NULL,
                    reconciled_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE IF NOT EXISTS agent_run_recovery (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    run_id TEXT NOT NULL,
                    state TEXT NOT NULL,
                    effect_id TEXT NOT NULL,
                    idempotency_key TEXT NOT NULL,
                    verifier TEXT NOT NULL,
                    evidence_json BLOB NOT NULL,
                    decision TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_agent_run_recovery_run
                    ON agent_run_recovery(run_id, id);
                PRAGMA user_version = 15;",
            )?;
        }
        if current < 16 {
            // Memory Extraction: kind/state/confidence/provenance-контракт
            // поверх Memory v1. Все legacy rows остаются активной памятью
            // (`state = confirmed`) и помечаются legacy-версиями извлекателя и
            // policy, чтобы их нельзя было спутать с model-generated
            // кандидатами. `canonical_subject` намеренно остаётся NULL: точный
            // нормализатор версионируется в Core и применяется к `title` при
            // чтении, а не приблизительной SQL-нормализацией во время
            // миграции.
            transaction.execute_batch(
                "ALTER TABLE memory_entries ADD COLUMN kind TEXT NOT NULL DEFAULT 'entity';
                 ALTER TABLE memory_entries ADD COLUMN canonical_subject TEXT;
                 ALTER TABLE memory_entries ADD COLUMN confirmation_state TEXT NOT NULL DEFAULT 'confirmed';
                 ALTER TABLE memory_entries ADD COLUMN model_confidence REAL NOT NULL DEFAULT 1.0;
                 ALTER TABLE memory_entries ADD COLUMN verification_confidence REAL NOT NULL DEFAULT 1.0;
                 ALTER TABLE memory_entries ADD COLUMN privacy_class TEXT NOT NULL DEFAULT 'normal';
                 ALTER TABLE memory_entries ADD COLUMN source_trust TEXT NOT NULL DEFAULT 'user';
                 ALTER TABLE memory_entries ADD COLUMN supersedes TEXT;
                 ALTER TABLE memory_entries ADD COLUMN superseded_by TEXT;
                 ALTER TABLE memory_entries ADD COLUMN supersession_reason TEXT;
                 ALTER TABLE memory_entries ADD COLUMN extractor_version TEXT NOT NULL DEFAULT 'v1_legacy';
                 ALTER TABLE memory_entries ADD COLUMN policy_version TEXT NOT NULL DEFAULT 'legacy-v1';
                 ALTER TABLE memory_entries ADD COLUMN validation_status TEXT NOT NULL DEFAULT 'not_required';
                 ALTER TABLE memory_entries ADD COLUMN validated_at TEXT;
                 ALTER TABLE memory_entries ADD COLUMN provenance_source_id TEXT;
                 UPDATE memory_entries SET kind = 'lesson' WHERE lesson_key IS NOT NULL;
                 UPDATE memory_entries SET confirmation_state = 'forgotten' WHERE forgotten = 1;
                 CREATE INDEX IF NOT EXISTS idx_memory_entries_kind
                    ON memory_entries(scope_kind, scope_id, kind);
                 CREATE INDEX IF NOT EXISTS idx_memory_entries_state
                    ON memory_entries(confirmation_state);
                 CREATE INDEX IF NOT EXISTS idx_memory_entries_subject
                    ON memory_entries(canonical_subject, scope_kind, scope_id);
                 CREATE INDEX IF NOT EXISTS idx_memory_entries_expires
                    ON memory_entries(expires_at);
                 CREATE INDEX IF NOT EXISTS idx_memory_entries_provenance_source
                    ON memory_entries(provenance_source_id);
                 CREATE TABLE IF NOT EXISTS memory_aliases (
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    alias TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    registered_by TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (scope_kind, scope_id, alias)
                 );
                 CREATE TABLE IF NOT EXISTS memory_tombstones (
                    tombstone_id TEXT PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    forgotten_at TEXT NOT NULL,
                    reason_class TEXT NOT NULL,
                    digest TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS memory_session_notes (
                    id TEXT PRIMARY KEY NOT NULL,
                    session_id TEXT NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    statement TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_memory_session_notes_session
                    ON memory_session_notes(session_id, expires_at);
                 PRAGMA user_version = 16;",
            )?;
        }
        if current < 17 {
            // План 01: context ledger, scratchpad задачи и artifact store.
            // Миграция additive: новые таблицы создаются рядом, существующие
            // записи не переписываются.
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS context_ledger (
                    id TEXT PRIMARY KEY NOT NULL,
                    schema_version INTEGER NOT NULL,
                    task_id TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    model_call_id TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    provider TEXT NOT NULL,
                    model TEXT NOT NULL,
                    profile_version TEXT NOT NULL,
                    profile_snapshot TEXT NOT NULL,
                    tokenizer_version TEXT NOT NULL,
                    normalizer_version TEXT NOT NULL,
                    strategy_version TEXT NOT NULL,
                    mandatory_tokens INTEGER NOT NULL,
                    selected_optional_tokens INTEGER NOT NULL,
                    reserves_tokens INTEGER NOT NULL,
                    estimated_prompt_tokens INTEGER NOT NULL,
                    selected_items TEXT NOT NULL DEFAULT '[]',
                    dropped_items TEXT NOT NULL DEFAULT '[]',
                    mandatory_parts TEXT NOT NULL DEFAULT '[]',
                    ladder_levels_applied TEXT NOT NULL DEFAULT '[]',
                    compression TEXT NOT NULL DEFAULT '[]',
                    loadout TEXT,
                    fallback_estimator INTEGER NOT NULL DEFAULT 0,
                    replan_of TEXT,
                    outcome TEXT NOT NULL,
                    budget_unavailable TEXT,
                    context_ledger_hash TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_context_ledger_task
                    ON context_ledger(task_id, created_at);
                 CREATE INDEX IF NOT EXISTS idx_context_ledger_session
                    ON context_ledger(session_id, created_at);
                 CREATE INDEX IF NOT EXISTS idx_context_ledger_hash
                    ON context_ledger(context_ledger_hash);
                 CREATE TABLE IF NOT EXISTS context_ledger_usage (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    ledger_id TEXT NOT NULL,
                    actual_prompt_tokens INTEGER NOT NULL,
                    actual_completion_tokens INTEGER NOT NULL,
                    estimator_drift REAL NOT NULL,
                    recorded_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_context_ledger_usage_ledger
                    ON context_ledger_usage(ledger_id);
                 CREATE TABLE IF NOT EXISTS context_ledger_receipts (
                    ledger_id TEXT NOT NULL,
                    receipt_id TEXT NOT NULL,
                    exported INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (ledger_id, receipt_id)
                 );
                 CREATE TABLE IF NOT EXISTS task_scratchpad (
                    id TEXT PRIMARY KEY NOT NULL,
                    task_id TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    category TEXT NOT NULL,
                    status TEXT NOT NULL,
                    trust TEXT NOT NULL,
                    privacy TEXT NOT NULL,
                    revision INTEGER NOT NULL DEFAULT 1,
                    parent_id TEXT,
                    content TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    ttl_ms INTEGER,
                    confirmation TEXT,
                    artifact_locator TEXT,
                    recovered_at_step INTEGER
                 );
                 CREATE INDEX IF NOT EXISTS idx_task_scratchpad_task
                    ON task_scratchpad(task_id, category, status);
                 CREATE INDEX IF NOT EXISTS idx_task_scratchpad_parent
                    ON task_scratchpad(parent_id, revision);
                 CREATE TABLE IF NOT EXISTS task_artifacts (
                    content_hash TEXT PRIMARY KEY NOT NULL,
                    bytes INTEGER NOT NULL,
                    content BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    last_access_at INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS task_artifact_refs (
                    locator TEXT PRIMARY KEY NOT NULL,
                    content_hash TEXT NOT NULL,
                    task_id TEXT NOT NULL,
                    owner_task_id TEXT NOT NULL,
                    bytes INTEGER NOT NULL,
                    privacy TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    last_access_at INTEGER NOT NULL,
                    ttl_ms INTEGER,
                    summary TEXT NOT NULL DEFAULT ''
                 );
                 CREATE INDEX IF NOT EXISTS idx_task_artifact_refs_hash
                    ON task_artifact_refs(content_hash);
                 CREATE INDEX IF NOT EXISTS idx_task_artifact_refs_task
                    ON task_artifact_refs(task_id, status);
                 CREATE TABLE IF NOT EXISTS artifact_tombstones (
                    content_hash TEXT PRIMARY KEY NOT NULL,
                    bytes INTEGER NOT NULL,
                    removed_at INTEGER NOT NULL,
                    reason TEXT NOT NULL
                 );
                 PRAGMA user_version = 17;",
            )?;
        }
        if current < 18 {
            // План 01.5: pin/unpin item, журнал mutation-команд контекста и
            // rate limit поверх него.
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS context_pins (
                    task_id TEXT NOT NULL,
                    item_id TEXT NOT NULL,
                    pinned INTEGER NOT NULL DEFAULT 1,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY (task_id, item_id)
                 );
                 CREATE TABLE IF NOT EXISTS context_command_audit (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    command TEXT NOT NULL,
                    subject TEXT,
                    outcome TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_context_command_audit_rate
                    ON context_command_audit(task_id, command, created_at);
                 PRAGMA user_version = 18;",
            )?;
        }
        if current < 19 {
            // Local Agentic RAG: generation-published workspace documents,
            // bounded chunks, FTS5, optional vector generations and a
            // metadata-only citation ledger. Retrieval only joins rows from
            // the single published generation for a workspace.
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS workspace_index_runs (
                    run_id TEXT PRIMARY KEY NOT NULL,
                    workspace_key TEXT NOT NULL,
                    generation INTEGER NOT NULL,
                    status TEXT NOT NULL CHECK(status IN
                        ('running','published','superseded','cancelled','failed')),
                    started_at INTEGER NOT NULL,
                    finished_at INTEGER,
                    published_at INTEGER,
                    scanner_version TEXT NOT NULL,
                    chunker_version TEXT NOT NULL,
                    tokenizer_version TEXT NOT NULL,
                    file_count INTEGER NOT NULL DEFAULT 0,
                    chunk_count INTEGER NOT NULL DEFAULT 0,
                    excluded_count INTEGER NOT NULL DEFAULT 0,
                    error_count INTEGER NOT NULL DEFAULT 0,
                    error_summary TEXT NOT NULL DEFAULT '[]',
                    dirty INTEGER NOT NULL DEFAULT 1,
                    UNIQUE(workspace_key, generation)
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_index_published
                    ON workspace_index_runs(workspace_key) WHERE status = 'published';
                 CREATE INDEX IF NOT EXISTS idx_workspace_index_runs_state
                    ON workspace_index_runs(workspace_key, status, generation);

                 CREATE TABLE IF NOT EXISTS workspace_documents (
                    document_id TEXT PRIMARY KEY NOT NULL,
                    workspace_key TEXT NOT NULL,
                    path TEXT NOT NULL,
                    generation INTEGER NOT NULL,
                    language TEXT NOT NULL,
                    mime TEXT NOT NULL,
                    file_hash TEXT NOT NULL,
                    size_bytes INTEGER NOT NULL,
                    encoding TEXT NOT NULL,
                    decode_status TEXT NOT NULL,
                    last_modified INTEGER NOT NULL,
                    indexed_at INTEGER NOT NULL,
                    status TEXT NOT NULL CHECK(status IN ('active','unstable','deleted')),
                    redaction_status TEXT NOT NULL CHECK(redaction_status IN ('none','partial','full')),
                    is_secret_path INTEGER NOT NULL DEFAULT 0,
                    UNIQUE(workspace_key, generation, path)
                 );
                 CREATE INDEX IF NOT EXISTS idx_workspace_documents_scope
                    ON workspace_documents(workspace_key, generation, status, path);
                 CREATE INDEX IF NOT EXISTS idx_workspace_documents_language
                    ON workspace_documents(workspace_key, generation, language);

                 CREATE TABLE IF NOT EXISTS document_chunks (
                    chunk_id TEXT PRIMARY KEY NOT NULL,
                    document_id TEXT NOT NULL,
                    workspace_key TEXT NOT NULL,
                    generation INTEGER NOT NULL,
                    ordinal INTEGER NOT NULL,
                    chunk_hash TEXT NOT NULL,
                    byte_start INTEGER NOT NULL,
                    byte_end INTEGER NOT NULL,
                    line_start INTEGER,
                    line_end INTEGER,
                    parent_context TEXT NOT NULL,
                    chunk_text TEXT NOT NULL,
                    symbol TEXT,
                    symbol_normalized TEXT NOT NULL DEFAULT '',
                    token_count INTEGER NOT NULL,
                    byte_count INTEGER NOT NULL,
                    strategy_version TEXT NOT NULL,
                    FOREIGN KEY(document_id) REFERENCES workspace_documents(document_id)
                        ON DELETE CASCADE,
                    UNIQUE(document_id, generation, ordinal)
                 );
                 CREATE INDEX IF NOT EXISTS idx_document_chunks_active
                    ON document_chunks(workspace_key, generation, document_id, ordinal);
                 CREATE INDEX IF NOT EXISTS idx_document_chunks_hash
                    ON document_chunks(workspace_key, chunk_hash);

                 CREATE VIRTUAL TABLE IF NOT EXISTS workspace_chunks_fts USING fts5(
                    chunk_text,
                    symbol_normalized,
                    path,
                    parent_context,
                    chunk_id UNINDEXED,
                    workspace_key UNINDEXED,
                    generation UNINDEXED,
                    tokenize='trigram'
                 );

                 CREATE TABLE IF NOT EXISTS workspace_vector_indexes (
                    index_id TEXT PRIMARY KEY NOT NULL,
                    workspace_key TEXT NOT NULL,
                    source_generation INTEGER NOT NULL,
                    embedding_model_id TEXT NOT NULL,
                    embedding_model_version TEXT NOT NULL,
                    vector_dimension INTEGER NOT NULL,
                    distance_metric TEXT NOT NULL,
                    normalization TEXT NOT NULL,
                    chunker_version TEXT NOT NULL,
                    build_status TEXT NOT NULL CHECK(build_status IN
                        ('building','ready','published','deprecated','failed','cancelled')),
                    created_at INTEGER NOT NULL,
                    published_at INTEGER,
                    vector_count INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_vector_published
                    ON workspace_vector_indexes(workspace_key) WHERE build_status = 'published';
                 CREATE INDEX IF NOT EXISTS idx_workspace_vector_state
                    ON workspace_vector_indexes(workspace_key, source_generation, build_status);
                 CREATE TABLE IF NOT EXISTS workspace_chunk_vectors (
                    index_id TEXT NOT NULL,
                    chunk_id TEXT NOT NULL,
                    vector BLOB NOT NULL,
                    PRIMARY KEY(index_id, chunk_id),
                    FOREIGN KEY(index_id) REFERENCES workspace_vector_indexes(index_id)
                        ON DELETE CASCADE,
                    FOREIGN KEY(chunk_id) REFERENCES document_chunks(chunk_id)
                        ON DELETE CASCADE
                 );

                 CREATE TABLE IF NOT EXISTS rag_context_ledger (
                    ledger_id TEXT NOT NULL,
                    query_id TEXT NOT NULL,
                    block_id TEXT NOT NULL,
                    rank INTEGER NOT NULL,
                    retrieval_score REAL NOT NULL,
                    checker_confidence REAL NOT NULL,
                    chunk_hash TEXT NOT NULL,
                    snippet_hash TEXT NOT NULL,
                    path TEXT NOT NULL,
                    line_start INTEGER,
                    line_end INTEGER,
                    citation_status TEXT NOT NULL,
                    selection_reason TEXT NOT NULL,
                    reread_result TEXT NOT NULL,
                    error_code TEXT,
                    created_at INTEGER NOT NULL,
                    PRIMARY KEY(ledger_id, block_id)
                 );
                 CREATE INDEX IF NOT EXISTS idx_rag_context_ledger_query
                    ON rag_context_ledger(query_id, rank);
                 PRAGMA user_version = 19;",
            )?;
        }
        if current < 20 {
            // Лимиты приходят от провайдера и живут дольше одного запуска: без
            // них планировщик контекста и ревью считают окно вслепую, а каталог
            // перечитывается не при каждом действии.
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS model_context_limits (
                    model TEXT PRIMARY KEY,
                    provider TEXT NOT NULL,
                    context_tokens INTEGER,
                    max_output_tokens INTEGER,
                    fetched_at TEXT NOT NULL
                 );
                 PRAGMA user_version = 20;",
            )?;
        }
        if current < 21 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS receipt_key_transitions (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    transition_id TEXT NOT NULL UNIQUE,
                    transition_hash TEXT NOT NULL UNIQUE,
                    previous_key_id TEXT,
                    new_key_id TEXT NOT NULL,
                    continuity TEXT NOT NULL,
                    canonical_json BLOB NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_receipt_key_transitions_new_key
                    ON receipt_key_transitions(new_key_id, sequence);
                 CREATE TABLE IF NOT EXISTS receipt_key_audit (
                    event_id TEXT PRIMARY KEY,
                    transition_id TEXT NOT NULL UNIQUE,
                    event_type TEXT NOT NULL,
                    old_key_id TEXT,
                    new_key_id TEXT,
                    transition_hash TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    actor TEXT NOT NULL,
                    outcome TEXT NOT NULL,
                    error_code TEXT,
                    created_at TEXT NOT NULL
                 );
                 PRAGMA user_version = 21;",
            )?;
        }
        if current < 22 {
            transaction.execute_batch("PRAGMA user_version = 22;")?;
        }
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiagnosticsSummary, ImportedTask, LocalDatabase, ModelRouteSnapshot, PolicySnapshot,
        RecoveryState, RoleRef, RunCheckpointRecord, RunEffectRecord, RunRecord, RunSnapshots,
        SkillRef, StorageError, WorkItemRecord, SCHEMA_VERSION,
    };
    use std::path::PathBuf;

    fn temp_database_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("evohime-test-{name}-{}.db", std::process::id()))
    }

    /// Clearing the review history must hide earlier runs from the list while
    /// leaving them in the journal, which stays append-only for audit and
    /// export.
    #[test]
    fn review_history_starts_after_the_newest_clear_marker() {
        let path = temp_database_path("review-history");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .append_event("review-old", "task.completed", b"{}")
            .expect("old review records");
        assert_eq!(
            database
                .read_review_events(10)
                .expect("history reads")
                .len(),
            1
        );

        database
            .append_event("review-history-1", "review.history_cleared", b"{}")
            .expect("marker records");
        assert!(database
            .read_review_events(10)
            .expect("history reads")
            .is_empty());

        database
            .append_event("review-new", "task.completed", b"{}")
            .expect("new review records");
        let visible = database.read_review_events(10).expect("history reads");
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].task_id, "review-new");
        // The cleared review is hidden from the list, not removed.
        let all = database
            .read_events_after(0, usize::MAX)
            .expect("journal reads");
        assert!(all.iter().any(|event| event.task_id == "review-old"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn creates_schema_and_reports_version() {
        let path = temp_database_path("schema");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        assert!(database.has_events_table().expect("table exists"));
        let id = database
            .record_tool_metric(
                "task-1",
                "filesystem.read",
                2,
                false,
                Some("not_found"),
                true,
                false,
            )
            .expect("metric records");
        assert!(id > 0);
        let metrics = database
            .read_tool_metrics("task-1", 10)
            .expect("metrics read");
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].failure_kind.as_deref(), Some("not_found"));
        assert!(metrics[0].recovery_hint);
        drop(database);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migrates_schema_18_to_workspace_rag_schema_19_transactionally() {
        let path = temp_database_path("migration-18-to-19-rag");
        let _ = std::fs::remove_file(&path);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch("CREATE TABLE legacy_marker(id INTEGER); PRAGMA user_version = 18;")
                .expect("legacy schema seeds");
        }
        let database = LocalDatabase::open(&path).expect("migration succeeds");
        // Открытие всегда доводит базу до текущей версии, поэтому проверяется
        // не «19», а наличие таблиц, которые добавила именно эта миграция.
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        for table in [
            "workspace_index_runs",
            "workspace_documents",
            "document_chunks",
            "workspace_vector_indexes",
            "workspace_chunk_vectors",
            "rag_context_ledger",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "{table} must exist after migration");
        }
        let _ = std::fs::remove_file(path);
    }

    /// Лимиты моделей должны появиться на уже работающей базе: пользователь
    /// обновляет сборку, а не заводит хранилище заново.
    #[test]
    fn migrates_schema_19_to_model_context_limits_schema_20() {
        let path = temp_database_path("migration-19-to-20-limits");
        let _ = std::fs::remove_file(&path);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch("CREATE TABLE legacy_marker(id INTEGER); PRAGMA user_version = 19;")
                .expect("legacy schema seeds");
        }
        let database = LocalDatabase::open(&path).expect("migration succeeds");
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'model_context_limits'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "model_context_limits must exist after migration");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn backs_up_existing_database_before_migration() {
        let path = temp_database_path("backup");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .pragma_update(None, "user_version", 0_u32)
                .expect("legacy version writes");
        }
        let _database = LocalDatabase::open(&path).expect("database migrates");
        assert!(path.with_extension("db.bak").exists());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn migration_12_to_16_is_idempotent_and_preserves_existing_rows() {
        let path = temp_database_path("feedback-migration");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            // Seed a pre-wave (user_version 12) database with an existing
            // memory_entries row, so we can confirm migrations 13 through 16 do not
            // touch unrelated data.
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE memory_entries (
                        id TEXT PRIMARY KEY NOT NULL,
                        scope_kind TEXT NOT NULL,
                        scope_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT NOT NULL,
                        provenance TEXT NOT NULL,
                        privacy TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        expires_at TEXT,
                        archived INTEGER NOT NULL,
                        forgotten INTEGER NOT NULL,
                        confirmations INTEGER NOT NULL DEFAULT 1,
                        lesson_key TEXT
                    );
                    INSERT INTO memory_entries
                        (id, scope_kind, scope_id, title, content, provenance, privacy,
                         created_at, expires_at, archived, forgotten)
                        VALUES ('m-1', 'project', 'p-1', 'Decision', 'keep this', 'run:1',
                                'internal', '2026-08-01T00:00:00Z', NULL, 0, 0);
                    PRAGMA user_version = 12;",
                )
                .expect("legacy schema and data write");
        }

        let database = LocalDatabase::open(&path).expect("database migrates forward");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        let feedback_table_exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='feedback_entries'",
                [],
                |row| row.get(0),
            )
            .expect("feedback table check");
        assert_eq!(feedback_table_exists, 1);
        let preserved: String = database
            .connection()
            .query_row(
                "SELECT content FROM memory_entries WHERE id = 'm-1'",
                [],
                |row| row.get(0),
            )
            .expect("existing memory row survives migration");
        assert_eq!(preserved, "keep this");
        drop(database);

        // Re-opening an already-migrated database must not error and must
        // not duplicate the feedback_entries table or existing rows
        // (guarded CREATE TABLE IF NOT EXISTS / PRAGMA user_version checks).
        let reopened = LocalDatabase::open(&path).expect("reopen is idempotent");
        assert_eq!(
            reopened.schema_version().expect("version stays current"),
            SCHEMA_VERSION
        );
        let row_count: i64 = reopened
            .connection()
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .expect("row count reads");
        assert_eq!(row_count, 1);
        drop(reopened);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn migration_16_maps_memory_v1_rows_onto_the_extraction_contract() {
        // Memory v1 -> Memory Extraction: явные failure lessons получают
        // kind=lesson, прочие старые факты -- kind=entity; все legacy rows
        // остаются активной памятью с legacy-версиями extractor/policy и
        // пустой цепочкой supersede.
        let path = temp_database_path("memory-extraction-migration");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE memory_entries (
                        id TEXT PRIMARY KEY NOT NULL,
                        scope_kind TEXT NOT NULL,
                        scope_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT NOT NULL,
                        provenance TEXT NOT NULL,
                        privacy TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        expires_at TEXT,
                        archived INTEGER NOT NULL,
                        forgotten INTEGER NOT NULL,
                        confirmations INTEGER NOT NULL DEFAULT 1,
                        lesson_key TEXT
                    );
                    INSERT INTO memory_entries
                        (id, scope_kind, scope_id, title, content, provenance, privacy,
                         created_at, expires_at, archived, forgotten, confirmations, lesson_key)
                        VALUES
                        ('fact-1', 'project', 'p-1', 'Решение', 'сборка через cargo', 'run:1',
                         'internal', '2026-08-01T00:00:00Z', NULL, 0, 0, 1, NULL),
                        ('lesson-1', 'project', 'p-1', 'Урок', 'проверяй аргументы', 'task:t-1',
                         'private', '2026-08-02T00:00:00Z', NULL, 0, 0, 3, 'lesson-key-1'),
                        ('gone-1', 'project', 'p-1', '', '', '', 'internal',
                         '2026-08-03T00:00:00Z', NULL, 0, 1, 1, NULL);
                    PRAGMA user_version = 12;",
                )
                .expect("legacy schema and data write");
        }

        let database = LocalDatabase::open(&path).expect("database migrates forward");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        // Транзакционность миграции подтверждается наличием backup рядом.
        assert!(path.with_extension("db.bak").exists());

        let mapped = |id: &str| -> (String, String, String, String, f64, f64) {
            database
                .connection()
                .query_row(
                    "SELECT kind, confirmation_state, extractor_version, policy_version,
                            model_confidence, verification_confidence
                     FROM memory_entries WHERE id = ?1",
                    rusqlite::params![id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .expect("mapped row reads")
        };
        assert_eq!(
            mapped("fact-1"),
            (
                "entity".to_owned(),
                "confirmed".to_owned(),
                "v1_legacy".to_owned(),
                "legacy-v1".to_owned(),
                1.0,
                1.0
            )
        );
        assert_eq!(mapped("lesson-1").0, "lesson");
        assert_eq!(mapped("lesson-1").1, "confirmed");
        // Уже забытая запись не воскресает в состоянии confirmed.
        assert_eq!(mapped("gone-1").1, "forgotten");

        let (supersedes, superseded_by): (Option<String>, Option<String>) = database
            .connection()
            .query_row(
                "SELECT supersedes, superseded_by FROM memory_entries WHERE id = 'fact-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("supersede columns read");
        assert_eq!(supersedes, None);
        assert_eq!(superseded_by, None);

        // Исходные statement и provenance сохранены дословно.
        let (content, provenance): (String, String) = database
            .connection()
            .query_row(
                "SELECT content, provenance FROM memory_entries WHERE id = 'fact-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("statement survives");
        assert_eq!(content, "сборка через cargo");
        assert_eq!(provenance, "run:1");

        // canonical_subject остаётся NULL: нормализатор версионируется в Core.
        let subject: Option<String> = database
            .connection()
            .query_row(
                "SELECT canonical_subject FROM memory_entries WHERE id = 'fact-1'",
                [],
                |row| row.get(0),
            )
            .expect("subject reads");
        assert_eq!(subject, None);

        for table in [
            "memory_aliases",
            "memory_tombstones",
            "memory_session_notes",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![table],
                    |row| row.get(0),
                )
                .expect("table check");
            assert_eq!(exists, 1, "{table} must exist after migration 16");
        }
        drop(database);

        // Повторное открытие не дублирует колонки и не меняет данные.
        let reopened = LocalDatabase::open(&path).expect("reopen is idempotent");
        let rows: i64 = reopened
            .connection()
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .expect("row count reads");
        assert_eq!(rows, 3);
        drop(reopened);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn restores_backup_when_migration_fails() {
        let path = temp_database_path("rollback");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch("CREATE TABLE marker(value TEXT NOT NULL); INSERT INTO marker VALUES ('legacy');")
                .expect("legacy data writes");
        }
        assert!(LocalDatabase::open_internal(&path, true).is_err());
        let database = LocalDatabase::open(&path).expect("database restores and migrates");
        let marker: String = database
            .connection
            .query_row("SELECT value FROM marker", [], |row| row.get(0))
            .expect("legacy marker survives rollback");
        assert_eq!(marker, "legacy");
        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn appends_and_replays_events_by_sequence() {
        let path = temp_database_path("events");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let first = database
            .append_event("task-1", "task.started", b"one")
            .expect("first event");
        let second = database
            .append_event("task-1", "task.completed", b"two")
            .expect("second event");
        let events = database.read_events_after(first, 10).expect("events read");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence_id, second);
        assert_eq!(events[0].payload, b"two");
        let task_events = database
            .read_task_events("task-1", 10)
            .expect("task events read");
        assert_eq!(task_events.len(), 2);
        assert_eq!(task_events[0].sequence_id, first);
        assert_eq!(task_events[1].sequence_id, second);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn exports_events_as_jsonl() {
        let path = temp_database_path("export");
        let output = path.with_extension("jsonl");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&output);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .append_event("task-export", "task.started", br#"{"ok":true}"#)
            .expect("event writes");
        database
            .export_events_jsonl(&output)
            .expect("export writes");
        let content = std::fs::read_to_string(&output).expect("export reads");
        let record: serde_json::Value = serde_json::from_str(content.trim()).expect("valid JSON");
        assert_eq!(record["task_id"], "task-export");
        assert_eq!(record["payload"]["ok"], true);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn diagnostics_summary_is_bounded_read_only_and_counts_tables_and_events() {
        let path = temp_database_path("diagnostics-summary");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("diagnostics-project", "Diagnostics", "C:\\workspace", None)
            .expect("project creates");
        database
            .append_event("task-1", "task.started", b"one")
            .expect("first event writes");
        database
            .append_event("task-1", "task.started", b"two")
            .expect("second event writes");
        database
            .append_event("task-1", "task.completed", b"three")
            .expect("third event writes");

        let before_version = database.schema_version().expect("schema version reads");
        let summary: DiagnosticsSummary = database
            .read_diagnostics_summary(1)
            .expect("diagnostics summary reads");

        assert_eq!(summary.total_events, 3);
        assert_eq!(summary.event_counts.len(), 1);
        assert_eq!(summary.event_counts[0].event_type, "task.started");
        assert_eq!(summary.event_counts[0].rows, 2);
        assert!(summary.event_types_truncated);
        assert_eq!(summary.table_counts.len(), 24);
        assert_eq!(
            summary
                .table_counts
                .iter()
                .find(|count| count.table == "projects")
                .expect("projects count exists")
                .rows,
            1
        );
        assert_eq!(
            summary
                .table_counts
                .iter()
                .find(|count| count.table == "events")
                .expect("events count exists")
                .rows,
            3
        );
        assert_eq!(
            database.schema_version().expect("schema version reads"),
            before_version
        );
        assert_eq!(
            database
                .read_events_after(0, 10)
                .expect("events remain readable")
                .len(),
            3
        );

        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn creates_and_updates_task_with_optimistic_version() {
        let path = temp_database_path("tasks");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-1", "Demo", "C:\\Projects\\demo", None)
            .expect("project creates");
        let item = WorkItemRecord {
            id: "work-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            title: "First task".into(),
            description: "desc".into(),
            source_ref: Some("prd:1".into()),
            acceptance_criteria: "tests pass".into(),
            non_goals: "no UI".into(),
            status: "backlog".into(),
            priority: 10,
            estimate: Some(2),
            complexity: Some("small".into()),
            attempt_count: 0,
            version: 1,
        };
        let created = database.create_work_item(&item).expect("task creates");
        let updated = database
            .update_work_item_status(&created.id, 1, "ready")
            .expect("task updates");
        assert_eq!(updated.status, "ready");
        assert_eq!(updated.version, 2);
        assert!(matches!(
            database.update_work_item_status(&created.id, 1, "done"),
            Err(StorageError::VersionConflict { .. })
        ));
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn lists_graph_rejects_cycles_and_selects_next_ready_deterministically() {
        let path = temp_database_path("task-graph");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-graph", "Graph", "C:\\Projects\\graph", None)
            .expect("project creates");
        for (id, title, status, priority) in [
            ("task-a", "A", "ready", 1),
            ("task-b", "B", "ready", 10),
            ("task-c", "C", "done", 100),
        ] {
            database
                .create_work_item(&WorkItemRecord {
                    id: id.into(),
                    project_id: "project-graph".into(),
                    parent_id: None,
                    title: title.into(),
                    description: String::new(),
                    source_ref: None,
                    acceptance_criteria: String::new(),
                    non_goals: String::new(),
                    status: status.into(),
                    priority,
                    estimate: None,
                    complexity: None,
                    attempt_count: 0,
                    version: 1,
                })
                .expect("task creates");
        }
        database
            .add_dependency("task-a", "task-c", "blocks")
            .expect("dependency creates");
        assert!(matches!(
            database.add_dependency("task-c", "task-a", "blocks"),
            Err(StorageError::DependencyCycle { .. })
        ));
        assert_eq!(database.list_work_items("project-graph").unwrap().len(), 3);
        assert_eq!(
            database.list_dependencies("project-graph").unwrap().len(),
            1
        );
        assert_eq!(
            database.next_ready("project-graph").unwrap().unwrap().id,
            "task-b"
        );
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn imports_prd_atomically_and_preserves_provenance() {
        let path = temp_database_path("prd-import");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-prd", "PRD", "C:\\Projects\\prd", None)
            .expect("project creates");
        let source = "# Plan\n\n## Imported\nDescription\n- [ ] Verify\n";
        let tasks = [ImportedTask {
            id: "import-task-1".into(),
            title: "Imported".into(),
            description: "Description".into(),
            source_ref: "prd.md#L3".into(),
            acceptance_criteria: "Verify".into(),
        }];
        let imported = database
            .import_prd("import-1", "project-prd", "prd.md", "v7", source, &tasks)
            .expect("PRD imports");
        assert_eq!(imported[0].status, "backlog");
        let provenance = database
            .get_provenance("import-1")
            .expect("provenance reads")
            .expect("provenance exists");
        assert_eq!(provenance.kind, "prd_import");
        assert_eq!(provenance.source, "prd.md");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&provenance.payload).unwrap()["version"],
            "v7"
        );
        assert!(database
            .import_prd("import-1", "project-prd", "prd.md", "v7", source, &tasks)
            .is_err());
        assert_eq!(database.list_work_items("project-prd").unwrap().len(), 1);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persists_run_linked_snapshot_payload_immutably() {
        let path = temp_database_path("snapshots");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let saved = database
            .save_snapshot("snapshot-1", "run-1", "workspace-hash", br#"{"files":[]}"#)
            .expect("snapshot saves");
        assert_eq!(saved.run_id, "run-1");
        assert_eq!(database.get_snapshot("snapshot-1").unwrap(), Some(saved));
        assert!(database
            .save_snapshot("snapshot-1", "run-2", "other", b"changed")
            .is_err());
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persists_project_policy_with_optimistic_versioning() {
        let path = temp_database_path("project-policy");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-policy", "Policy", ".", None)
            .expect("project creates");
        let first = database
            .upsert_project_policy("project-policy", br#"{"timeout_ms":30000}"#, None)
            .expect("policy creates");
        assert_eq!(first.version, 1);
        let second = database
            .upsert_project_policy("project-policy", br#"{"timeout_ms":15000}"#, Some(1))
            .expect("policy updates");
        assert_eq!(second.version, 2);
        assert!(matches!(
            database.upsert_project_policy("project-policy", b"{}", Some(1)),
            Err(StorageError::VersionConflict {
                entity: "project_policy",
                ..
            })
        ));
        assert_eq!(
            database
                .get_project_policy("project-policy")
                .unwrap()
                .unwrap(),
            second
        );
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn checkpoints_and_unknown_effects_recover_without_retry() {
        let path = temp_database_path("recovery");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-recovery", "Recovery", ".", None)
            .expect("project creates");
        let task = WorkItemRecord {
            id: "task-recovery".into(),
            project_id: "project-recovery".into(),
            parent_id: None,
            title: "recover me".into(),
            description: String::new(),
            source_ref: None,
            acceptance_criteria: String::new(),
            non_goals: String::new(),
            status: "in_progress".into(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        };
        database.create_work_item(&task).expect("task creates");
        let run = RunRecord {
            id: "run-recovery".into(),
            work_item_id: task.id.clone(),
            status: "running".into(),
            policy_snapshot: vec![],
            role_snapshot: vec![],
            skill_snapshot: vec![],
            model_route_snapshot: vec![],
        };
        let checkpoint = RunCheckpointRecord {
            run_id: run.id.clone(),
            checkpoint_id: "checkpoint-1".into(),
            stage: "build".into(),
            node_id: "node-1".into(),
            attempt: 1,
            input_hash: "input-hash".into(),
            state_json: br#"{"stage":"build"}"#.to_vec(),
            pending_effects_json: br#"["effect-1"]"#.to_vec(),
            committed_at: "2026-01-01T00:00:00Z".into(),
        };
        let effect = RunEffectRecord {
            effect_id: "effect-1".into(),
            run_id: run.id.clone(),
            node_id: "node-1".into(),
            kind: "bounded_build".into(),
            idempotency_key: "run-recovery:build".into(),
            immutable_intent_hash: "intent-hash".into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        database
            .prepare_run_effect(&run, &checkpoint, &effect)
            .expect("effect prepares");
        database
            .mark_effect_executing("effect-1")
            .expect("effect starts");
        drop(database);
        let database = LocalDatabase::open(&path).expect("database reopens after restart");
        let recovered = database.recover_unknown_effects().expect("recovery runs");
        assert_eq!(recovered.len(), 1);
        assert_eq!(
            database.get_run("run-recovery").unwrap().unwrap().status,
            "blocked"
        );
        assert_eq!(
            database
                .latest_checkpoint("run-recovery")
                .unwrap()
                .unwrap()
                .checkpoint_id,
            "checkpoint-1"
        );
        assert_eq!(
            database.read_task_events(&task.id, 10).unwrap()[0].event_type,
            "run.recovery.blocked"
        );
        assert!(
            database.recover_unknown_effects().unwrap().is_empty(),
            "recovery is idempotent"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn run_lease_is_single_owner_and_effect_can_be_reconciled() {
        let path = temp_database_path("leases");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-lease", "Lease", ".", None)
            .expect("project creates");
        database
            .create_work_item(&WorkItemRecord {
                id: "task-lease".into(),
                project_id: "project-lease".into(),
                parent_id: None,
                title: "lease".into(),
                description: String::new(),
                source_ref: None,
                acceptance_criteria: String::new(),
                non_goals: String::new(),
                status: "in_progress".into(),
                priority: 0,
                estimate: None,
                complexity: None,
                attempt_count: 0,
                version: 1,
            })
            .expect("task creates");
        let run = RunRecord {
            id: "run-lease".into(),
            work_item_id: "task-lease".into(),
            status: "running".into(),
            policy_snapshot: vec![],
            role_snapshot: vec![],
            skill_snapshot: vec![],
            model_route_snapshot: vec![],
        };
        let checkpoint = RunCheckpointRecord {
            run_id: run.id.clone(),
            checkpoint_id: "checkpoint-lease".into(),
            stage: "build".into(),
            node_id: "bounded-build".into(),
            attempt: 1,
            input_hash: "intent".into(),
            state_json: b"{}".to_vec(),
            pending_effects_json: br#"["effect-lease"]"#.to_vec(),
            committed_at: String::new(),
        };
        let effect = RunEffectRecord {
            effect_id: "effect-lease".into(),
            run_id: run.id.clone(),
            node_id: "bounded-build".into(),
            kind: "bounded_build".into(),
            idempotency_key: "lease-key".into(),
            immutable_intent_hash: "intent".into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        database
            .prepare_run_effect(&run, &checkpoint, &effect)
            .expect("effect prepares");
        database
            .acquire_run_lease("run-lease", "lease-1", "core-a", 1, 30)
            .expect("first owner claims");
        assert!(matches!(
            database.acquire_run_lease("run-lease", "lease-2", "core-b", 2, 30),
            Err(StorageError::InvalidRunEffect(_))
        ));
        database
            .heartbeat_run_lease("run-lease", "lease-1", "core-a", 1, 30)
            .expect("owner heartbeats");
        database
            .mark_effect_executing("effect-lease")
            .expect("effect executes");
        database
            .recover_unknown_effects()
            .expect("unknown effect recovers");
        let reconciliation = database
            .reconcile_run_effect(
                "effect-lease",
                true,
                "snapshot",
                br#"{"snapshot_id":"snapshot-1"}"#,
            )
            .expect("effect reconciles");
        assert_eq!(reconciliation.state, "reconciled_success");
        assert_eq!(
            database
                .get_run_effect("effect-lease")
                .unwrap()
                .unwrap()
                .state,
            "completed_success"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_run_effect_has_an_independent_lease_and_completes() {
        let path = temp_database_path("agent-run-lease");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let effect = RunEffectRecord {
            effect_id: "agent-effect-1".into(),
            run_id: "agent-run-1".into(),
            node_id: "agent-task".into(),
            kind: "agent_task".into(),
            idempotency_key: "agent-run-1:agent-task".into(),
            immutable_intent_hash: "intent-agent".into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        database
            .prepare_agent_run_effect(&effect, "shell-task-1")
            .expect("agent effect prepares");
        database
            .acquire_agent_run_lease("agent-run-1", "agent-lease-1", "core", 1, 30)
            .expect("agent lease claims");
        database
            .heartbeat_agent_run_lease("agent-run-1", "agent-lease-1", "core", 1, 30)
            .expect("agent lease heartbeats");
        database
            .mark_agent_effect_executing("agent-effect-1")
            .expect("agent effect executes");
        let completed = database
            .complete_agent_run_effect("agent-effect-1", true, Some("result"))
            .expect("agent effect completes");
        assert_eq!(completed.state, "completed_success");
        database
            .release_agent_run_lease("agent-run-1", "agent-lease-1", "core", 1)
            .expect("agent lease releases");
        assert!(database
            .get_agent_run_lease("agent-run-1")
            .unwrap()
            .is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn deduplicates_same_request_and_rejects_reused_request_id() {
        let path = temp_database_path("dedup");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        assert_eq!(
            database
                .record_deduplicated("client", "request", "hash", b"ok")
                .expect("first write"),
            None
        );
        assert_eq!(
            database
                .record_deduplicated("client", "request", "hash", b"different")
                .expect("replay"),
            Some(b"ok".to_vec())
        );
        assert!(matches!(
            database.record_deduplicated("client", "request", "other", b"bad"),
            Err(StorageError::DeduplicationConflict { .. })
        ));
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persists_immutable_run_snapshots() {
        let path = temp_database_path("run-snapshots");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-run", "Run project", "C:\\Projects\\run", None)
            .expect("project creates");
        database
            .create_work_item(&WorkItemRecord {
                id: "task-run".into(),
                project_id: "project-run".into(),
                parent_id: None,
                title: "Run task".into(),
                description: String::new(),
                source_ref: None,
                acceptance_criteria: String::new(),
                non_goals: String::new(),
                status: "ready".into(),
                priority: 0,
                estimate: None,
                complexity: None,
                attempt_count: 0,
                version: 1,
            })
            .expect("task creates");
        let run = RunRecord {
            id: "run-1".into(),
            work_item_id: "task-run".into(),
            status: "queued".into(),
            policy_snapshot: br#"{"max_iterations":1}"#.to_vec(),
            role_snapshot: br#"{"id":"planner","version":1}"#.to_vec(),
            skill_snapshot: br#"{"id":"native","version":1}"#.to_vec(),
            model_route_snapshot: br#"{"route":"local-first"}"#.to_vec(),
        };
        assert_eq!(database.create_run(&run).expect("run creates"), run);
        assert!(
            database.create_run(&run).is_err(),
            "run snapshot is immutable"
        );
        assert_eq!(database.get_run("run-1").expect("run reads"), Some(run));
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn round_trips_typed_snapshot_contracts() {
        let path = temp_database_path("typed-snapshots");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-typed", "Typed", "C:\\Projects\\typed", None)
            .expect("project creates");
        database
            .create_work_item(&WorkItemRecord {
                id: "task-typed".into(),
                project_id: "project-typed".into(),
                parent_id: None,
                title: "Typed task".into(),
                description: String::new(),
                source_ref: None,
                acceptance_criteria: String::new(),
                non_goals: String::new(),
                status: "ready".into(),
                priority: 0,
                estimate: None,
                complexity: None,
                attempt_count: 0,
                version: 1,
            })
            .expect("task creates");
        let snapshots = RunSnapshots {
            role_ref: RoleRef {
                id: "planner".into(),
                version: "1".into(),
                hash: "role-hash".into(),
            },
            skill_ref: SkillRef {
                id: "native".into(),
                version: "2".into(),
                hash: "skill-hash".into(),
            },
            policy: PolicySnapshot {
                schema_version: 1,
                policy_version: 3,
                effective_permissions_hash: "permissions-hash".into(),
                canonical_json: br#"{"tools":["filesystem.read"]}"#.to_vec(),
            },
            model_route: ModelRouteSnapshot {
                requested_route: "local-first".into(),
                resolved_provider: "mock".into(),
                resolved_model: "test-model".into(),
                route_policy_version: 1,
                canonical_json: br#"{"route":"local-first"}"#.to_vec(),
            },
        };
        database
            .create_run_with_snapshots("run-typed", "task-typed", "queued", &snapshots)
            .expect("typed run creates");
        assert_eq!(
            database
                .get_run_snapshots("run-typed")
                .expect("typed run reads"),
            Some(snapshots)
        );
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recovery_transitions_are_durable_and_audited_without_retry() {
        let path = temp_database_path("recovery-state-machine");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-state", "State", ".", None)
            .expect("project creates");
        database
            .create_work_item(&WorkItemRecord {
                id: "task-state".into(),
                project_id: "project-state".into(),
                parent_id: None,
                title: "State task".into(),
                description: String::new(),
                source_ref: None,
                acceptance_criteria: String::new(),
                non_goals: String::new(),
                status: "ready".into(),
                priority: 0,
                estimate: None,
                complexity: None,
                attempt_count: 0,
                version: 1,
            })
            .expect("task creates");
        database
            .create_run(&RunRecord {
                id: "run-state".into(),
                work_item_id: "task-state".into(),
                status: "running".into(),
                policy_snapshot: Vec::new(),
                role_snapshot: Vec::new(),
                skill_snapshot: Vec::new(),
                model_route_snapshot: Vec::new(),
            })
            .expect("run creates");

        database
            .transition_recovery(
                "run-state",
                RecoveryState::Recovering,
                "effect-state",
                "run-state:effect-state",
                "startup",
                br#"{"reason":"process_restart"}"#,
                "recovery_started",
            )
            .expect("recovering transition");
        database
            .transition_recovery(
                "run-state",
                RecoveryState::Reconciling,
                "effect-state",
                "run-state:effect-state:reconciling",
                "file_hash",
                br#"{"path":"src/lib.rs"}"#,
                "verifier_started",
            )
            .expect("reconciling transition");
        let blocked = database
            .transition_recovery(
                "run-state",
                RecoveryState::Blocked,
                "effect-state",
                "run-state:effect-state:blocked",
                "file_hash",
                br#"{"match":false}"#,
                "outcome_unconfirmed",
            )
            .expect("blocked transition");
        assert_eq!(blocked.state, RecoveryState::Blocked);
        assert_eq!(
            database.latest_recovery("run-state").expect("latest reads"),
            Some(blocked)
        );
        let repeated = database
            .transition_recovery(
                "run-state",
                RecoveryState::Blocked,
                "effect-state",
                "run-state:effect-state:blocked",
                "file_hash",
                br#"{"match":false}"#,
                "outcome_unconfirmed",
            )
            .expect("repeated decision is idempotent");
        assert_eq!(
            repeated.id,
            database
                .latest_recovery("run-state")
                .expect("latest reads")
                .expect("record exists")
                .id
        );
        assert!(matches!(
            database.transition_recovery(
                "run-state",
                RecoveryState::Resumable,
                "effect-state",
                "run-state:effect-state:blind-retry",
                "file_hash",
                br#"{}"#,
                "blind_retry"
            ),
            Err(StorageError::InvalidRecovery(_))
        ));
        let events = database.read_events_after(0, 10).expect("events read");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == "run.recovery.decision")
                .count(),
            3
        );
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_12_is_idempotent_and_preserves_pre_existing_memory_rows() {
        // Reproduces the pre-wave-VI state: a v8 `memory_entries` table
        // (no `confirmations` / `lesson_key`) with one real row already in
        // it, then confirms the 11 -> 12 migration both preserves that row
        // and can be re-applied (guarded re-open) without altering the
        // already-migrated columns a second time.
        let path = temp_database_path("migration-12-idempotent");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE memory_entries (
                        id TEXT PRIMARY KEY NOT NULL,
                        scope_kind TEXT NOT NULL,
                        scope_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT NOT NULL,
                        provenance TEXT NOT NULL,
                        privacy TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        expires_at TEXT,
                        archived INTEGER NOT NULL,
                        forgotten INTEGER NOT NULL
                    );
                    CREATE INDEX IF NOT EXISTS idx_memory_entries_scope
                        ON memory_entries(scope_kind, scope_id);
                    INSERT INTO memory_entries
                        (id, scope_kind, scope_id, title, content, provenance, privacy,
                         created_at, expires_at, archived, forgotten)
                    VALUES
                        ('pre-existing', 'project', 'scope-a', 'Old title', 'Old content',
                         'task:pre-wave-vi', 'internal', '2026-01-01T00:00:00Z', NULL, 0, 0);
                    PRAGMA user_version = 11;",
                )
                .expect("v8-shaped legacy memory table seeds");
        }

        let database = LocalDatabase::open(&path).expect("database migrates 11 -> 12");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        let (confirmations, lesson_key): (i64, Option<String>) = database
            .connection()
            .query_row(
                "SELECT confirmations, lesson_key FROM memory_entries WHERE id = 'pre-existing'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("pre-existing row survives migration with new columns defaulted");
        assert_eq!(confirmations, 1, "DEFAULT 1 applied to pre-existing rows");
        assert_eq!(lesson_key, None);
        let title: String = database
            .connection()
            .query_row(
                "SELECT title FROM memory_entries WHERE id = 'pre-existing'",
                [],
                |row| row.get(0),
            )
            .expect("original content untouched by migration");
        assert_eq!(title, "Old title");
        drop(database);

        // Re-opening an already-migrated database must not re-run the
        // ALTER TABLE (which would error on a duplicate column) and must
        // not disturb existing data.
        let database = LocalDatabase::open(&path).expect("re-open is idempotent");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        let confirmations_after_reopen: i64 = database
            .connection()
            .query_row(
                "SELECT confirmations FROM memory_entries WHERE id = 'pre-existing'",
                [],
                |row| row.get(0),
            )
            .expect("row still present after idempotent re-open");
        assert_eq!(confirmations_after_reopen, 1);
        let row_count: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .expect("count reads");
        assert_eq!(row_count, 1, "no duplicate rows created by re-migration");

        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn research_and_memory_stores_round_trip_against_shared_migrated_database() {
        use crate::memory_store::{MemoryPrivacy, MemoryRecord, MemoryScope, MemoryStoreSql};
        use crate::research_store::{ResearchEvidenceRecord, ResearchEvidenceSql};

        let path = temp_database_path("bounded-stores");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );

        let evidence = ResearchEvidenceRecord {
            id: "evidence-1".into(),
            source_kind: "url".into(),
            source_ref: "https://example.test/source".into(),
            redacted_excerpt: "redacted result".into(),
            source_hash: "sha256:abc".into(),
            fetched_at: "2026-08-12T10:00:00Z".into(),
            ttl_seconds: 3600,
            provenance_link: Some("run:shared-db".into()),
        };
        ResearchEvidenceSql::insert(database.connection(), &evidence)
            .expect("evidence inserts against shared connection");
        assert_eq!(
            ResearchEvidenceSql::get_by_id(database.connection(), "evidence-1")
                .expect("evidence reads"),
            Some(evidence)
        );
        assert_eq!(
            ResearchEvidenceSql::list_by_provenance(database.connection(), "run:shared-db")
                .expect("evidence lists")
                .len(),
            1
        );

        let memory = MemoryRecord::new(
            "memory-1",
            MemoryScope::Project,
            "project-shared-db",
            "Decision",
            "keep this fact",
            "run:shared-db",
            MemoryPrivacy::Internal,
            "2026-08-12T10:00:00Z",
            Some("2027-01-01T00:00:00Z".into()),
        )
        .expect("memory record builds");
        MemoryStoreSql::insert(database.connection(), &memory)
            .expect("memory inserts against shared connection");
        assert_eq!(
            MemoryStoreSql::get_by_id(database.connection(), "memory-1").expect("memory reads"),
            Some(memory)
        );
        let found = MemoryStoreSql::search(
            database.connection(),
            MemoryScope::Project,
            "project-shared-db",
            "fact",
            "2026-09-01T00:00:00Z",
            10,
        )
        .expect("memory search");
        assert_eq!(found.len(), 1);
        assert!(MemoryStoreSql::archive(database.connection(), "memory-1").expect("archive"));
        assert!(MemoryStoreSql::forget(database.connection(), "memory-1").expect("forget"));

        drop(database);
        let _ = std::fs::remove_file(&path);
    }
}
