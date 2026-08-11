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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchEvidenceRecord {
    pub id: String,
    pub source_kind: String,
    /// URL, local path, or another stable source locator.
    pub source_ref: String,
    pub redacted_excerpt: String,
    /// Hash of the redacted excerpt or canonical source payload.
    pub source_hash: String,
    /// RFC 3339 or another canonical UTC timestamp chosen by the caller.
    pub fetched_at: String,
    pub ttl_seconds: u64,
    /// Stable link to the task, run, or provenance record that used it.
    pub provenance_link: Option<String>,
}

impl ResearchEvidenceRecord {
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

#[derive(Debug, thiserror::Error)]
pub enum ResearchEvidenceError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: u64 },
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
pub struct ResearchEvidenceSql;

impl ResearchEvidenceSql {
    pub const INSERT: &'static str = r#"
        INSERT INTO research_evidence
            (id, source_kind, source_ref, redacted_excerpt, source_hash,
             fetched_at, ttl_seconds, provenance_link)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
    "#;

    pub const SELECT_BY_ID: &'static str = r#"
        SELECT id, source_kind, source_ref, redacted_excerpt, source_hash,
               fetched_at, ttl_seconds, provenance_link
        FROM research_evidence
        WHERE id = ?1
    "#;

    pub const SELECT_BY_PROVENANCE: &'static str = r#"
        SELECT id, source_kind, source_ref, redacted_excerpt, source_hash,
               fetched_at, ttl_seconds, provenance_link
        FROM research_evidence
        WHERE provenance_link = ?1
        ORDER BY id ASC
    "#;

    pub const DELETE_BY_ID: &'static str = "DELETE FROM research_evidence WHERE id = ?1";

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
                i64::try_from(record.ttl_seconds).expect("bounded TTL fits SQLite integer"),
                record.provenance_link,
            ],
        )?;
        Ok(())
    }

    pub fn get_by_id(
        connection: &Connection,
        id: &str,
    ) -> Result<Option<ResearchEvidenceRecord>, ResearchEvidenceError> {
        let record = connection
            .query_row(Self::SELECT_BY_ID, params![id], map_record)
            .optional()?;
        Ok(record)
    }

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
mod tests {
    use super::*;

    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE research_evidence (
                    id TEXT PRIMARY KEY NOT NULL,
                    source_kind TEXT NOT NULL,
                    source_ref TEXT NOT NULL,
                    redacted_excerpt TEXT NOT NULL,
                    source_hash TEXT NOT NULL,
                    fetched_at TEXT NOT NULL,
                    ttl_seconds INTEGER NOT NULL,
                    provenance_link TEXT
                );",
            )
            .expect("contract fixture creates");
    }

    fn record(id: &str, provenance_link: Option<&str>) -> ResearchEvidenceRecord {
        ResearchEvidenceRecord {
            id: id.into(),
            source_kind: "url".into(),
            source_ref: "https://example.test/source".into(),
            redacted_excerpt: "redacted result".into(),
            source_hash: "sha256:abc".into(),
            fetched_at: "2026-08-12T10:00:00Z".into(),
            ttl_seconds: 3600,
            provenance_link: provenance_link.map(str::to_owned),
        }
    }

    #[test]
    fn round_trips_record_without_schema_migration() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        let expected = record("evidence-1", Some("run:1"));

        ResearchEvidenceSql::insert(&connection, &expected).expect("evidence inserts");

        assert_eq!(
            ResearchEvidenceSql::get_by_id(&connection, "evidence-1").expect("evidence reads"),
            Some(expected)
        );
    }

    #[test]
    fn lists_by_provenance_in_deterministic_order_and_deletes() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        ResearchEvidenceSql::insert(&connection, &record("b", Some("task:1")))
            .expect("first evidence inserts");
        ResearchEvidenceSql::insert(&connection, &record("a", Some("task:1")))
            .expect("second evidence inserts");
        ResearchEvidenceSql::insert(&connection, &record("c", Some("task:2")))
            .expect("third evidence inserts");

        let records =
            ResearchEvidenceSql::list_by_provenance(&connection, "task:1").expect("evidence lists");
        assert_eq!(
            records
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert!(ResearchEvidenceSql::delete_by_id(&connection, "a").expect("evidence deletes"));
        assert!(!ResearchEvidenceSql::delete_by_id(&connection, "missing").expect("missing delete"));
    }

    #[test]
    fn rejects_unbounded_or_empty_contract_fields_before_sql() {
        let mut invalid = record("evidence-1", None);
        invalid.redacted_excerpt = "x".repeat(MAX_EXCERPT_BYTES + 1);
        assert!(matches!(
            invalid.validate(),
            Err(ResearchEvidenceError::Limit {
                field: "redacted_excerpt",
                ..
            })
        ));

        invalid = record("evidence-1", None);
        invalid.source_kind.clear();
        assert_eq!(
            invalid.validate(),
            Err(ResearchEvidenceError::Empty {
                field: "source_kind"
            })
        );

        invalid = record("evidence-1", None);
        invalid.ttl_seconds = MAX_TTL_SECONDS + 1;
        assert!(matches!(
            invalid.validate(),
            Err(ResearchEvidenceError::Limit {
                field: "ttl_seconds",
                ..
            })
        ));
    }
}
