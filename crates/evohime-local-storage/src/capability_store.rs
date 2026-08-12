//! Migration-neutral persistence contract for the bounded capability
//! registry catalog (roles/skills manifests).
//!
//! The module deliberately does not create or migrate tables. A caller owns
//! schema lifecycle and can apply these statements to an existing compatible
//! `capability_manifests` table. Keeping the SQL and record together lets
//! Core adopt the contract without coupling it to the storage migration
//! sequence, matching `research_store.rs` / `memory_store.rs`.
//!
//! This store persists the manifest catalog only: it does not download,
//! extract, verify signatures against, or execute any archive. Manifest
//! validation (bounds, hash format, registry-wide uniqueness, update
//! compatibility) is entirely owned by
//! `evohime_core::capability_registry`; this module only stores and
//! retrieves the resulting canonical JSON blob plus a few columns cheap
//! enough to list/filter on without deserializing every row.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const MAX_ID_BYTES: usize = 256;
const MAX_VERSION_BYTES: usize = 64;
const MAX_RISK_CLASS_BYTES: usize = 32;
const MAX_HASH_BYTES: usize = 128;
const MAX_MANIFEST_JSON_BYTES: usize = 256 * 1024;

/// Cheap classification of a manifest's dominant content, derived from
/// which of its `roles` / `skills` lists are non-empty. This is a listing
/// convenience only: `capability_registry::CapabilityManifest` itself makes
/// no such distinction (a manifest may carry both).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestKind {
    Role,
    Skill,
    Mixed,
}

impl ManifestKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::Skill => "skill",
            Self::Mixed => "mixed",
        }
    }

    fn parse(value: &str) -> Result<Self, CapabilityStoreError> {
        match value {
            "role" => Ok(Self::Role),
            "skill" => Ok(Self::Skill),
            "mixed" => Ok(Self::Mixed),
            _ => Err(CapabilityStoreError::InvalidKind),
        }
    }
}

/// One installed capability manifest row. `manifest_json` is the canonical
/// JSON encoding of a `capability_registry::CapabilityManifest`; the other
/// fields are denormalized copies kept only for cheap listing/filtering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityManifestRecord {
    /// The manifest name; also the registry's unique identity.
    pub id: String,
    pub kind: ManifestKind,
    pub version: String,
    pub risk_class: String,
    pub content_hash: String,
    pub manifest_json: String,
}

impl CapabilityManifestRecord {
    pub fn validate(&self) -> Result<(), CapabilityStoreError> {
        validate_text("id", &self.id, MAX_ID_BYTES)?;
        validate_text("version", &self.version, MAX_VERSION_BYTES)?;
        validate_text("risk_class", &self.risk_class, MAX_RISK_CLASS_BYTES)?;
        validate_text("content_hash", &self.content_hash, MAX_HASH_BYTES)?;
        validate_text(
            "manifest_json",
            &self.manifest_json,
            MAX_MANIFEST_JSON_BYTES,
        )?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CapabilityStoreError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: usize },
    #[error("invalid manifest kind")]
    InvalidKind,
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl PartialEq for CapabilityStoreError {
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
            (Self::InvalidKind, Self::InvalidKind) => true,
            _ => false,
        }
    }
}

impl Eq for CapabilityStoreError {}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), CapabilityStoreError> {
    if value.trim().is_empty() {
        return Err(CapabilityStoreError::Empty { field });
    }
    if value.len() > max_bytes {
        return Err(CapabilityStoreError::Limit {
            field,
            max: max_bytes,
        });
    }
    Ok(())
}

/// SQL contract only; schema creation and migrations remain outside this API.
pub struct CapabilityStoreSql;

impl CapabilityStoreSql {
    pub const INSERT_OR_REPLACE: &'static str = r#"
        INSERT INTO capability_manifests
            (id, kind, version, risk_class, content_hash, manifest_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(id) DO UPDATE SET
            kind = excluded.kind,
            version = excluded.version,
            risk_class = excluded.risk_class,
            content_hash = excluded.content_hash,
            manifest_json = excluded.manifest_json
    "#;

    pub const SELECT_BY_ID: &'static str = r#"
        SELECT id, kind, version, risk_class, content_hash, manifest_json
        FROM capability_manifests
        WHERE id = ?1
    "#;

    pub const SELECT_ALL: &'static str = r#"
        SELECT id, kind, version, risk_class, content_hash, manifest_json
        FROM capability_manifests
        ORDER BY installed_at DESC, id ASC
        LIMIT ?1
    "#;

    pub const DELETE_BY_ID: &'static str = "DELETE FROM capability_manifests WHERE id = ?1";

    /// Inserts a new manifest, or replaces the row of the same id (used for
    /// installing an update once the caller has already validated the
    /// update against the existing manifest via
    /// `capability_registry::validate_update`).
    pub fn insert(
        connection: &Connection,
        record: &CapabilityManifestRecord,
    ) -> Result<(), CapabilityStoreError> {
        record.validate()?;
        connection.execute(
            Self::INSERT_OR_REPLACE,
            params![
                record.id,
                record.kind.as_str(),
                record.version,
                record.risk_class,
                record.content_hash,
                record.manifest_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_by_id(
        connection: &Connection,
        id: &str,
    ) -> Result<Option<CapabilityManifestRecord>, CapabilityStoreError> {
        let record = connection
            .query_row(Self::SELECT_BY_ID, params![id], map_record)
            .optional()?;
        Ok(record)
    }

    /// Lists installed manifests, newest-first, bounded to at most 200 rows
    /// per call -- comfortably above `capability_registry::MAX_MANIFESTS`
    /// (128) so a caller can always fetch the full catalog in one call.
    /// Enforcing the exact registry bound is the caller's responsibility
    /// (via `capability_registry::validate_registry`); this bound only
    /// protects the query itself.
    pub fn list(
        connection: &Connection,
        limit: u32,
    ) -> Result<Vec<CapabilityManifestRecord>, CapabilityStoreError> {
        let mut statement = connection.prepare(Self::SELECT_ALL)?;
        let records = statement
            .query_map(params![i64::from(limit.clamp(1, 200))], map_record)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    pub fn delete_by_id(connection: &Connection, id: &str) -> Result<bool, CapabilityStoreError> {
        Ok(connection.execute(Self::DELETE_BY_ID, params![id])? == 1)
    }
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CapabilityManifestRecord> {
    Ok(CapabilityManifestRecord {
        id: row.get(0)?,
        kind: ManifestKind::parse(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
        version: row.get(2)?,
        risk_class: row.get(3)?,
        content_hash: row.get(4)?,
        manifest_json: row.get(5)?,
    })
}

fn to_sql_error(error: CapabilityStoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE capability_manifests (
                    id TEXT PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL,
                    version TEXT NOT NULL,
                    risk_class TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    manifest_json BLOB NOT NULL,
                    installed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );",
            )
            .expect("contract fixture creates");
    }

    fn record(id: &str, version: &str) -> CapabilityManifestRecord {
        CapabilityManifestRecord {
            id: id.into(),
            kind: ManifestKind::Role,
            version: version.into(),
            risk_class: "medium".into(),
            content_hash: "0123456789abcdef0123456789abcdef".into(),
            manifest_json: r#"{"name":"reviewer"}"#.into(),
        }
    }

    #[test]
    fn round_trips_record_without_schema_migration() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        let expected = record("reviewer", "1.0.0");

        CapabilityStoreSql::insert(&connection, &expected).expect("manifest inserts");

        assert_eq!(
            CapabilityStoreSql::get_by_id(&connection, "reviewer").expect("manifest reads"),
            Some(expected)
        );
    }

    #[test]
    fn insert_upserts_by_id_for_updates() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        CapabilityStoreSql::insert(&connection, &record("reviewer", "1.0.0")).expect("insert v1");
        CapabilityStoreSql::insert(&connection, &record("reviewer", "2.0.0")).expect("insert v2");

        let all = CapabilityStoreSql::list(&connection, 10).expect("manifests list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].version, "2.0.0");
    }

    #[test]
    fn lists_newest_first_and_deletes() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        CapabilityStoreSql::insert(&connection, &record("planner", "1.0.0")).expect("insert a");
        std::thread::sleep(std::time::Duration::from_millis(5));
        CapabilityStoreSql::insert(&connection, &record("reviewer", "1.0.0")).expect("insert b");

        let all = CapabilityStoreSql::list(&connection, 10).expect("manifests list");
        assert_eq!(
            all.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            ["reviewer", "planner"]
        );

        assert!(CapabilityStoreSql::delete_by_id(&connection, "planner").expect("delete"));
        assert!(!CapabilityStoreSql::delete_by_id(&connection, "missing").expect("missing delete"));
        assert_eq!(CapabilityStoreSql::list(&connection, 10).unwrap().len(), 1);
    }

    #[test]
    fn rejects_unbounded_or_empty_contract_fields_before_sql() {
        let mut invalid = record("reviewer", "1.0.0");
        invalid.manifest_json = "x".repeat(MAX_MANIFEST_JSON_BYTES + 1);
        assert!(matches!(
            invalid.validate(),
            Err(CapabilityStoreError::Limit {
                field: "manifest_json",
                ..
            })
        ));

        invalid = record("reviewer", "1.0.0");
        invalid.id.clear();
        assert_eq!(
            invalid.validate(),
            Err(CapabilityStoreError::Empty { field: "id" })
        );
    }
}
