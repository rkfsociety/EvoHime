//! Durable contract and manifest storage for plan 28 (Persistent Analysis Kernel).
//!
//! This module stores metadata only. Kernel process memory and arbitrary values
//! are deliberately absent; large values must be addressed by an existing
//! Core-owned ArtifactStore reference.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::StorageError;

/// Version of the analysis kernel contract implemented by this crate.
pub const ANALYSIS_KERNEL_VERSION: u32 = 1;
/// Version of the serialized session and object schemas.
pub const ANALYSIS_KERNEL_SCHEMA_VERSION: u32 = 1;
/// Maximum UTF-8 byte length for kernel, task, workspace, and object identifiers.
pub const ANALYSIS_KERNEL_MAX_ID_BYTES: usize = 128;
/// Maximum UTF-8 byte length for an object's logical name.
pub const ANALYSIS_KERNEL_MAX_NAME_BYTES: usize = 128;
/// Maximum size of an inline object value in bytes.
pub const ANALYSIS_KERNEL_MAX_INLINE_BYTES: usize = 16 * 1024;
/// Maximum number of object references in one kernel session.
pub const ANALYSIS_KERNEL_MAX_OBJECTS: usize = 1024;
/// Maximum aggregate object size tracked for a kernel session.
pub const ANALYSIS_KERNEL_MAX_OBJECT_BYTES: u64 = 256 * 1024 * 1024;
/// Maximum serialized result size stored for an idempotency key.
pub const ANALYSIS_KERNEL_MAX_IDEMPOTENCY_RESULT_BYTES: usize = 16 * 1024;
/// Maximum number of sessions returned by the running-session query.
pub const ANALYSIS_KERNEL_MAX_RUNNING_SESSIONS: usize = 256;

/// Persisted lifecycle state for an analysis kernel session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelStatus {
    /// Session metadata was created but execution has not begun.
    Created,
    /// Kernel is currently executing.
    Running,
    /// Kernel stopped normally.
    Stopped,
    /// Kernel exited unexpectedly.
    Crashed,
    /// Kernel state was reset.
    Reset,
    /// A declared execution limit was exceeded.
    LimitExceeded,
    /// Kernel execution is blocked by policy or host control.
    Blocked,
}

impl KernelStatus {
    /// Returns the stable snake-case representation stored in SQLite.
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

/// Persistence lifetime for an analysis-kernel object reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelObjectPersistence {
    /// Object belongs only to the current session and may be discarded on shutdown.
    Ephemeral,
    /// Object is backed by a durable artifact reference.
    Checkpointed,
}

impl KernelObjectPersistence {
    /// Returns the stable snake-case representation stored in SQLite.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ephemeral => "ephemeral",
            Self::Checkpointed => "checkpointed",
        }
    }
}

/// Data sensitivity classification used to decide whether inline storage is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelSensitivity {
    /// Data is suitable for unrestricted local display.
    Public,
    /// Data is internal to the application.
    Internal,
    /// Data requires additional handling and must not be inlined.
    Sensitive,
    /// Data is secret and must not be inlined.
    Secret,
}

impl KernelSensitivity {
    /// Returns the stable snake-case representation stored in SQLite.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Sensitive => "sensitive",
            Self::Secret => "secret",
        }
    }

    /// Reports whether this sensitivity permits inline object data.
    pub const fn allows_inline(self) -> bool {
        matches!(self, Self::Public | Self::Internal)
    }
}

/// Resource and time limits enforced for one analysis kernel session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelLimitsV1 {
    /// Maximum CPU time in milliseconds.
    pub cpu_time_ms: u64,
    /// Maximum memory use in bytes.
    pub memory_bytes: u64,
    /// Maximum captured output size in bytes.
    pub output_bytes: u64,
    /// Maximum number of object references.
    pub object_count: u32,
    /// Maximum aggregate object size in bytes.
    pub object_bytes: u64,
    /// Maximum host requests per minute.
    pub host_requests_per_minute: u32,
    /// Idle timeout in milliseconds.
    pub idle_timeout_ms: u64,
    /// Absolute session lifetime in milliseconds.
    pub lifetime_timeout_ms: u64,
}

impl Default for KernelLimitsV1 {
    /// Uses the bounded local runtime defaults defined by the kernel contract.
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
    /// Checks that every configured limit is non-zero and within contract bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use evohime_local_storage::analysis_kernel::KernelLimitsV1;
    ///
    /// KernelLimitsV1::default().validate().unwrap();
    /// ```
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

/// Versioned metadata describing one persistent analysis kernel session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisKernelSessionV1 {
    /// Serialized session schema version.
    pub schema_version: u32,
    /// Stable session identifier.
    pub id: String,
    /// Task that owns the session.
    pub task_id: String,
    /// Workspace associated with the session.
    pub workspace_id: String,
    /// Runtime contract version expected by the session.
    pub runtime_version: String,
    /// Digest of the package manifest used to initialize the session.
    pub package_manifest_hash: String,
    /// Digest of the effective policy snapshot.
    pub policy_hash: String,
    /// Current session lifecycle state.
    pub status: KernelStatus,
    /// Monotonically increasing session revision.
    pub revision: u64,
    /// Resource limits bound to this session.
    pub limits: KernelLimitsV1,
    /// Session creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Last session update time in Unix milliseconds.
    pub updated_at_ms: i64,
}

impl AnalysisKernelSessionV1 {
    /// Validates schema, required identifiers, hashes, runtime version, and limits.
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

    /// Serializes the validated session to deterministic JSON bytes.
    pub fn canonical_json(&self) -> Result<Vec<u8>, AnalysisKernelError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| AnalysisKernelError::Serialization)
    }

    /// Computes a SHA-256 digest of the canonical serialized session.
    pub fn content_hash(&self) -> Result<String, AnalysisKernelError> {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json()?);
        Ok(hex::encode(hasher.finalize()))
    }
}

/// Metadata reference to an object owned by the analysis kernel or artifact store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelObjectRefV1 {
    /// Stable object reference identifier.
    pub id: String,
    /// Session that owns this object.
    pub kernel_id: String,
    /// Bounded logical name used by the session.
    pub logical_name: String,
    /// Optional type label supplied by the producer.
    pub type_hint: String,
    /// Object content size in bytes.
    pub size: u64,
    /// Sensitivity classification that controls inline handling.
    pub sensitivity: KernelSensitivity,
    /// Whether the object is ephemeral or artifact-backed.
    pub persistence: KernelObjectPersistence,
    /// Optional content hash for the object bytes.
    pub content_hash: Option<String>,
    /// Optional locator in the Core-owned artifact store.
    pub artifact_locator: Option<String>,
    /// Provenance label for the object.
    pub provenance: String,
    /// Creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Invalidation time, if this object is no longer usable.
    pub invalidated_at_ms: Option<i64>,
}

impl KernelObjectRefV1 {
    /// Validates identifiers, names, sizes, hashes, sensitivity, and persistence constraints.
    pub fn validate(&self) -> Result<(), AnalysisKernelError> {
        validate_id("id", &self.id)?;
        validate_id("kernel_id", &self.kernel_id)?;
        validate_id("logical_name", &self.logical_name)?;
        validate_id("type_hint", &self.type_hint)?;
        validate_id("provenance", &self.provenance)?;
        if self.size > ANALYSIS_KERNEL_MAX_OBJECT_BYTES {
            return Err(AnalysisKernelError::ObjectTooLarge(self.size));
        }
        if self.persistence == KernelObjectPersistence::Checkpointed
            && self.sensitivity.allows_inline()
            && self.size > 0
            && self.artifact_locator.is_none()
        {
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

/// Validation and persistence errors in the analysis kernel contract.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnalysisKernelError {
    /// Requested schema version is unsupported.
    #[error("unsupported analysis kernel version {0}")]
    UnsupportedVersion(u32),
    /// A session or object field violated its contract.
    #[error("invalid analysis kernel field {0}")]
    InvalidField(&'static str),
    /// A resource limit was zero or exceeded its maximum.
    #[error("invalid analysis kernel limits: {0}")]
    InvalidLimits(&'static str),
    /// Object size exceeded the per-session bound.
    #[error("analysis kernel object is too large: {0} bytes")]
    ObjectTooLarge(u64),
    /// Serialized request exceeded the accepted request size.
    #[error("analysis kernel request is too large: {0} bytes")]
    RequestTooLarge(usize),
    /// The requested operation is prohibited by the kernel contract.
    #[error("analysis kernel operation is not permitted")]
    ForbiddenOperation,
    /// The requested capability is prohibited by policy.
    #[error("analysis kernel capability is not permitted")]
    ForbiddenCapability,
    /// Object metadata lacks a required artifact locator.
    #[error("analysis kernel object requires an ArtifactStore reference")]
    MissingArtifactRef,
    /// A checkpointed object lacks its digest or artifact locator.
    #[error("checkpointed object requires a hash and ArtifactStore reference")]
    CheckpointRequiresArtifact,
    /// Secret-classified object metadata is rejected by this contract.
    #[error("secret kernel objects are not accepted")]
    SecretObject,
    /// Process memory was requested as durable persisted state.
    #[error("analysis kernel values cannot be persisted as process memory")]
    ProcessMemoryPersistence,
    /// Canonical serialization could not be produced.
    #[error("analysis kernel canonical serialization failed")]
    Serialization,
    /// Sensitive data was supplied through a field that permits only inline-safe content.
    #[error("analysis kernel sensitive inline payload is forbidden")]
    SensitiveInlinePayload,
    /// Session revision did not match the caller's expected revision.
    #[error("analysis kernel optimistic version conflict: expected {expected}, current {current}")]
    VersionConflict {
        /// Revision expected by the caller.
        expected: u64,
        /// Revision currently stored.
        current: u64,
    },
}

/// Creates the analysis kernel session, object, event, and idempotency tables and indexes.
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

/// SQLite repository for session metadata, object references, events, and idempotency results.
pub struct AnalysisKernelStore<'a> {
    connection: &'a Connection,
}

impl<'a> AnalysisKernelStore<'a> {
    /// Creates a store borrowing the caller-owned SQLite connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Validates and inserts a new session record.
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

    /// Loads a session by identifier, returning `None` when it does not exist.
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

    /// Lists running sessions in identifier order, capped by [`ANALYSIS_KERNEL_MAX_RUNNING_SESSIONS`].
    pub fn list_running_sessions(&self) -> Result<Vec<AnalysisKernelSessionV1>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM analysis_kernel_sessions WHERE status='running' ORDER BY id LIMIT ?1",
        )?;
        let ids = statement
            .query_map([ANALYSIS_KERNEL_MAX_RUNNING_SESSIONS as i64], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                self.get_session(&id)?
                    .ok_or_else(|| StorageError::InvalidInput("kernel session disappeared".into()))
            })
            .collect()
    }

    /// Changes a session's status only if its current revision matches `expected_revision`.
    ///
    /// Returns the incremented revision or a version-conflict error when another writer won.
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

    /// Validates and inserts an object reference without replacing an existing identifier.
    pub fn put_object(&self, object: &KernelObjectRefV1) -> Result<(), StorageError> {
        object.validate()?;
        self.connection.execute(
            "INSERT OR IGNORE INTO analysis_kernel_objects
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

    /// Lists a session's object references in creation order, capped at [`ANALYSIS_KERNEL_MAX_OBJECTS`].
    pub fn list_objects(&self, kernel_id: &str) -> Result<Vec<KernelObjectRefV1>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id,kernel_id,logical_name,type_hint,size,sensitivity,persistence,content_hash,
                    artifact_locator,provenance,created_at_ms,invalidated_at_ms
             FROM analysis_kernel_objects WHERE kernel_id=?1 ORDER BY created_at_ms,id LIMIT ?2",
        )?;
        let rows = statement.query_map(
            rusqlite::params![kernel_id, ANALYSIS_KERNEL_MAX_OBJECTS as i64],
            |row| {
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
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// Appends a bounded event payload and returns its SQLite sequence identifier.
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

    /// Loads a previously stored result for a session-scoped operation key.
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

    /// Stores an operation result once, enforcing key and result-size bounds.
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
#[path = "analysis_kernel_tests.rs"]
mod tests;
