//! Public data contract and validation rules for ambient storage.

use evohime_listener_contract::{ExtractionState, ProposalKind, ProposalState};

/// Maximum UTF-8 byte length for ambient record identifiers.
pub const MAX_ID_BYTES: usize = 256;
/// Maximum UTF-8 byte length for serialized timestamps.
pub const MAX_TIMESTAMP_BYTES: usize = 64;
/// Maximum UTF-8 byte length for utterance text.
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
/// Maximum UTF-8 byte length for a content hash.
pub const MAX_HASH_BYTES: usize = 128;
/// Maximum UTF-8 byte length for a language tag.
pub const MAX_LANGUAGE_BYTES: usize = 32;
/// Maximum number of rows returned by one ambient-storage read.
pub const MAX_ROWS_PER_READ: usize = 500;
/// Speaker marker used when speaker identity has not been verified.
pub const SPEAKER_UNVERIFIED: &str = "unverified";
/// Removal reason for an explicit user request.
pub const REASON_USER_REQUEST: &str = "user_request";
/// Removal reason for retention-policy cleanup.
pub const REASON_RETENTION: &str = "retention";
/// Removal reason for expiration of the forget window.
pub const REASON_FORGET_WINDOW: &str = "forget_window";
const REASONS: [&str; 3] = [REASON_USER_REQUEST, REASON_RETENTION, REASON_FORGET_WINDOW];
/// Rejection reason applied to a candidate whose source episode was deleted.
pub const CANDIDATE_REJECTION_REASON: &str = "source_deleted";
/// Identifier of the ambient proactivity profile.
pub const PROACTIVITY_PROFILE_ID: &str = "ambient";
/// Maximum UTF-8 byte length for ambient proposal subjects and titles.
pub const MAX_TITLE_BYTES: usize = 2 * 1024;

/// Validation and persistence errors for ambient records.
#[derive(Debug, thiserror::Error)]
pub enum AmbientStoreError {
    /// A required string field was empty or whitespace-only.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the empty field.
        field: &'static str,
    },
    /// A string field exceeded its byte limit.
    #[error("{field} exceeds {max} bytes")]
    Limit {
        /// Name of the field that exceeded its limit.
        field: &'static str,
        /// Maximum permitted byte length.
        max: usize,
    },
    /// The record used a speaker marker other than [`SPEAKER_UNVERIFIED`].
    #[error("ambient v1 stores only unverified speakers")]
    InvalidSpeaker,
    /// A newly created episode supplied nonzero initial counters.
    #[error("ambient episodes must start with zero counters")]
    InvalidInitialCounters,
    /// A removal reason is not one of the supported [`REASON_USER_REQUEST`], [`REASON_RETENTION`], or [`REASON_FORGET_WINDOW`] values.
    #[error("unknown removal reason")]
    InvalidReason,
    /// A counter or duration field was negative.
    #[error("{field} must not be negative")]
    Negative {
        /// Name of the negative field.
        field: &'static str,
    },
    /// The underlying SQLite operation failed.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Source-deletion timestamp and reason were supplied inconsistently.
    #[error("a proposal may only carry both source_deleted fields or neither")]
    InvalidSourceDeletion,
    /// A new proposal did not start in the proposed state.
    #[error("a new proposal must start in the proposed state")]
    InvalidInitialState,
}

/// A retained ambient capture episode and its expiration metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientEpisodeRecord {
    /// Stable identifier of the episode.
    pub episode_id: String,
    /// Timestamp at which capture began.
    pub started_at: String,
    /// Timestamp at which capture ended, if it has ended.
    pub ended_at: Option<String>,
    /// Number of utterances recorded in the episode.
    pub utterance_count: i64,
    /// Total speech duration in milliseconds.
    pub speech_ms: i64,
    /// Version of the speech-recognition engine used.
    pub engine_version: String,
    /// Identifier of the recognition model used.
    pub model_id: String,
    /// Extraction lifecycle state for the episode.
    pub extraction_state: ExtractionState,
    /// Timestamp after which retention may remove the episode.
    pub expires_at: String,
}

/// A single timestamped utterance retained within an ambient episode.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientUtteranceRecord {
    /// Stable identifier of the utterance.
    pub utterance_id: String,
    /// Identifier of the containing episode.
    pub episode_id: String,
    /// Zero-based or store-assigned position within the episode.
    pub sequence: i64,
    /// Timestamp at which the utterance began.
    pub started_at: String,
    /// Utterance duration in milliseconds.
    pub duration_ms: i64,
    /// Recognized utterance text.
    pub text: String,
    /// Hash of the recognized text used for integrity checks.
    pub text_hash: String,
    /// Language tag associated with the recognized text.
    pub language: String,
    /// Recognition engine average log probability.
    pub avg_logprob: f64,
    /// Speaker marker; ambient v1 accepts only [`SPEAKER_UNVERIFIED`].
    pub speaker: String,
    /// Whether the utterance text has been redacted.
    pub redacted: bool,
    /// Timestamp after which retention may remove the utterance.
    pub expires_at: String,
}

/// A tombstone recording that an ambient episode was removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbientTombstoneRecord {
    /// Stable identifier of the tombstone.
    pub tombstone_id: String,
    /// Identifier of the removed episode.
    pub episode_id: String,
    /// Timestamp at which removal occurred.
    pub removed_at: String,
    /// Supported reason for removal.
    pub reason: String,
    /// Number of utterances removed with the episode.
    pub utterance_count: i64,
    /// Timestamp after which the tombstone itself may be purged.
    pub expires_at: String,
}

/// A proposed follow-up derived from retained ambient context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbientProposalRecord {
    /// Stable identifier of the proposal.
    pub proposal_id: String,
    /// Deduplication key for equivalent proposals.
    pub proposal_key: String,
    /// Key used to suppress proposals muted by the user.
    pub mute_key: String,
    /// Category of proposed follow-up.
    pub kind: ProposalKind,
    /// Stable key of the proposal subject.
    pub subject_key: String,
    /// Human-readable proposal subject.
    pub subject: String,
    /// Short display title.
    pub title: String,
    /// Source episode identifier when the source is retained.
    pub source_episode_id: Option<String>,
    /// Deletion timestamp when the source episode was deleted.
    pub source_deleted_at: Option<String>,
    /// Deletion reason when the source episode was deleted.
    pub source_deleted_reason: Option<String>,
    /// Creation timestamp.
    pub created_at: String,
    /// Most recent update timestamp.
    pub updated_at: String,
    /// Expiration timestamp.
    pub expires_at: String,
    /// Number of equivalent observations merged into this proposal.
    pub occurrences: i64,
    /// Current lifecycle state of the proposal.
    pub state: ProposalState,
    /// Accepted task identifier, if the proposal was accepted.
    pub accepted_task_id: Option<String>,
    /// Idempotency key associated with proposal creation, if supplied.
    pub idempotency_key: Option<String>,
}

/// Outcome of attempting to insert an ambient proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalInsert {
    /// A new proposal was created.
    Created,
    /// An equivalent proposal already existed and its occurrence count was updated.
    Duplicate {
        /// Identifier of the existing proposal.
        proposal_id: String,
        /// Updated number of observations represented by the proposal.
        occurrences: i64,
    },
    /// The proposal matched a user mute rule and was not inserted.
    Muted,
}

/// Persisted counters used to enforce hourly and daily proactivity limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProactivityCountersRow {
    /// Start of the current accounting hour in Unix milliseconds.
    pub hour_started_at_ms: i64,
    /// Number of proposals made in the current hour.
    pub hour_count: i64,
    /// Start of the current accounting day in Unix milliseconds.
    pub day_started_at_ms: i64,
    /// Number of proposals made in the current day.
    pub day_count: i64,
    /// Timestamp of the most recent proposal, if any.
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

/// Counts of records removed or rejected by an episode deletion operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AmbientDeletion {
    /// Number of episodes removed.
    pub episodes_removed: usize,
    /// Number of utterances removed.
    pub utterances_removed: usize,
    /// Number of tombstones written.
    pub tombstones_written: usize,
    /// Number of events removed.
    pub events_removed: usize,
    /// Number of derived candidates rejected.
    pub candidates_rejected: usize,
    /// Number of expired proposals affected.
    pub proposals_expired: usize,
}

/// Counts of records removed, rejected, or expired by an ambient retention purge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AmbientPurge {
    /// Number of episodes removed.
    pub episodes_removed: usize,
    /// Number of utterances removed.
    pub utterances_removed: usize,
    /// Number of tombstones written for removed episodes.
    pub tombstones_written: usize,
    /// Number of expired tombstones removed.
    pub tombstones_removed: usize,
    /// Number of events removed.
    pub events_removed: usize,
    /// Number of derived candidates rejected.
    pub candidates_rejected: usize,
    /// Number of proposals marked expired.
    pub proposals_expired: usize,
    /// Number of expired proposals removed.
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
