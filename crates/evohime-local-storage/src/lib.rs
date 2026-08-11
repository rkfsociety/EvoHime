use std::{
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 4;

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
pub struct ProjectRecord {
    pub id: String,
    pub title: String,
    pub workspace_path: String,
    pub source_ref: Option<String>,
    pub version: i64,
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
        Ok(Self { path, connection })
    }

    pub fn path(&self) -> &Path {
        &self.path
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
        self.get_snapshot(id)?.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
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

    pub fn latest_snapshot_for_task(&self, task_id: &str) -> Result<Option<SnapshotRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT s.id, s.run_id, s.workspace_hash, s.payload, s.created_at
             FROM snapshots s JOIN runs r ON r.id = s.run_id
             WHERE r.work_item_id = ?1 ORDER BY s.created_at DESC, s.id DESC LIMIT 1",
        )?;
        Ok(statement
            .query_row([task_id], |row| {
                Ok(SnapshotRecord {
                    id: row.get(0)?, run_id: row.get(1)?, workspace_hash: row.get(2)?,
                    payload: row.get(3)?, created_at: row.get(4)?,
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
        self.get_run(&run.id)?.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
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
        self.get_run(&run.id)?.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn create_checkpoint(&self, checkpoint: &RunCheckpointRecord) -> Result<RunCheckpointRecord, StorageError> {
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

    pub fn latest_checkpoint(&self, run_id: &str) -> Result<Option<RunCheckpointRecord>, StorageError> {
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
                run.id, run.work_item_id, run.status, run.policy_snapshot, run.role_snapshot,
                run.skill_snapshot, run.model_route_snapshot
            ],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO run_checkpoints(run_id, checkpoint_id, stage, node_id, attempt,
             input_hash, state_json, pending_effects_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                checkpoint.run_id, checkpoint.checkpoint_id, checkpoint.stage, checkpoint.node_id,
                checkpoint.attempt, checkpoint.input_hash, checkpoint.state_json, checkpoint.pending_effects_json
            ],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO run_effects(effect_id, run_id, node_id, kind, idempotency_key,
             immutable_intent_hash, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                effect.effect_id, effect.run_id, effect.node_id, effect.kind, effect.idempotency_key,
                effect.immutable_intent_hash, effect.state
            ],
        )?;
        transaction.commit()?;
        self.get_run_effect(&effect.effect_id)?.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get_run_effect(&self, effect_id: &str) -> Result<Option<RunEffectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT effect_id, run_id, node_id, kind, idempotency_key, immutable_intent_hash,
             state, started_at, completed_at, result_hash FROM run_effects WHERE effect_id = ?1",
        )?;
        Ok(statement
            .query_row([effect_id], |row| {
                Ok(RunEffectRecord {
                    effect_id: row.get(0)?, run_id: row.get(1)?, node_id: row.get(2)?,
                    kind: row.get(3)?, idempotency_key: row.get(4)?, immutable_intent_hash: row.get(5)?,
                    state: row.get(6)?, started_at: row.get(7)?, completed_at: row.get(8)?, result_hash: row.get(9)?,
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
        self.get_run_effect(effect_id)?.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn complete_run_effect(&self, effect_id: &str, success: bool, result_hash: Option<&str>) -> Result<RunEffectRecord, StorageError> {
        let state = if success { "completed_success" } else { "completed_failure" };
        self.connection.execute(
            "UPDATE run_effects SET state = ?1, completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), result_hash = ?2
             WHERE effect_id = ?3 AND state = 'executing'",
            rusqlite::params![state, result_hash, effect_id],
        )?;
        self.get_run_effect(effect_id)?.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn update_run_status(&self, run_id: &str, status: &str) -> Result<(), StorageError> {
        self.connection.execute("UPDATE runs SET status = ?1 WHERE id = ?2", rusqlite::params![status, run_id])?;
        Ok(())
    }

    pub fn recover_unknown_effects(&self) -> Result<Vec<RecoveredRunRecord>, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut statement = transaction.prepare(
            "SELECT e.run_id, r.work_item_id, e.effect_id FROM run_effects e
             JOIN runs r ON r.id = e.run_id WHERE e.state = 'executing'",
        )?;
        let records = statement
            .query_map([], |row| {
                Ok(RecoveredRunRecord { run_id: row.get(0)?, work_item_id: row.get(1)?, effect_id: row.get(2)? })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for record in &records {
            transaction.execute(
                "UPDATE run_effects SET state = 'unknown', completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE effect_id = ?1 AND state = 'executing'",
                [&record.effect_id],
            )?;
            transaction.execute(
                "UPDATE runs SET status = 'blocked' WHERE id = ?1 AND status = 'running'",
                [&record.run_id],
            )?;
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
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ImportedTask, LocalDatabase, ModelRouteSnapshot, PolicySnapshot, RoleRef, RunCheckpointRecord,
        RunEffectRecord, RunRecord, RunSnapshots, SkillRef, StorageError, WorkItemRecord,
        SCHEMA_VERSION,
    };
    use std::path::PathBuf;

    fn temp_database_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("evohime-test-{name}-{}.db", std::process::id()))
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
        drop(database);
        let _ = std::fs::remove_file(&path);
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
        let task_events = database.read_task_events("task-1", 10).expect("task events read");
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
        assert_eq!(database.list_dependencies("project-graph").unwrap().len(), 1);
        assert_eq!(database.next_ready("project-graph").unwrap().unwrap().id, "task-b");
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
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&provenance.payload).unwrap()["version"], "v7");
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
    fn checkpoints_and_unknown_effects_recover_without_retry() {
        let path = temp_database_path("recovery");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-recovery", "Recovery", ".", None)
            .expect("project creates");
        let task = WorkItemRecord {
            id: "task-recovery".into(), project_id: "project-recovery".into(), parent_id: None,
            title: "recover me".into(), description: String::new(), source_ref: None,
            acceptance_criteria: String::new(), non_goals: String::new(), status: "in_progress".into(),
            priority: 0, estimate: None, complexity: None, attempt_count: 0, version: 1,
        };
        database.create_work_item(&task).expect("task creates");
        let run = RunRecord {
            id: "run-recovery".into(), work_item_id: task.id.clone(), status: "running".into(),
            policy_snapshot: vec![], role_snapshot: vec![], skill_snapshot: vec![], model_route_snapshot: vec![],
        };
        let checkpoint = RunCheckpointRecord {
            run_id: run.id.clone(), checkpoint_id: "checkpoint-1".into(), stage: "build".into(),
            node_id: "node-1".into(), attempt: 1, input_hash: "input-hash".into(),
            state_json: br#"{"stage":"build"}"#.to_vec(), pending_effects_json: br#"["effect-1"]"#.to_vec(),
            committed_at: "2026-01-01T00:00:00Z".into(),
        };
        let effect = RunEffectRecord {
            effect_id: "effect-1".into(), run_id: run.id.clone(), node_id: "node-1".into(),
            kind: "bounded_build".into(), idempotency_key: "run-recovery:build".into(),
            immutable_intent_hash: "intent-hash".into(), state: "prepared".into(),
            started_at: None, completed_at: None, result_hash: None,
        };
        database.prepare_run_effect(&run, &checkpoint, &effect).expect("effect prepares");
        database.mark_effect_executing("effect-1").expect("effect starts");
        drop(database);
        let database = LocalDatabase::open(&path).expect("database reopens after restart");
        let recovered = database.recover_unknown_effects().expect("recovery runs");
        assert_eq!(recovered.len(), 1);
        assert_eq!(database.get_run("run-recovery").unwrap().unwrap().status, "blocked");
        assert_eq!(database.latest_checkpoint("run-recovery").unwrap().unwrap().checkpoint_id, "checkpoint-1");
        assert_eq!(database.read_task_events(&task.id, 10).unwrap()[0].event_type, "run.recovery.blocked");
        assert!(database.recover_unknown_effects().unwrap().is_empty(), "recovery is idempotent");
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
        assert!(database.create_run(&run).is_err(), "run snapshot is immutable");
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
            role_ref: RoleRef { id: "planner".into(), version: "1".into(), hash: "role-hash".into() },
            skill_ref: SkillRef { id: "native".into(), version: "2".into(), hash: "skill-hash".into() },
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
        assert_eq!(database.get_run_snapshots("run-typed").expect("typed run reads"), Some(snapshots));
        drop(database);
        let _ = std::fs::remove_file(path);
    }
}
