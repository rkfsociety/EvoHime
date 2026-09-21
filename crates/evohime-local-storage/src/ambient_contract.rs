//! Public data contract and validation rules for ambient storage.

use evohime_listener_contract::{ExtractionState, ProposalKind, ProposalState};

pub const MAX_ID_BYTES: usize = 256;
pub const MAX_TIMESTAMP_BYTES: usize = 64;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_HASH_BYTES: usize = 128;
pub const MAX_LANGUAGE_BYTES: usize = 32;
pub const MAX_ROWS_PER_READ: usize = 500;
pub const SPEAKER_UNVERIFIED: &str = "unverified";
pub const REASON_USER_REQUEST: &str = "user_request";
pub const REASON_RETENTION: &str = "retention";
pub const REASON_FORGET_WINDOW: &str = "forget_window";
const REASONS: [&str; 3] = [REASON_USER_REQUEST, REASON_RETENTION, REASON_FORGET_WINDOW];
pub const CANDIDATE_REJECTION_REASON: &str = "source_deleted";
pub const PROACTIVITY_PROFILE_ID: &str = "ambient";
pub const MAX_TITLE_BYTES: usize = 2 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum AmbientStoreError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: usize },
    #[error("ambient v1 stores only unverified speakers")]
    InvalidSpeaker,
    #[error("ambient episodes must start with zero counters")]
    InvalidInitialCounters,
    #[error("unknown removal reason")]
    InvalidReason,
    #[error("{field} must not be negative")]
    Negative { field: &'static str },
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("a proposal may only carry both source_deleted fields or neither")]
    InvalidSourceDeletion,
    #[error("a new proposal must start in the proposed state")]
    InvalidInitialState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmbientEpisodeRecord {
    pub episode_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub utterance_count: i64,
    pub speech_ms: i64,
    pub engine_version: String,
    pub model_id: String,
    pub extraction_state: ExtractionState,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmbientUtteranceRecord {
    pub utterance_id: String,
    pub episode_id: String,
    pub sequence: i64,
    pub started_at: String,
    pub duration_ms: i64,
    pub text: String,
    pub text_hash: String,
    pub language: String,
    pub avg_logprob: f64,
    pub speaker: String,
    pub redacted: bool,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbientTombstoneRecord {
    pub tombstone_id: String,
    pub episode_id: String,
    pub removed_at: String,
    pub reason: String,
    pub utterance_count: i64,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbientProposalRecord {
    pub proposal_id: String,
    pub proposal_key: String,
    pub mute_key: String,
    pub kind: ProposalKind,
    pub subject_key: String,
    pub subject: String,
    pub title: String,
    pub source_episode_id: Option<String>,
    pub source_deleted_at: Option<String>,
    pub source_deleted_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    pub occurrences: i64,
    pub state: ProposalState,
    pub accepted_task_id: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalInsert {
    Created,
    Duplicate {
        proposal_id: String,
        occurrences: i64,
    },
    Muted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProactivityCountersRow {
    pub hour_started_at_ms: i64,
    pub hour_count: i64,
    pub day_started_at_ms: i64,
    pub day_count: i64,
    pub last_proposed_at_ms: Option<i64>,
}

impl AmbientProposalRecord {
    pub(crate) fn validate(&self) -> Result<(), AmbientStoreError> {
        validate_required("proposal_id", &self.proposal_id, MAX_ID_BYTES)?;
        validate_required("proposal_key", &self.proposal_key, MAX_ID_BYTES)?;
        validate_required("mute_key", &self.mute_key, MAX_ID_BYTES)?;
        validate_required("subject_key", &self.subject_key, MAX_ID_BYTES)?;
        validate_required("subject", &self.subject, MAX_TITLE_BYTES)?;
        validate_required("title", &self.title, MAX_TITLE_BYTES)?;
        validate_required("created_at", &self.created_at, MAX_TIMESTAMP_BYTES)?;
        validate_required("updated_at", &self.updated_at, MAX_TIMESTAMP_BYTES)?;
        validate_required("expires_at", &self.expires_at, MAX_TIMESTAMP_BYTES)?;
        if let Some(episode_id) = &self.source_episode_id {
            validate_required("source_episode_id", episode_id, MAX_ID_BYTES)?;
        }
        if self.source_deleted_at.is_some() != self.source_deleted_reason.is_some() {
            return Err(AmbientStoreError::InvalidSourceDeletion);
        }
        validate_non_negative("occurrences", self.occurrences)?;
        if self.state != ProposalState::Proposed {
            return Err(AmbientStoreError::InvalidInitialState);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AmbientDeletion {
    pub episodes_removed: usize,
    pub utterances_removed: usize,
    pub tombstones_written: usize,
    pub events_removed: usize,
    pub candidates_rejected: usize,
    pub proposals_expired: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AmbientPurge {
    pub episodes_removed: usize,
    pub utterances_removed: usize,
    pub tombstones_written: usize,
    pub tombstones_removed: usize,
    pub events_removed: usize,
    pub candidates_rejected: usize,
    pub proposals_expired: usize,
    pub proposals_removed: usize,
}

impl AmbientEpisodeRecord {
    pub(crate) fn validate(&self) -> Result<(), AmbientStoreError> {
        validate_required("episode_id", &self.episode_id, MAX_ID_BYTES)?;
        validate_required("started_at", &self.started_at, MAX_TIMESTAMP_BYTES)?;
        if let Some(ended_at) = &self.ended_at {
            validate_required("ended_at", ended_at, MAX_TIMESTAMP_BYTES)?;
        }
        validate_required("engine_version", &self.engine_version, MAX_ID_BYTES)?;
        validate_required("model_id", &self.model_id, MAX_ID_BYTES)?;
        validate_required("expires_at", &self.expires_at, MAX_TIMESTAMP_BYTES)?;
        validate_non_negative("utterance_count", self.utterance_count)?;
        validate_non_negative("speech_ms", self.speech_ms)?;
        Ok(())
    }
}

impl AmbientUtteranceRecord {
    pub(crate) fn validate(&self) -> Result<(), AmbientStoreError> {
        validate_required("utterance_id", &self.utterance_id, MAX_ID_BYTES)?;
        validate_required("episode_id", &self.episode_id, MAX_ID_BYTES)?;
        validate_required("started_at", &self.started_at, MAX_TIMESTAMP_BYTES)?;
        validate_required("text", &self.text, MAX_TEXT_BYTES)?;
        validate_required("text_hash", &self.text_hash, MAX_HASH_BYTES)?;
        validate_required("language", &self.language, MAX_LANGUAGE_BYTES)?;
        validate_required("expires_at", &self.expires_at, MAX_TIMESTAMP_BYTES)?;
        validate_non_negative("sequence", self.sequence)?;
        validate_non_negative("duration_ms", self.duration_ms)?;
        if self.speaker != SPEAKER_UNVERIFIED {
            return Err(AmbientStoreError::InvalidSpeaker);
        }
        Ok(())
    }
}

pub(crate) fn validate_required(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), AmbientStoreError> {
    if value.trim().is_empty() {
        return Err(AmbientStoreError::Empty { field });
    }
    if value.len() > max {
        return Err(AmbientStoreError::Limit { field, max });
    }
    Ok(())
}

pub(crate) fn validate_non_negative(
    field: &'static str,
    value: i64,
) -> Result<(), AmbientStoreError> {
    if value < 0 {
        return Err(AmbientStoreError::Negative { field });
    }
    Ok(())
}

pub(crate) fn validate_reason(reason: &str) -> Result<(), AmbientStoreError> {
    if REASONS.contains(&reason) {
        Ok(())
    } else {
        Err(AmbientStoreError::InvalidReason)
    }
}
