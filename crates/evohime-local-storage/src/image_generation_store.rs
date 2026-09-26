//! Bounded durable metadata for Core-owned image-generation jobs.
//!
//! Prompts and image bytes are never accepted by this store. Image content
//! remains in ArtifactStore; rows contain only request hashes, safe snapshots,
//! status and artifact references.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::StorageError;

/// Maximum bytes in the request/capability snapshot persisted for one job.
pub const MAX_JOB_SNAPSHOT_BYTES: usize = 16 * 1024;
/// Maximum bytes in the published artifact reference list for one job.
pub const MAX_JOB_RESULT_BYTES: usize = 8 * 1024;
/// Maximum identifier and idempotency-key bytes.
pub const MAX_JOB_ID_BYTES: usize = 128;
/// Maximum bounded terminal reason code bytes.
pub const MAX_ERROR_CODE_BYTES: usize = 96;

/// Durable image-job lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageJobState {
    /// Request passed validation but has not reached a provider.
    Preflight,
    /// Request is admitted and waiting for dispatch.
    Queued,
    /// Provider dispatch began; restart recovery treats its outcome as ambiguous.
    Dispatched,
    /// All image outputs were verified and published.
    Completed,
    /// The operation failed with a bounded reason code.
    Failed,
    /// The operation was cancelled before provider dispatch.
    Cancelled,
    /// Provider dispatch may have happened, but no safe result can be recovered.
    UnknownOutcome,
}

impl ImageJobState {
    /// Returns the stable serialized state name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preflight => "preflight",
            Self::Queued => "queued",
            Self::Dispatched => "dispatched",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::UnknownOutcome => "unknown_outcome",
        }
    }

    /// Returns whether a transition is allowed by the durable lifecycle.
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Preflight,
                Self::Queued | Self::Failed | Self::Cancelled
            ) | (
                Self::Queued,
                Self::Dispatched | Self::Failed | Self::Cancelled
            ) | (
                Self::Dispatched,
                Self::Completed | Self::Failed | Self::UnknownOutcome
            ) | (Self::UnknownOutcome, Self::Completed | Self::Failed)
                | (Self::Completed, Self::Failed)
        )
    }
}

/// Metadata-only image job record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageJobRecord {
    /// Stable Core-generated job identifier.
    pub job_id: String,
    /// Owning task identifier used by ArtifactStore access checks.
    pub task_id: String,
    /// Caller-supplied bounded idempotency key.
    pub idempotency_key: String,
    /// Digest of ephemeral request content and input artifact hashes.
    pub request_hash: String,
    /// Durable lifecycle state.
    pub state: ImageJobState,
    /// Monotonic optimistic revision.
    pub revision: u64,
    /// Safe request and frozen route/capability metadata, without prompt or bytes.
    pub snapshot_json: Vec<u8>,
    /// Verified ArtifactStore references only; no image bytes or prompt content.
    pub result_json: Option<Vec<u8>>,
    /// Stable bounded failure reason, when terminal without a result.
    pub error_code: Option<String>,
    /// Creation time in Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Last transition time in Unix epoch milliseconds.
    pub updated_at_ms: i64,
}

/// Outcome of an idempotent image-job insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertImageJobOutcome {
    /// A new job row was committed.
    Created(ImageJobRecord),
    /// The existing matching job was returned.
    Existing(ImageJobRecord),
}

/// Storage methods for metadata-only image jobs.
pub struct ImageGenerationStore<'a> {
    connection: &'a Connection,
}

impl<'a> ImageGenerationStore<'a> {
    /// Creates a store using the caller-owned SQLite connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Creates the additive image-generation metadata schema inside a migration transaction.
    pub fn install_schema(transaction: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
        crate::migrations::v177::apply(transaction, 176)
    }

    /// Inserts a preflight job, returning a matching idempotent replay when present.
    pub fn insert_preflight(
        &self,
        record: &ImageJobRecord,
    ) -> Result<InsertImageJobOutcome, StorageError> {
        validate_record(record)?;
        if record.state != ImageJobState::Preflight
            || record.revision != 1
            || record.result_json.is_some()
            || record.error_code.is_some()
        {
            return Err(StorageError::InvalidInput(
                "invalid_initial_image_job_state".into(),
            ));
        }
        if let Some(existing) = self.get_by_idempotency(&record.task_id, &record.idempotency_key)? {
            if existing.request_hash == record.request_hash {
                return Ok(InsertImageJobOutcome::Existing(existing));
            }
            return Err(StorageError::InvalidInput(
                "image_idempotency_conflict".into(),
            ));
        }
        self.connection.execute(
            "INSERT INTO image_generation_jobs
                (job_id,task_id,idempotency_key,request_hash,state,revision,snapshot_json,
                 result_json,error_code,created_at_ms,updated_at_ms)
             VALUES (?1,?2,?3,?4,'preflight',1,?5,NULL,NULL,?6,?6)",
            params![
                record.job_id,
                record.task_id,
                record.idempotency_key,
                record.request_hash,
                record.snapshot_json,
                record.created_at_ms,
            ],
        )?;
        Ok(InsertImageJobOutcome::Created(record.clone()))
    }

    /// Reads one image job by its stable identifier.
    pub fn get(&self, job_id: &str) -> Result<Option<ImageJobRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT job_id,task_id,idempotency_key,request_hash,state,revision,
                        snapshot_json,result_json,error_code,created_at_ms,updated_at_ms
                 FROM image_generation_jobs WHERE job_id=?1",
                [job_id],
                read_record,
            )
            .optional()?)
    }

    /// Reads the task-scoped record bound to one idempotency key.
    pub fn get_by_idempotency(
        &self,
        task_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<ImageJobRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT job_id,task_id,idempotency_key,request_hash,state,revision,
                        snapshot_json,result_json,error_code,created_at_ms,updated_at_ms
                 FROM image_generation_jobs WHERE task_id=?1 AND idempotency_key=?2",
                params![task_id, idempotency_key],
                read_record,
            )
            .optional()?)
    }

    /// Applies one revision-checked state transition and replaces safe metadata atomically.
    #[allow(clippy::too_many_arguments)]
    pub fn transition(
        &self,
        job_id: &str,
        expected_revision: u64,
        next: ImageJobState,
        snapshot_json: &[u8],
        result_json: Option<&[u8]>,
        error_code: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, StorageError> {
        validate_metadata(snapshot_json, result_json, error_code)?;
        let Some(current) = self.get(job_id)? else {
            return Ok(false);
        };
        if current.revision != expected_revision || !current.state.can_transition_to(next) {
            return Ok(false);
        }
        let completed_result_required = next == ImageJobState::Completed;
        if completed_result_required != result_json.is_some() {
            return Err(StorageError::InvalidInput(
                "image_result_state_mismatch".into(),
            ));
        }
        let next_revision = expected_revision.saturating_add(1);
        Ok(self.connection.execute(
            "UPDATE image_generation_jobs
             SET state=?1,revision=?2,snapshot_json=?3,result_json=?4,error_code=?5,updated_at_ms=?6
             WHERE job_id=?7 AND revision=?8 AND state=?9",
            params![
                next.as_str(),
                next_revision as i64,
                snapshot_json,
                result_json,
                error_code,
                now_ms,
                job_id,
                expected_revision as i64,
                current.state.as_str(),
            ],
        )? == 1)
    }

    /// Marks jobs interrupted before dispatch as cancelled and dispatched jobs as unknown.
    pub fn recover_after_restart(&self, now_ms: i64) -> Result<usize, StorageError> {
        let cancelled = self.connection.execute(
            "UPDATE image_generation_jobs
             SET state='cancelled',revision=revision+1,error_code='core_restarted_before_dispatch',updated_at_ms=?1
             WHERE state IN ('preflight','queued')",
            [now_ms],
        )?;
        let unknown = self.connection.execute(
            "UPDATE image_generation_jobs
             SET state='unknown_outcome',revision=revision+1,error_code='core_restarted_after_dispatch',updated_at_ms=?1
             WHERE state='dispatched'",
            [now_ms],
        )?;
        Ok(cancelled.saturating_add(unknown))
    }
}

fn validate_record(record: &ImageJobRecord) -> Result<(), StorageError> {
    for value in [&record.job_id, &record.task_id, &record.idempotency_key] {
        if value.trim().is_empty() || value.len() > MAX_JOB_ID_BYTES {
            return Err(StorageError::InvalidInput("image_job_id_limit".into()));
        }
    }
    if record.request_hash.len() != 64
        || !record
            .request_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(StorageError::InvalidInput(
            "image_request_hash_invalid".into(),
        ));
    }
    validate_metadata(
        &record.snapshot_json,
        record.result_json.as_deref(),
        record.error_code.as_deref(),
    )
}

fn validate_metadata(
    snapshot: &[u8],
    result: Option<&[u8]>,
    error_code: Option<&str>,
) -> Result<(), StorageError> {
    if snapshot.is_empty() || snapshot.len() > MAX_JOB_SNAPSHOT_BYTES {
        return Err(StorageError::InvalidInput("image_snapshot_limit".into()));
    }
    validate_redacted_json(snapshot)?;
    if let Some(result) = result {
        if result.is_empty() || result.len() > MAX_JOB_RESULT_BYTES {
            return Err(StorageError::InvalidInput("image_result_limit".into()));
        }
        validate_redacted_json(result)?;
    }
    if error_code.is_some_and(|code| {
        code.is_empty()
            || code.len() > MAX_ERROR_CODE_BYTES
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    }) {
        return Err(StorageError::InvalidInput(
            "image_error_code_invalid".into(),
        ));
    }
    Ok(())
}

fn validate_redacted_json(bytes: &[u8]) -> Result<(), StorageError> {
    fn check(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(map) => map.iter().all(|(key, value)| {
                let key = key.to_ascii_lowercase();
                ![
                    "prompt",
                    "base64",
                    "image_bytes",
                    "raw_bytes",
                    "binary_data",
                ]
                .iter()
                .any(|marker| key.contains(marker))
                    && check(value)
            }),
            serde_json::Value::Array(values) => values.iter().all(check),
            _ => true,
        }
    }
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    if !check(&value) {
        return Err(StorageError::InvalidInput(
            "image_metadata_not_redacted".into(),
        ));
    }
    Ok(())
}

fn read_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ImageJobRecord> {
    let state: String = row.get(4)?;
    let state = match state.as_str() {
        "preflight" => ImageJobState::Preflight,
        "queued" => ImageJobState::Queued,
        "dispatched" => ImageJobState::Dispatched,
        "completed" => ImageJobState::Completed,
        "failed" => ImageJobState::Failed,
        "cancelled" => ImageJobState::Cancelled,
        "unknown_outcome" => ImageJobState::UnknownOutcome,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(ImageJobRecord {
        job_id: row.get(0)?,
        task_id: row.get(1)?,
        idempotency_key: row.get(2)?,
        request_hash: row.get(3)?,
        state,
        revision: row.get::<_, i64>(5)?.max(0) as u64,
        snapshot_json: row.get(6)?,
        result_json: row.get(7)?,
        error_code: row.get(8)?,
        created_at_ms: row.get(9)?,
        updated_at_ms: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn record(job_id: &str, request_hash: &str, idempotency_key: &str) -> ImageJobRecord {
        ImageJobRecord {
            job_id: job_id.into(),
            task_id: "task-1".into(),
            idempotency_key: idempotency_key.into(),
            request_hash: request_hash.into(),
            state: ImageJobState::Preflight,
            revision: 1,
            snapshot_json: br#"{"operation":"generate","capability_epoch":4}"#.to_vec(),
            result_json: None,
            error_code: None,
            created_at_ms: 10,
            updated_at_ms: 10,
        }
    }

    fn initialized_connection() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database");
        let transaction = connection.unchecked_transaction().expect("transaction");
        crate::migrations::v177::apply(&transaction, 176).expect("image job schema");
        transaction.commit().expect("commit image job schema");
        connection
    }

    #[test]
    fn image_job_insert_is_idempotent_and_rejects_conflicting_key_reuse() {
        let connection = initialized_connection();
        let store = ImageGenerationStore::new(&connection);
        let first = record("job-1", &"a".repeat(64), "once");
        assert!(matches!(
            store.insert_preflight(&first).expect("insert"),
            InsertImageJobOutcome::Created(_)
        ));
        assert!(matches!(
            store
                .insert_preflight(&record("job-2", &"a".repeat(64), "once"))
                .expect("idempotent replay"),
            InsertImageJobOutcome::Existing(_)
        ));
        assert!(matches!(
            store.insert_preflight(&record("job-3", &"b".repeat(64), "once")),
            Err(StorageError::InvalidInput(reason)) if reason == "image_idempotency_conflict"
        ));
    }

    #[test]
    fn image_job_transition_uses_revision_cas_and_restart_never_retries_dispatch() {
        assert!(ImageJobState::Completed.can_transition_to(ImageJobState::Failed));
        assert!(!ImageJobState::Completed.can_transition_to(ImageJobState::Dispatched));
        let connection = initialized_connection();
        let store = ImageGenerationStore::new(&connection);
        store
            .insert_preflight(&record("job-1", &"a".repeat(64), "once"))
            .expect("insert");
        assert!(store
            .transition(
                "job-1",
                1,
                ImageJobState::Queued,
                br#"{"operation":"generate"}"#,
                None,
                None,
                11,
            )
            .expect("queue"));
        assert!(!store
            .transition(
                "job-1",
                1,
                ImageJobState::Dispatched,
                br#"{"operation":"generate"}"#,
                None,
                None,
                12,
            )
            .expect("stale transition"));
        assert!(store
            .transition(
                "job-1",
                2,
                ImageJobState::Dispatched,
                br#"{"operation":"generate"}"#,
                None,
                None,
                13,
            )
            .expect("dispatch"));
        assert_eq!(store.recover_after_restart(14).expect("recovery"), 1);
        let recovered = store.get("job-1").expect("read").expect("job exists");
        assert_eq!(recovered.state, ImageJobState::UnknownOutcome);
        assert_eq!(recovered.revision, 4);
        assert!(!recovered.error_code.unwrap_or_default().contains("retry"));
        store
            .insert_preflight(&record("job-2", &"c".repeat(64), "cancel-before-dispatch"))
            .expect("second job insert");
        assert!(store
            .transition(
                "job-2",
                1,
                ImageJobState::Cancelled,
                br#"{"operation":"generate"}"#,
                None,
                Some("cancelled_before_dispatch"),
                15,
            )
            .expect("cancel before dispatch"));
        assert!(!store
            .transition(
                "job-2",
                1,
                ImageJobState::Dispatched,
                br#"{"operation":"generate"}"#,
                None,
                None,
                16,
            )
            .expect("dispatch cannot beat completed cancel CAS"));
    }

    #[test]
    fn image_metadata_rejects_raw_prompt_keys_and_result_bytes() {
        let connection = initialized_connection();
        let store = ImageGenerationStore::new(&connection);
        let mut unsafe_record = record("job-1", &"a".repeat(64), "once");
        unsafe_record.snapshot_json = br#"{"prompt":"secret prompt"}"#.to_vec();
        assert!(matches!(
            store.insert_preflight(&unsafe_record),
            Err(StorageError::InvalidInput(reason)) if reason == "image_metadata_not_redacted"
        ));
        let mut binary_result = record("job-2", &"b".repeat(64), "twice");
        binary_result.result_json = Some(br#"{"raw_bytes":"AAAA"}"#.to_vec());
        assert!(matches!(
            validate_record(&binary_result),
            Err(StorageError::InvalidInput(reason)) if reason == "image_metadata_not_redacted"
        ));
    }
}
