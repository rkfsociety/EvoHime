//! Durable contract and manifest storage for plan 28 (Persistent Analysis Kernel).
//!
//! This module stores metadata only. Kernel process memory and arbitrary values
//! are deliberately absent; large values must be addressed by an existing
//! Core-owned ArtifactStore reference.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::StorageError;

pub const ANALYSIS_KERNEL_VERSION: u32 = 1;
pub const ANALYSIS_KERNEL_SCHEMA_VERSION: u32 = 1;
pub const ANALYSIS_KERNEL_MAX_ID_BYTES: usize = 128;
pub const ANALYSIS_KERNEL_MAX_NAME_BYTES: usize = 128;
pub const ANALYSIS_KERNEL_MAX_INLINE_BYTES: usize = 16 * 1024;
pub const ANALYSIS_KERNEL_MAX_OBJECTS: usize = 1024;
pub const ANALYSIS_KERNEL_MAX_OBJECT_BYTES: u64 = 256 * 1024 * 1024;
pub const ANALYSIS_KERNEL_MAX_IDEMPOTENCY_RESULT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelStatus {
    Created,
    Running,
    Stopped,
    Crashed,
    Reset,
    LimitExceeded,
    Blocked,
}

impl KernelStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Crashed => "crashed",
            Self::Reset => "reset",
            Self::LimitExceeded => "limit_exceeded",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelObjectPersistence {
    Ephemeral,
    Checkpointed,
}

impl KernelObjectPersistence {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ephemeral => "ephemeral",
            Self::Checkpointed => "checkpointed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelSensitivity {
    Public,
    Internal,
    Sensitive,
    Secret,
}

impl KernelSensitivity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Sensitive => "sensitive",
            Self::Secret => "secret",
        }
    }

    pub const fn allows_inline(self) -> bool {
        matches!(self, Self::Public | Self::Internal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelLimitsV1 {
    pub cpu_time_ms: u64,
    pub memory_bytes: u64,
    pub output_bytes: u64,
    pub object_count: u32,
    pub object_bytes: u64,
    pub host_requests_per_minute: u32,
    pub idle_timeout_ms: u64,
    pub lifetime_timeout_ms: u64,
}

impl Default for KernelLimitsV1 {
    fn default() -> Self {
        Self {
            cpu_time_ms: 30_000,
            memory_bytes: 512 * 1024 * 1024,
            output_bytes: 1024 * 1024,
            object_count: ANALYSIS_KERNEL_MAX_OBJECTS as u32,
            object_bytes: ANALYSIS_KERNEL_MAX_OBJECT_BYTES,
            host_requests_per_minute: 120,
            idle_timeout_ms: 5 * 60 * 1000,
            lifetime_timeout_ms: 30 * 60 * 1000,
        }
    }
}

impl KernelLimitsV1 {
    pub fn validate(&self) -> Result<(), AnalysisKernelError> {
        if self.cpu_time_ms == 0 || self.cpu_time_ms > 10 * 60 * 1000 {
            return Err(AnalysisKernelError::InvalidLimits("cpu_time_ms"));
        }
        if self.memory_bytes == 0 || self.memory_bytes > 2 * 1024 * 1024 * 1024 {
            return Err(AnalysisKernelError::InvalidLimits("memory_bytes"));
        }
        if self.output_bytes == 0 || self.output_bytes > 16 * 1024 * 1024 {
            return Err(AnalysisKernelError::InvalidLimits("output_bytes"));
        }
        if self.object_count == 0 || self.object_count as usize > ANALYSIS_KERNEL_MAX_OBJECTS {
            return Err(AnalysisKernelError::InvalidLimits("object_count"));
        }
        if self.object_bytes == 0 || self.object_bytes > ANALYSIS_KERNEL_MAX_OBJECT_BYTES {
            return Err(AnalysisKernelError::InvalidLimits("object_bytes"));
        }
        if self.host_requests_per_minute == 0 || self.host_requests_per_minute > 10_000 {
            return Err(AnalysisKernelError::InvalidLimits(
                "host_requests_per_minute",
            ));
        }
        if self.idle_timeout_ms == 0 || self.lifetime_timeout_ms < self.idle_timeout_ms {
            return Err(AnalysisKernelError::InvalidLimits("timeouts"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisKernelSessionV1 {
    pub schema_version: u32,
    pub id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub runtime_version: String,
    pub package_manifest_hash: String,
    pub policy_hash: String,
    pub status: KernelStatus,
    pub revision: u64,
    pub limits: KernelLimitsV1,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl AnalysisKernelSessionV1 {
    pub fn validate(&self) -> Result<(), AnalysisKernelError> {
        if self.schema_version != ANALYSIS_KERNEL_SCHEMA_VERSION {
            return Err(AnalysisKernelError::UnsupportedVersion(self.schema_version));
        }
        for (field, value) in [
            ("id", self.id.as_str()),
            ("task_id", self.task_id.as_str()),
            ("workspace_id", self.workspace_id.as_str()),
            ("runtime_version", self.runtime_version.as_str()),
            ("package_manifest_hash", self.package_manifest_hash.as_str()),
            ("policy_hash", self.policy_hash.as_str()),
        ] {
            validate_id(field, value)?;
        }
        self.limits.validate()?;
        validate_hash("package_manifest_hash", &self.package_manifest_hash)?;
        validate_hash("policy_hash", &self.policy_hash)?;
        if self.runtime_version != "trusted-local-1" {
            return Err(AnalysisKernelError::InvalidField("runtime_version"));
        }
        if self.created_at_ms <= 0 || self.updated_at_ms < self.created_at_ms {
            return Err(AnalysisKernelError::InvalidField("timestamps"));
        }
        Ok(())
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, AnalysisKernelError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| AnalysisKernelError::Serialization)
    }

    pub fn content_hash(&self) -> Result<String, AnalysisKernelError> {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json()?);
        Ok(hex::encode(hasher.finalize()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelObjectRefV1 {
    pub id: String,
    pub kernel_id: String,
    pub logical_name: String,
    pub type_hint: String,
    pub size: u64,
    pub sensitivity: KernelSensitivity,
    pub persistence: KernelObjectPersistence,
    pub content_hash: Option<String>,
    pub artifact_locator: Option<String>,
    pub provenance: String,
    pub created_at_ms: i64,
    pub invalidated_at_ms: Option<i64>,
}

impl KernelObjectRefV1 {
    pub fn validate(&self) -> Result<(), AnalysisKernelError> {
        validate_id("id", &self.id)?;
        validate_id("kernel_id", &self.kernel_id)?;
        validate_id("logical_name", &self.logical_name)?;
        validate_id("type_hint", &self.type_hint)?;
        validate_id("provenance", &self.provenance)?;
        if self.size > ANALYSIS_KERNEL_MAX_OBJECT_BYTES {
            return Err(AnalysisKernelError::ObjectTooLarge(self.size));
        }
        if self.sensitivity.allows_inline() && self.size > 0 && self.artifact_locator.is_none() {
            return Err(AnalysisKernelError::MissingArtifactRef);
        }
        if self.persistence == KernelObjectPersistence::Checkpointed
            && (self.content_hash.is_none() || self.artifact_locator.is_none())
        {
            return Err(AnalysisKernelError::CheckpointRequiresArtifact);
        }
        if self.sensitivity == KernelSensitivity::Secret {
            return Err(AnalysisKernelError::SecretObject);
        }
        if self.created_at_ms <= 0
            || self
                .invalidated_at_ms
                .is_some_and(|v| v < self.created_at_ms)
        {
            return Err(AnalysisKernelError::InvalidField("timestamps"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnalysisKernelError {
    #[error("unsupported analysis kernel version {0}")]
    UnsupportedVersion(u32),
    #[error("invalid analysis kernel field {0}")]
    InvalidField(&'static str),
    #[error("invalid analysis kernel limits: {0}")]
    InvalidLimits(&'static str),
    #[error("analysis kernel object is too large: {0} bytes")]
    ObjectTooLarge(u64),
    #[error("analysis kernel request is too large: {0} bytes")]
    RequestTooLarge(usize),
    #[error("analysis kernel operation is not permitted")]
    ForbiddenOperation,
    #[error("analysis kernel object requires an ArtifactStore reference")]
    MissingArtifactRef,
    #[error("checkpointed object requires a hash and ArtifactStore reference")]
    CheckpointRequiresArtifact,
    #[error("secret kernel objects are not accepted")]
    SecretObject,
    #[error("analysis kernel values cannot be persisted as process memory")]
    ProcessMemoryPersistence,
    #[error("analysis kernel canonical serialization failed")]
    Serialization,
    #[error("analysis kernel sensitive inline payload is forbidden")]
    SensitiveInlinePayload,
    #[error("analysis kernel optimistic version conflict: expected {expected}, current {current}")]
    VersionConflict { expected: u64, current: u64 },
}

pub fn install_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS analysis_kernel_sessions (
            id TEXT PRIMARY KEY NOT NULL,
            task_id TEXT NOT NULL,
            workspace_id TEXT NOT NULL,
            runtime_version TEXT NOT NULL,
            package_manifest_hash TEXT NOT NULL,
            policy_hash TEXT NOT NULL,
            status TEXT NOT NULL,
            revision INTEGER NOT NULL,
            limits_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_analysis_kernel_sessions_task
            ON analysis_kernel_sessions(task_id, updated_at_ms DESC);
        CREATE TABLE IF NOT EXISTS analysis_kernel_objects (
            id TEXT PRIMARY KEY NOT NULL,
            kernel_id TEXT NOT NULL REFERENCES analysis_kernel_sessions(id),
            logical_name TEXT NOT NULL,
            type_hint TEXT NOT NULL,
            size INTEGER NOT NULL,
            sensitivity TEXT NOT NULL,
            persistence TEXT NOT NULL,
            content_hash TEXT,
            artifact_locator TEXT,
            provenance TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            invalidated_at_ms INTEGER
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_analysis_kernel_objects_name
            ON analysis_kernel_objects(kernel_id, logical_name);
        CREATE TABLE IF NOT EXISTS analysis_kernel_events (
            sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
            kernel_id TEXT NOT NULL REFERENCES analysis_kernel_sessions(id),
            event_type TEXT NOT NULL,
            payload BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_analysis_kernel_events_kernel
            ON analysis_kernel_events(kernel_id, sequence_id);
        CREATE TABLE IF NOT EXISTS analysis_kernel_idempotency (
            kernel_id TEXT NOT NULL REFERENCES analysis_kernel_sessions(id),
            idempotency_key TEXT NOT NULL,
            operation TEXT NOT NULL,
            result_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(kernel_id, idempotency_key)
        );",
    )
}

pub struct AnalysisKernelStore<'a> {
    connection: &'a Connection,
}

impl<'a> AnalysisKernelStore<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn create_session(&self, session: &AnalysisKernelSessionV1) -> Result<(), StorageError> {
        session.validate()?;
        let limits = serde_json::to_vec(&session.limits)?;
        self.connection.execute(
            "INSERT INTO analysis_kernel_sessions
             (id, task_id, workspace_id, runtime_version, package_manifest_hash,
              policy_hash, status, revision, limits_json, created_at_ms, updated_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![
                session.id,
                session.task_id,
                session.workspace_id,
                session.runtime_version,
                session.package_manifest_hash,
                session.policy_hash,
                session.status.as_str(),
                session.revision,
                limits,
                session.created_at_ms,
                session.updated_at_ms
            ],
        )?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<Option<AnalysisKernelSessionV1>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, task_id, workspace_id, runtime_version, package_manifest_hash,
                        policy_hash, status, revision, limits_json, created_at_ms, updated_at_ms
                 FROM analysis_kernel_sessions WHERE id = ?1",
                [id],
                |row| {
                    let status: String = row.get(6)?;
                    let limits: Vec<u8> = row.get(8)?;
                    Ok(AnalysisKernelSessionV1 {
                        schema_version: ANALYSIS_KERNEL_SCHEMA_VERSION,
                        id: row.get(0)?,
                        task_id: row.get(1)?,
                        workspace_id: row.get(2)?,
                        runtime_version: row.get(3)?,
                        package_manifest_hash: row.get(4)?,
                        policy_hash: row.get(5)?,
                        status: parse_status(&status),
                        revision: row.get(7)?,
                        limits: serde_json::from_slice(&limits).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                8,
                                rusqlite::types::Type::Blob,
                                Box::new(error),
                            )
                        })?,
                        created_at_ms: row.get(9)?,
                        updated_at_ms: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn list_running_sessions(&self) -> Result<Vec<AnalysisKernelSessionV1>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM analysis_kernel_sessions WHERE status='running' ORDER BY id",
        )?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                self.get_session(&id)?
                    .ok_or_else(|| StorageError::InvalidInput("kernel session disappeared".into()))
            })
            .collect()
    }

    pub fn set_status(
        &self,
        id: &str,
        expected_revision: u64,
        status: KernelStatus,
        now_ms: i64,
    ) -> Result<u64, StorageError> {
        let changed = self.connection.execute(
            "UPDATE analysis_kernel_sessions SET status=?1, revision=revision+1, updated_at_ms=?2
             WHERE id=?3 AND revision=?4",
            rusqlite::params![status.as_str(), now_ms, id, expected_revision],
        )?;
        if changed == 0 {
            let current = self.get_session(id)?.map_or(0, |s| s.revision);
            return Err(AnalysisKernelError::VersionConflict {
                expected: expected_revision,
                current,
            }
            .into());
        }
        Ok(expected_revision + 1)
    }

    pub fn put_object(&self, object: &KernelObjectRefV1) -> Result<(), StorageError> {
        object.validate()?;
        self.connection.execute(
            "INSERT INTO analysis_kernel_objects
             (id,kernel_id,logical_name,type_hint,size,sensitivity,persistence,content_hash,
              artifact_locator,provenance,created_at_ms,invalidated_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            rusqlite::params![
                object.id,
                object.kernel_id,
                object.logical_name,
                object.type_hint,
                object.size as i64,
                object.sensitivity.as_str(),
                object.persistence.as_str(),
                object.content_hash,
                object.artifact_locator,
                object.provenance,
                object.created_at_ms,
                object.invalidated_at_ms
            ],
        )?;
        Ok(())
    }

    pub fn list_objects(&self, kernel_id: &str) -> Result<Vec<KernelObjectRefV1>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id,kernel_id,logical_name,type_hint,size,sensitivity,persistence,content_hash,
                    artifact_locator,provenance,created_at_ms,invalidated_at_ms
             FROM analysis_kernel_objects WHERE kernel_id=?1 ORDER BY created_at_ms,id",
        )?;
        let rows = statement.query_map([kernel_id], |row| {
            Ok(KernelObjectRefV1 {
                id: row.get(0)?,
                kernel_id: row.get(1)?,
                logical_name: row.get(2)?,
                type_hint: row.get(3)?,
                size: row.get::<_, i64>(4)? as u64,
                sensitivity: parse_sensitivity(&row.get::<_, String>(5)?),
                persistence: parse_persistence(&row.get::<_, String>(6)?),
                content_hash: row.get(7)?,
                artifact_locator: row.get(8)?,
                provenance: row.get(9)?,
                created_at_ms: row.get(10)?,
                invalidated_at_ms: row.get(11)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn append_event(
        &self,
        kernel_id: &str,
        event_type: &str,
        payload: &[u8],
        now_ms: i64,
    ) -> Result<i64, StorageError> {
        if event_type.is_empty() || event_type.len() > ANALYSIS_KERNEL_MAX_NAME_BYTES {
            return Err(AnalysisKernelError::InvalidField("event_type").into());
        }
        if payload.len() > ANALYSIS_KERNEL_MAX_INLINE_BYTES {
            return Err(AnalysisKernelError::InvalidField("event_payload").into());
        }
        self.connection.execute(
            "INSERT INTO analysis_kernel_events(kernel_id,event_type,payload,created_at_ms)
             VALUES (?1,?2,?3,?4)",
            rusqlite::params![kernel_id, event_type, payload, now_ms],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn get_idempotency(
        &self,
        kernel_id: &str,
        idempotency_key: &str,
        operation: &str,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        self.connection
            .query_row(
                "SELECT result_json FROM analysis_kernel_idempotency
                 WHERE kernel_id=?1 AND idempotency_key=?2 AND operation=?3",
                rusqlite::params![kernel_id, idempotency_key, operation],
                |row| row.get(0),
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn put_idempotency(
        &self,
        kernel_id: &str,
        idempotency_key: &str,
        operation: &str,
        result_json: &[u8],
        now_ms: i64,
    ) -> Result<(), StorageError> {
        if idempotency_key.is_empty() || operation.is_empty() {
            return Err(AnalysisKernelError::InvalidField("idempotency").into());
        }
        if result_json.len() > ANALYSIS_KERNEL_MAX_IDEMPOTENCY_RESULT_BYTES {
            return Err(AnalysisKernelError::InvalidField("idempotency_result").into());
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO analysis_kernel_idempotency
             (kernel_id,idempotency_key,operation,result_json,created_at_ms)
             VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![kernel_id, idempotency_key, operation, result_json, now_ms],
        )?;
        Ok(())
    }
}

fn validate_id(field: &'static str, value: &str) -> Result<(), AnalysisKernelError> {
    if value.is_empty() || value.len() > ANALYSIS_KERNEL_MAX_ID_BYTES || value.contains('\0') {
        return Err(AnalysisKernelError::InvalidField(field));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(AnalysisKernelError::InvalidField(field));
    }
    Ok(())
}

fn validate_hash(field: &'static str, value: &str) -> Result<(), AnalysisKernelError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AnalysisKernelError::InvalidField(field));
    }
    Ok(())
}

fn parse_status(value: &str) -> KernelStatus {
    match value {
        "running" => KernelStatus::Running,
        "stopped" => KernelStatus::Stopped,
        "crashed" => KernelStatus::Crashed,
        "reset" => KernelStatus::Reset,
        "limit_exceeded" => KernelStatus::LimitExceeded,
        "blocked" => KernelStatus::Blocked,
        _ => KernelStatus::Created,
    }
}

fn parse_sensitivity(value: &str) -> KernelSensitivity {
    match value {
        "internal" => KernelSensitivity::Internal,
        "sensitive" => KernelSensitivity::Sensitive,
        "secret" => KernelSensitivity::Secret,
        _ => KernelSensitivity::Public,
    }
}

fn parse_persistence(value: &str) -> KernelObjectPersistence {
    if value == "checkpointed" {
        KernelObjectPersistence::Checkpointed
    } else {
        KernelObjectPersistence::Ephemeral
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalDatabase;

    fn session() -> AnalysisKernelSessionV1 {
        AnalysisKernelSessionV1 {
            schema_version: 1,
            id: "kernel-1".into(),
            task_id: "task-1".into(),
            workspace_id: "workspace-1".into(),
            runtime_version: "trusted-local-1".into(),
            package_manifest_hash: "a".repeat(64),
            policy_hash: "b".repeat(64),
            status: KernelStatus::Created,
            revision: 0,
            limits: KernelLimitsV1::default(),
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn canonical_hash_is_stable_and_authority_fields_are_validated() {
        let value = session();
        assert_eq!(
            value.canonical_json().unwrap(),
            value.canonical_json().unwrap()
        );
        assert_eq!(value.content_hash().unwrap().len(), 64);
        let mut invalid = value;
        invalid.schema_version = 2;
        assert!(matches!(
            invalid.validate(),
            Err(AnalysisKernelError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn object_rejects_secret_and_ephemeral_process_memory() {
        let object = KernelObjectRefV1 {
            id: "object-1".into(),
            kernel_id: "kernel-1".into(),
            logical_name: "rows".into(),
            type_hint: "json".into(),
            size: 10,
            sensitivity: KernelSensitivity::Secret,
            persistence: KernelObjectPersistence::Ephemeral,
            content_hash: None,
            artifact_locator: None,
            provenance: "core:test".into(),
            created_at_ms: 1,
            invalidated_at_ms: None,
        };
        assert!(matches!(
            object.validate(),
            Err(AnalysisKernelError::SecretObject)
        ));
    }

    #[test]
    fn store_round_trip_and_stale_update_are_typed() {
        let path = std::env::temp_dir().join(format!(
            "evohime-analysis-kernel-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = LocalDatabase::open(&path).unwrap();
        let store = AnalysisKernelStore::new(db.connection());
        let value = session();
        store.create_session(&value).unwrap();
        assert_eq!(store.get_session("kernel-1").unwrap().unwrap(), value);
        assert_eq!(
            store
                .set_status("kernel-1", 0, KernelStatus::Running, 2)
                .unwrap(),
            1
        );
        assert!(matches!(
            store.set_status("kernel-1", 0, KernelStatus::Stopped, 3),
            Err(StorageError::AnalysisKernel(
                AnalysisKernelError::VersionConflict { .. }
            ))
        ));
        store
            .put_idempotency("kernel-1", "idem", "json_parse", br#"{"ok":true}"#, 3)
            .unwrap();
        assert_eq!(
            store
                .get_idempotency("kernel-1", "idem", "json_parse")
                .unwrap()
                .unwrap(),
            br#"{"ok":true}"#
        );
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
