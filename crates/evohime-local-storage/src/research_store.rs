//! Migration-neutral persistence contract for bounded research evidence.
//!
//! The module deliberately does not create or migrate tables.  A caller owns
//! schema lifecycle and can apply these statements to an existing compatible
//! table.  Keeping the SQL and record together lets Core adopt the contract
//! without coupling it to the storage migration sequence.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const MAX_ID_BYTES: usize = 256;
const MAX_SOURCE_KIND_BYTES: usize = 64;
const MAX_SOURCE_REF_BYTES: usize = 8 * 1024;
const MAX_EXCERPT_BYTES: usize = 64 * 1024;
const MAX_HASH_BYTES: usize = 128;
const MAX_TIMESTAMP_BYTES: usize = 64;
const MAX_PROVENANCE_LINK_BYTES: usize = 8 * 1024;
const MAX_TTL_SECONDS: u64 = 31 * 24 * 60 * 60;

/// A redacted, bounded piece of research evidence.
///
/// ```
/// use evohime_local_storage::research_store::ResearchEvidenceRecord;
///
/// let evidence = ResearchEvidenceRecord {
///     id: "evidence-1".into(),
///     source_kind: "web".into(),
///     source_ref: "https://example.invalid/spec".into(),
///     redacted_excerpt: "The API returns a bounded result.".into(),
///     source_hash: "sha256:abc123".into(),
///     fetched_at: "2026-09-23T00:00:00Z".into(),
///     ttl_seconds: 3600,
///     provenance_link: Some("task-42".into()),
/// };
/// assert!(evidence.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchEvidenceRecord {
    /// Stable identifier unique within the research evidence table.
    pub id: String,
    /// Kind of source, such as a web page or repository document.
    pub source_kind: String,
    /// URL, local path, or another stable source locator.
    pub source_ref: String,
    /// Excerpt stored after sensitive content has been redacted.
    pub redacted_excerpt: String,
    /// Hash of the redacted excerpt or canonical source payload.
    pub source_hash: String,
    /// RFC 3339 or another canonical UTC timestamp chosen by the caller.
    pub fetched_at: String,
    /// Maximum age of this evidence in seconds; limited to 31 days.
    pub ttl_seconds: u64,
    /// Stable link to the task, run, or provenance record that used it.
    pub provenance_link: Option<String>,
}

impl ResearchEvidenceRecord {
    /// Checks required text fields, byte limits, and the maximum evidence lifetime.
    pub fn validate(&self) -> Result<(), ResearchEvidenceError> {
        validate_text("id", &self.id, MAX_ID_BYTES)?;
        validate_text("source_kind", &self.source_kind, MAX_SOURCE_KIND_BYTES)?;
        validate_text("source_ref", &self.source_ref, MAX_SOURCE_REF_BYTES)?;
        validate_text(
            "redacted_excerpt",
            &self.redacted_excerpt,
            MAX_EXCERPT_BYTES,
        )?;
        validate_text("source_hash", &self.source_hash, MAX_HASH_BYTES)?;
        validate_text("fetched_at", &self.fetched_at, MAX_TIMESTAMP_BYTES)?;
        if self.ttl_seconds > MAX_TTL_SECONDS {
            return Err(ResearchEvidenceError::Limit {
                field: "ttl_seconds",
                max: MAX_TTL_SECONDS,
            });
        }
        if let Some(link) = &self.provenance_link {
            validate_text("provenance_link", link, MAX_PROVENANCE_LINK_BYTES)?;
        }
        Ok(())
    }
}

/// Validation and SQLite failures from the research evidence contract.
#[derive(Debug, thiserror::Error)]
pub enum ResearchEvidenceError {
    /// A required field is empty or whitespace-only.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the empty field.
        field: &'static str,
    },
    /// A field exceeds the byte limit shown in `max`.
    #[error("{field} exceeds {max} bytes")]
    Limit {
        /// Name of the field that exceeded its limit.
        field: &'static str,
        /// Maximum permitted size in bytes or lifetime in seconds.
        max: u64,
    },
    /// The underlying SQLite operation failed.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl PartialEq for ResearchEvidenceError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty { field: left }, Self::Empty { field: right }) => left == right,
            (
                Self::Limit {
                    field: left,
                    max: left_max,
                },
                Self::Limit {
                    field: right,
                    max: right_max,
                },
            ) => left == right && left_max == right_max,
            _ => false,
        }
    }
}

impl Eq for ResearchEvidenceError {}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), ResearchEvidenceError> {
    if value.trim().is_empty() {
        return Err(ResearchEvidenceError::Empty { field });
    }
    if value.len() > max_bytes {
        return Err(ResearchEvidenceError::Limit {
            field,
            max: max_bytes as u64,
        });
    }
    Ok(())
}

/// SQL contract only; schema creation and migrations remain outside this API.
///
/// The caller must create a compatible `research_evidence` table before use.
pub struct ResearchEvidenceSql;

impl ResearchEvidenceSql {
    /// SQL statement that inserts a validated evidence record.
    pub const INSERT: &'static str = r#"
        INSERT INTO research_evidence
            (id, source_kind, source_ref, redacted_excerpt, source_hash,
             fetched_at, ttl_seconds, provenance_link)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
    "#;

    /// SQL statement that selects one evidence record by identifier.
    pub const SELECT_BY_ID: &'static str = r#"
        SELECT id, source_kind, source_ref, redacted_excerpt, source_hash,
               fetched_at, ttl_seconds, provenance_link
        FROM research_evidence
        WHERE id = ?1
    "#;

    /// SQL statement that selects records for one provenance link in identifier order.
    pub const SELECT_BY_PROVENANCE: &'static str = r#"
        SELECT id, source_kind, source_ref, redacted_excerpt, source_hash,
               fetched_at, ttl_seconds, provenance_link
        FROM research_evidence
        WHERE provenance_link = ?1
        ORDER BY id ASC
    "#;

    /// SQL statement that deletes a record by identifier.
    pub const DELETE_BY_ID: &'static str = "DELETE FROM research_evidence WHERE id = ?1";

    /// Validates and inserts a record using the caller-provided schema.
    pub fn insert(
        connection: &Connection,
        record: &ResearchEvidenceRecord,
    ) -> Result<(), ResearchEvidenceError> {
        record.validate()?;
        connection.execute(
            Self::INSERT,
            params![
                record.id,
                record.source_kind,
                record.source_ref,
                record.redacted_excerpt,
                record.source_hash,
                record.fetched_at,
                i64::try_from(record.ttl_seconds).map_err(|_| ResearchEvidenceError::Limit {
                    field: "ttl_seconds",
                    max: MAX_TTL_SECONDS,
                })?,
                record.provenance_link,
            ],
        )?;
        Ok(())
    }

    /// Returns the matching record, or `None` when the identifier is absent.
    pub fn get_by_id(
        connection: &Connection,
        id: &str,
    ) -> Result<Option<ResearchEvidenceRecord>, ResearchEvidenceError> {
        let record = connection
            .query_row(Self::SELECT_BY_ID, params![id], map_record)
            .optional()?;
        Ok(record)
    }

    /// Lists records associated with a provenance link in stable identifier order.
    pub fn list_by_provenance(
        connection: &Connection,
        provenance_link: &str,
    ) -> Result<Vec<ResearchEvidenceRecord>, ResearchEvidenceError> {
        let mut statement = connection.prepare(Self::SELECT_BY_PROVENANCE)?;
        let records = statement
            .query_map(params![provenance_link], map_record)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    /// Deletes a record and reports whether one row was removed.
    pub fn delete_by_id(connection: &Connection, id: &str) -> Result<bool, ResearchEvidenceError> {
        Ok(connection.execute(Self::DELETE_BY_ID, params![id])? == 1)
    }
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchEvidenceRecord> {
    let ttl_seconds: i64 = row.get(6)?;
    Ok(ResearchEvidenceRecord {
        id: row.get(0)?,
        source_kind: row.get(1)?,
        source_ref: row.get(2)?,
        redacted_excerpt: row.get(3)?,
        source_hash: row.get(4)?,
        fetched_at: row.get(5)?,
        ttl_seconds: u64::try_from(ttl_seconds).map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Integer,
                "ttl_seconds must be non-negative".into(),
            )
        })?,
        provenance_link: row.get(7)?,
    })
}

#[cfg(test)]
#[path = "research_store_tests.rs"]
mod tests;
