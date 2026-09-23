//! Deterministic, type-specific verification of snapshot outcomes.
//!
//! This module deliberately has no persistence or retry side effects.  A caller
//! supplies the expected and observed snapshot facts once and receives an
//! auditable decision: confirmed, unconfirmed, or blocked.

use serde::{Deserialize, Serialize};

/// Maximum byte length accepted for a snapshot identifier.
pub const MAX_SNAPSHOT_ID_BYTES: usize = 256;
/// Maximum byte length accepted for expected and observed hashes.
pub const MAX_HASH_BYTES: usize = 128;
/// Maximum byte length accepted for a verification reason code.
pub const MAX_REASON_BYTES: usize = 256;

/// Type of external state represented by a reconciliation snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    /// A file identified by a content hash.
    File,
    /// A database identified by schema version and content hash.
    Database,
    /// A process identified by its generation and liveness.
    Process,
}

/// Expected and observed facts used to verify a file snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshotOutcome {
    /// Hash the file is expected to have.
    pub expected_hash: String,
    /// Observed file hash, or `None` when unavailable.
    pub observed_hash: Option<String>,
    /// Whether the file currently exists.
    pub exists: bool,
}

/// Expected and observed schema/hash facts for a database snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseSnapshotOutcome {
    /// Schema version expected by the caller.
    pub expected_schema_version: u32,
    /// Observed schema version, or `None` when unavailable.
    pub observed_schema_version: Option<u32>,
    /// Content hash expected by the caller.
    pub expected_content_hash: String,
    /// Observed database content hash, or `None` when unavailable.
    pub observed_content_hash: Option<String>,
}

/// Expected and observed process-generation facts for a process snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessSnapshotOutcome {
    /// Process generation expected by the caller.
    pub expected_generation: u64,
    /// Observed generation, or `None` when it could not be read.
    pub observed_generation: Option<u64>,
    /// Whether the process is currently alive.
    pub alive: bool,
}

/// Type-tagged outcome for one file, database, or process snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "outcome", rename_all = "snake_case")]
pub enum SnapshotOutcome {
    /// Verification evidence for a file.
    File(FileSnapshotOutcome),
    /// Verification evidence for a database.
    Database(DatabaseSnapshotOutcome),
    /// Verification evidence for a process.
    Process(ProcessSnapshotOutcome),
}

impl SnapshotOutcome {
    /// Returns the snapshot kind represented by this outcome.
    pub fn kind(&self) -> SnapshotKind {
        match self {
            Self::File(_) => SnapshotKind::File,
            Self::Database(_) => SnapshotKind::Database,
            Self::Process(_) => SnapshotKind::Process,
        }
    }
}

/// Result classification returned by [`verify_snapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// Available evidence matches the expected snapshot.
    Confirmed,
    /// Evidence is available but contradicts the expected snapshot.
    Unconfirmed,
    /// Required evidence is missing, so verification cannot conclude.
    Blocked,
}

/// Auditable result for a snapshot verification attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotVerification {
    /// Identifier supplied for the snapshot.
    pub snapshot_id: String,
    /// Kind of snapshot that was checked.
    pub kind: SnapshotKind,
    /// Verification classification.
    pub status: VerificationStatus,
    /// Stable machine-readable explanation of the result.
    pub reason_code: String,
}

/// Validation failures for snapshot identifiers, hashes, and reason codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationError {
    /// The snapshot identifier was empty.
    EmptySnapshotId,
    /// The snapshot identifier exceeded [`MAX_SNAPSHOT_ID_BYTES`].
    SnapshotIdTooLong,
    /// A hash was empty or exceeded [`MAX_HASH_BYTES`].
    HashTooLong,
    /// A hash contained a non-hexadecimal character.
    InvalidHash,
    /// The reason code exceeded [`MAX_REASON_BYTES`].
    ReasonTooLong,
}

impl SnapshotVerification {
    fn new(
        snapshot_id: &str,
        kind: SnapshotKind,
        status: VerificationStatus,
        reason: &str,
    ) -> Result<Self, VerificationError> {
        if snapshot_id.is_empty() {
            return Err(VerificationError::EmptySnapshotId);
        }
        if snapshot_id.len() > MAX_SNAPSHOT_ID_BYTES {
            return Err(VerificationError::SnapshotIdTooLong);
        }
        if reason.len() > MAX_REASON_BYTES {
            return Err(VerificationError::ReasonTooLong);
        }
        Ok(Self {
            snapshot_id: snapshot_id.to_owned(),
            kind,
            status,
            reason_code: reason.to_owned(),
        })
    }
}

fn valid_hash(hash: &str) -> Result<(), VerificationError> {
    if hash.is_empty() || hash.len() > MAX_HASH_BYTES {
        return Err(VerificationError::HashTooLong);
    }
    if !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(VerificationError::InvalidHash);
    }
    Ok(())
}

/// Verify one snapshot outcome without retrying or mutating state.
pub fn verify_snapshot(
    snapshot_id: &str,
    outcome: &SnapshotOutcome,
) -> Result<SnapshotVerification, VerificationError> {
    let kind = outcome.kind();
    let (status, reason) = match outcome {
        SnapshotOutcome::File(file) => {
            valid_hash(&file.expected_hash)?;
            if let Some(observed) = file.observed_hash.as_deref() {
                valid_hash(observed)?;
            }
            if !file.exists {
                (VerificationStatus::Unconfirmed, "file_missing")
            } else if file.observed_hash.as_deref() == Some(file.expected_hash.as_str()) {
                (VerificationStatus::Confirmed, "file_hash_match")
            } else {
                (VerificationStatus::Unconfirmed, "file_hash_mismatch")
            }
        }
        SnapshotOutcome::Database(database) => {
            valid_hash(&database.expected_content_hash)?;
            if let Some(observed) = database.observed_content_hash.as_deref() {
                valid_hash(observed)?;
            }
            match (
                database.observed_schema_version,
                database.observed_content_hash.as_deref(),
            ) {
                (None, _) | (_, None) => (VerificationStatus::Blocked, "database_evidence_missing"),
                (Some(schema), Some(_hash)) if schema != database.expected_schema_version => {
                    (VerificationStatus::Unconfirmed, "database_schema_mismatch")
                }
                (Some(_), Some(hash)) if hash != database.expected_content_hash => {
                    (VerificationStatus::Unconfirmed, "database_hash_mismatch")
                }
                (Some(_), Some(_)) => (VerificationStatus::Confirmed, "database_snapshot_match"),
            }
        }
        SnapshotOutcome::Process(process) => match process.observed_generation {
            None => (VerificationStatus::Blocked, "process_generation_missing"),
            Some(_generation) if !process.alive => {
                (VerificationStatus::Unconfirmed, "process_not_alive")
            }
            Some(generation) if generation != process.expected_generation => (
                VerificationStatus::Unconfirmed,
                "process_generation_mismatch",
            ),
            Some(_) => (VerificationStatus::Confirmed, "process_generation_match"),
        },
    };
    SnapshotVerification::new(snapshot_id, kind, status, reason)
}

#[cfg(test)]
#[path = "reconciliation_verifier_tests.rs"]
mod tests;
