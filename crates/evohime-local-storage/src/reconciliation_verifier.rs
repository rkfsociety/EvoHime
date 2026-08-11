//! Deterministic, type-specific verification of snapshot outcomes.
//!
//! This module deliberately has no persistence or retry side effects.  A caller
//! supplies the expected and observed snapshot facts once and receives an
//! auditable decision: confirmed, unconfirmed, or blocked.

use serde::{Deserialize, Serialize};

pub const MAX_SNAPSHOT_ID_BYTES: usize = 256;
pub const MAX_HASH_BYTES: usize = 128;
pub const MAX_REASON_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    File,
    Database,
    Process,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshotOutcome {
    pub expected_hash: String,
    pub observed_hash: Option<String>,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseSnapshotOutcome {
    pub expected_schema_version: u32,
    pub observed_schema_version: Option<u32>,
    pub expected_content_hash: String,
    pub observed_content_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessSnapshotOutcome {
    pub expected_generation: u64,
    pub observed_generation: Option<u64>,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "outcome", rename_all = "snake_case")]
pub enum SnapshotOutcome {
    File(FileSnapshotOutcome),
    Database(DatabaseSnapshotOutcome),
    Process(ProcessSnapshotOutcome),
}

impl SnapshotOutcome {
    pub fn kind(&self) -> SnapshotKind {
        match self {
            Self::File(_) => SnapshotKind::File,
            Self::Database(_) => SnapshotKind::Database,
            Self::Process(_) => SnapshotKind::Process,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Confirmed,
    Unconfirmed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotVerification {
    pub snapshot_id: String,
    pub kind: SnapshotKind,
    pub status: VerificationStatus,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationError {
    EmptySnapshotId,
    SnapshotIdTooLong,
    HashTooLong,
    InvalidHash,
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
mod tests {
    use super::*;

    #[test]
    fn file_hash_match_is_confirmed() {
        let result = verify_snapshot(
            "snapshot-file",
            &SnapshotOutcome::File(FileSnapshotOutcome {
                expected_hash: "aabb".into(),
                observed_hash: Some("aabb".into()),
                exists: true,
            }),
        )
        .unwrap();
        assert_eq!(result.status, VerificationStatus::Confirmed);
        assert_eq!(result.reason_code, "file_hash_match");
    }

    #[test]
    fn mismatch_is_unconfirmed_without_retry() {
        let result = verify_snapshot(
            "snapshot-file",
            &SnapshotOutcome::File(FileSnapshotOutcome {
                expected_hash: "aabb".into(),
                observed_hash: Some("ccdd".into()),
                exists: true,
            }),
        )
        .unwrap();
        assert_eq!(result.status, VerificationStatus::Unconfirmed);
        assert_eq!(result.reason_code, "file_hash_mismatch");
    }

    #[test]
    fn missing_database_evidence_is_blocked() {
        let result = verify_snapshot(
            "snapshot-db",
            &SnapshotOutcome::Database(DatabaseSnapshotOutcome {
                expected_schema_version: 6,
                observed_schema_version: Some(6),
                expected_content_hash: "aabb".into(),
                observed_content_hash: None,
            }),
        )
        .unwrap();
        assert_eq!(result.status, VerificationStatus::Blocked);
    }

    #[test]
    fn process_generation_mismatch_is_unconfirmed() {
        let result = verify_snapshot(
            "snapshot-process",
            &SnapshotOutcome::Process(ProcessSnapshotOutcome {
                expected_generation: 2,
                observed_generation: Some(3),
                alive: true,
            }),
        )
        .unwrap();
        assert_eq!(result.status, VerificationStatus::Unconfirmed);
        assert_eq!(result.reason_code, "process_generation_mismatch");
    }

    #[test]
    fn oversized_input_is_rejected() {
        let result = verify_snapshot(
            &"x".repeat(MAX_SNAPSHOT_ID_BYTES + 1),
            &SnapshotOutcome::File(FileSnapshotOutcome {
                expected_hash: "aabb".into(),
                observed_hash: Some("aabb".into()),
                exists: true,
            }),
        );
        assert_eq!(result, Err(VerificationError::SnapshotIdTooLong));
    }

    #[test]
    fn verification_serialization_is_deterministic() {
        let outcome = SnapshotOutcome::Process(ProcessSnapshotOutcome {
            expected_generation: 4,
            observed_generation: Some(4),
            alive: true,
        });
        let first =
            serde_json::to_string(&verify_snapshot("snapshot-process", &outcome).unwrap()).unwrap();
        let second =
            serde_json::to_string(&verify_snapshot("snapshot-process", &outcome).unwrap()).unwrap();
        assert_eq!(first, second);
    }
}
