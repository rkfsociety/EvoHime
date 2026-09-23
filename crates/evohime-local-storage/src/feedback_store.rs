//! Bounded local Feedback persistence contract (Этап 7, wave VII+).
//!
//! Mirrors the shape and bounds established by `memory_store.rs`: this file
//! keeps record bounds, redaction, and parameterized SQL together, and is
//! registered in `lib.rs`'s migration ladder (SCHEMA_VERSION 13).
//!
//! Feedback data is local-only by construction. Nothing in this module
//! sends data anywhere; see [`crate::feedback_store::external_telemetry_allowed`] for the single,
//! explicit, default-closed gate any *future* external telemetry sink must
//! call before it may read feedback rows. No such sink exists yet in this
//! codebase, and this module does not build one -- it only documents and
//! tests the boundary so a later change cannot silently cross it.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Maximum UTF-8 byte length of a feedback record identifier.
pub const MAX_ID_BYTES: usize = 256;
/// Maximum UTF-8 byte length of a run identifier.
pub const MAX_RUN_ID_BYTES: usize = 256;
/// Maximum UTF-8 byte length of an optional task identifier.
pub const MAX_TASK_ID_BYTES: usize = 256;
/// Maximum UTF-8 byte length of an optional subject reference.
pub const MAX_SUBJECT_REF_BYTES: usize = 256;
/// Maximum UTF-8 byte length of a correction.
pub const MAX_CORRECTION_BYTES: usize = 4 * 1024;
/// Maximum UTF-8 byte length of a rejection reason.
pub const MAX_REJECTION_REASON_BYTES: usize = 1024;
/// Maximum UTF-8 byte length of an outcome label.
pub const MAX_OUTCOME_BYTES: usize = 64;
/// Maximum UTF-8 byte length of provenance text.
pub const MAX_PROVENANCE_BYTES: usize = 2 * 1024;
/// Maximum UTF-8 byte length of the timestamp representation.
pub const MAX_TIMESTAMP_BYTES: usize = 64;

/// Useful/not-useful signal. Tri-state: an explicit "neutral" exists for
/// feedback that only carries a correction/rejection reason without a
/// direct usefulness judgement (e.g. a tool-result comment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackSignal {
    /// The user indicated that the associated result was useful.
    Useful,
    /// The user indicated that the associated result was not useful.
    NotUseful,
    /// Feedback was supplied without a direct usefulness judgment.
    Neutral,
}

impl FeedbackSignal {
    fn as_str(self) -> &'static str {
        match self {
            Self::Useful => "useful",
            Self::NotUseful => "not_useful",
            Self::Neutral => "neutral",
        }
    }

    fn parse(value: &str) -> Result<Self, FeedbackStoreError> {
        match value {
            "useful" => Ok(Self::Useful),
            "not_useful" => Ok(Self::NotUseful),
            "neutral" => Ok(Self::Neutral),
            _ => Err(FeedbackStoreError::InvalidSignal),
        }
    }
}

/// Outcome of the tool result / run this feedback is about, when known.
/// Bounded free text rather than a closed enum so new outcome kinds do not
/// require a migration; still length-capped and redacted like every other
/// text field here.
pub type FeedbackOutcome = Option<String>;

/// Validated user feedback tied to an existing run and optional subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackRecord {
    /// Unique identifier for the feedback row.
    pub id: String,
    /// Correlates to the existing `runs.id` / audit-trail `run_id` --
    /// feedback never invents a new correlation id.
    pub run_id: String,
    /// Optional task identifier associated with the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Existing tool-call / effect / approval identifier this feedback is
    /// about (e.g. `run_effects.effect_id` or an `ApprovalAuditEntry.approval_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<String>,
    /// Explicit usefulness signal.
    pub signal: FeedbackSignal,
    /// Optional correction text, redacted before persistence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<String>,
    /// Optional reason for rejecting or disapproving the result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
    /// e.g. "tool_succeeded", "tool_failed", "approval_granted", "approval_denied".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: FeedbackOutcome,
    /// Origin of the feedback, such as a UI or tool-result channel.
    pub provenance: String,
    /// Timestamp string supplied by the caller.
    pub created_at: String,
}

impl FeedbackRecord {
    /// Builds and validates a record, redacting sensitive patterns in free-text feedback.
    pub fn new(input: FeedbackRecordInput) -> Result<Self, FeedbackStoreError> {
        let record = Self {
            id: input.id,
            run_id: input.run_id,
            task_id: input.task_id,
            subject_ref: input.subject_ref,
            signal: input.signal,
            correction: input.correction.map(|value| redact_sensitive(&value)),
            rejection_reason: input.rejection_reason.map(|value| redact_sensitive(&value)),
            outcome: input.outcome,
            provenance: input.provenance,
            created_at: input.created_at,
        };
        record.validate()?;
        Ok(record)
    }

    /// Validates required identifiers and all configured byte-length limits.
    pub fn validate(&self) -> Result<(), FeedbackStoreError> {
        validate_required("id", &self.id, MAX_ID_BYTES)?;
        validate_required("run_id", &self.run_id, MAX_RUN_ID_BYTES)?;
        if let Some(task_id) = &self.task_id {
            validate_required("task_id", task_id, MAX_TASK_ID_BYTES)?;
        }
        if let Some(subject_ref) = &self.subject_ref {
            validate_required("subject_ref", subject_ref, MAX_SUBJECT_REF_BYTES)?;
        }
        if let Some(correction) = &self.correction {
            validate_bounded("correction", correction, MAX_CORRECTION_BYTES)?;
        }
        if let Some(rejection_reason) = &self.rejection_reason {
            validate_bounded(
                "rejection_reason",
                rejection_reason,
                MAX_REJECTION_REASON_BYTES,
            )?;
        }
        if let Some(outcome) = &self.outcome {
            validate_bounded("outcome", outcome, MAX_OUTCOME_BYTES)?;
        }
        validate_required("provenance", &self.provenance, MAX_PROVENANCE_BYTES)?;
        validate_required("created_at", &self.created_at, MAX_TIMESTAMP_BYTES)?;
        Ok(())
    }
}

/// Caller-provided fields used to construct a validated [`FeedbackRecord`].
#[derive(Debug, Clone)]
pub struct FeedbackRecordInput {
    /// Unique feedback identifier.
    pub id: String,
    /// Existing run identifier to which feedback belongs.
    pub run_id: String,
    /// Optional task identifier.
    pub task_id: Option<String>,
    /// Optional tool call, effect, or approval identifier discussed by the feedback.
    pub subject_ref: Option<String>,
    /// Usefulness judgment.
    pub signal: FeedbackSignal,
    /// Optional correction, redacted by [`FeedbackRecord::new`].
    pub correction: Option<String>,
    /// Optional rejection explanation, redacted by [`FeedbackRecord::new`].
    pub rejection_reason: Option<String>,
    /// Optional bounded outcome label.
    pub outcome: FeedbackOutcome,
    /// Source or channel that provided the feedback.
    pub provenance: String,
    /// Timestamp string supplied by the caller.
    pub created_at: String,
}

/// Validation and database errors produced by feedback persistence operations.
#[derive(Debug, thiserror::Error)]
pub enum FeedbackStoreError {
    /// A required field was empty.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the required field that was empty.
        field: &'static str,
    },
    /// A field exceeded its configured maximum byte length.
    #[error("{field} exceeds {max} bytes")]
    Limit {
        /// Name of the field that exceeded its bound.
        field: &'static str,
        /// Maximum allowed byte length.
        max: usize,
    },
    /// A persisted feedback signal did not match a known value.
    #[error("invalid feedback signal")]
    InvalidSignal,
    /// SQLite rejected a query or row conversion.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

fn validate_required(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), FeedbackStoreError> {
    if value.trim().is_empty() {
        return Err(FeedbackStoreError::Empty { field });
    }
    validate_bounded(field, value, max)
}

fn validate_bounded(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), FeedbackStoreError> {
    if value.len() > max {
        return Err(FeedbackStoreError::Limit { field, max });
    }
    Ok(())
}

/// Mirrors `memory_store::redact_sensitive`: strips bearer tokens, API
/// keys, and email-shaped tokens from free-text fields before they are
/// ever persisted.
fn redact_sensitive(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if token.contains('@')
                || lower.starts_with("bearer")
                || lower.starts_with("sk-")
                || lower.starts_with("ghp_")
                || lower.starts_with("github_pat_")
                || lower.starts_with("api_key=")
                || lower.starts_with("token=")
            {
                "[REDACTED]".to_owned()
            } else {
                token.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Single, explicit, default-closed gate for any external telemetry send of
/// feedback data. There is no external telemetry sink in this codebase
/// today; this function exists so that if/when one is built, it has one
/// obvious place to check, and so the "opt-in only" contract is testable
/// now rather than assumed later. Always call this immediately before any
/// prospective external send -- never cache its result.
/// Returns the explicit opt-in value used to guard any future external telemetry send.
///
/// The value is default-closed when callers pass `false`; callers must check it immediately
/// before sending and must not cache the result.
pub fn external_telemetry_allowed(opt_in_setting: bool) -> bool {
    opt_in_setting
}

/// Simple local aggregation: counts of feedback by signal, and rejection
/// reason frequency. Intentionally not a general analytics engine -- P1
/// scope only needs counts/group-by.
/// Local counts and ranked groups produced by [`FeedbackStoreSql::aggregate`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackAggregate {
    /// Number of records with [`FeedbackSignal::Useful`].
    pub useful_count: i64,
    /// Number of records with [`FeedbackSignal::NotUseful`].
    pub not_useful_count: i64,
    /// Number of records with [`FeedbackSignal::Neutral`].
    pub neutral_count: i64,
    /// (rejection_reason, count), highest count first, bounded to the
    /// caller-supplied limit.
    pub rejection_reasons: Vec<(String, i64)>,
    /// (outcome, count), highest count first.
    pub outcomes: Vec<(String, i64)>,
}

/// Parameterized SQL operations for feedback rows; schema creation remains external.
pub struct FeedbackStoreSql;

impl FeedbackStoreSql {
    /// Insert statement used by [`Self::insert`].
    pub const INSERT: &'static str = "INSERT INTO feedback_entries
        (id, run_id, task_id, subject_ref, signal, correction, rejection_reason,
         outcome, provenance, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";
    /// Lookup statement used by [`Self::get_by_id`].
    pub const SELECT_BY_ID: &'static str = "SELECT id, run_id, task_id, subject_ref, signal,
        correction, rejection_reason, outcome, provenance, created_at
        FROM feedback_entries WHERE id = ?1";
    /// Run listing statement used by [`Self::list_by_run`].
    pub const LIST_BY_RUN: &'static str = "SELECT id, run_id, task_id, subject_ref, signal,
        correction, rejection_reason, outcome, provenance, created_at
        FROM feedback_entries WHERE run_id = ?1 ORDER BY created_at DESC, id ASC LIMIT ?2";

    /// Validates and inserts one feedback record using parameterized SQL.
    pub fn insert(
        connection: &Connection,
        record: &FeedbackRecord,
    ) -> Result<(), FeedbackStoreError> {
        record.validate()?;
        connection.execute(
            Self::INSERT,
            params![
                record.id,
                record.run_id,
                record.task_id,
                record.subject_ref,
                record.signal.as_str(),
                record.correction,
                record.rejection_reason,
                record.outcome,
                record.provenance,
                record.created_at,
            ],
        )?;
        Ok(())
    }

    /// Retrieves a feedback record by its unique identifier.
    pub fn get_by_id(
        connection: &Connection,
        id: &str,
    ) -> Result<Option<FeedbackRecord>, FeedbackStoreError> {
        Ok(connection
            .query_row(Self::SELECT_BY_ID, params![id], map_record)
            .optional()?)
    }

    /// Lists feedback tied to one run, newest first.
    pub fn list_by_run(
        connection: &Connection,
        run_id: &str,
        limit: u32,
    ) -> Result<Vec<FeedbackRecord>, FeedbackStoreError> {
        validate_required("run_id", run_id, MAX_RUN_ID_BYTES)?;
        let mut statement = connection.prepare(Self::LIST_BY_RUN)?;
        let records = statement
            .query_map(params![run_id, i64::from(limit.clamp(1, 500))], map_record)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    /// Local aggregation: signal counts plus the top `reason_limit`
    /// rejection reasons and `outcome_limit` outcomes by frequency.
    pub fn aggregate(
        connection: &Connection,
        reason_limit: u32,
        outcome_limit: u32,
    ) -> Result<FeedbackAggregate, FeedbackStoreError> {
        let mut aggregate = FeedbackAggregate::default();
        let mut signal_statement =
            connection.prepare("SELECT signal, COUNT(*) FROM feedback_entries GROUP BY signal")?;
        let signal_rows = signal_statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (signal, count) in signal_rows {
            match FeedbackSignal::parse(&signal) {
                Ok(FeedbackSignal::Useful) => aggregate.useful_count = count,
                Ok(FeedbackSignal::NotUseful) => aggregate.not_useful_count = count,
                Ok(FeedbackSignal::Neutral) => aggregate.neutral_count = count,
                Err(_) => {}
            }
        }

        let mut reason_statement = connection.prepare(
            "SELECT rejection_reason, COUNT(*) FROM feedback_entries
             WHERE rejection_reason IS NOT NULL
             GROUP BY rejection_reason ORDER BY COUNT(*) DESC, rejection_reason ASC LIMIT ?1",
        )?;
        aggregate.rejection_reasons = reason_statement
            .query_map(params![i64::from(reason_limit.clamp(1, 100))], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut outcome_statement = connection.prepare(
            "SELECT outcome, COUNT(*) FROM feedback_entries
             WHERE outcome IS NOT NULL
             GROUP BY outcome ORDER BY COUNT(*) DESC, outcome ASC LIMIT ?1",
        )?;
        aggregate.outcomes = outcome_statement
            .query_map(params![i64::from(outcome_limit.clamp(1, 100))], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(aggregate)
    }
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackRecord> {
    Ok(FeedbackRecord {
        id: row.get(0)?,
        run_id: row.get(1)?,
        task_id: row.get(2)?,
        subject_ref: row.get(3)?,
        signal: FeedbackSignal::parse(&row.get::<_, String>(4)?).map_err(to_sql_error)?,
        correction: row.get(5)?,
        rejection_reason: row.get(6)?,
        outcome: row.get(7)?,
        provenance: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn to_sql_error(error: FeedbackStoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

use rusqlite::OptionalExtension;

#[cfg(test)]
#[path = "feedback_store_tests.rs"]
mod tests;
