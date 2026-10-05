//! Durable, metadata-only storage for scoped free-access evidence.
//!
//! The Core contract owns the semantic evidence. This store owns only its
//! bounded serialized snapshot and its revision fence; it never accepts raw
//! provider responses, prompts or credential material.
//!
//! ```
//! use evohime_local_storage::free_access_evidence_store::{
//!     install_schema, put, FreeAccessEvidenceRecord,
//! };
//! let connection = rusqlite::Connection::open_in_memory()?;
//! install_schema(&connection)?;
//! let published = put(&connection, &FreeAccessEvidenceRecord {
//!     provider_id: "provider-a".into(),
//!     model_id: "model-a".into(),
//!     credential_binding: "account-1".into(),
//!     region: "global".into(),
//!     revision: 1,
//!     content_hash: "a".repeat(64),
//!     evidence_json: br#"{"tier":"free"}"#.to_vec(),
//!     observed_at_ms: 1,
//!     expires_at_ms: 60_001,
//!     invalidation: None,
//! })?;
//! assert!(published);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use rusqlite::{params, Connection, OptionalExtension};

/// Maximum encoded metadata size for one evidence record.
pub const MAX_EVIDENCE_JSON_BYTES: usize = 128 * 1024;
/// Maximum number of distinct evidence scopes stored at once.
pub const MAX_EVIDENCE_ROWS: u32 = 2_048;
/// Maximum UTF-8 byte length of provider and model scope identifiers.
pub const MAX_SCOPE_TOKEN_BYTES: usize = 256;
/// Maximum UTF-8 byte length of a region identifier.
pub const MAX_REGION_BYTES: usize = 64;
/// Maximum UTF-8 byte length of a non-secret credential binding identifier.
pub const MAX_CREDENTIAL_BINDING_BYTES: usize = 128;

/// Bounded metadata snapshot for one provider/model/credential/region scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreeAccessEvidenceRecord {
    /// Provider identifier.
    pub provider_id: String,
    /// Model identifier within the provider.
    pub model_id: String,
    /// Opaque account binding; must not contain credential material.
    pub credential_binding: String,
    /// Provider region or region group.
    pub region: String,
    /// Positive, monotonically increasing revision for this scope.
    pub revision: i64,
    /// Lowercase or uppercase hexadecimal SHA-256 digest of the evidence.
    pub content_hash: String,
    /// Serialized, metadata-only evidence object.
    pub evidence_json: Vec<u8>,
    /// Observation time as Unix milliseconds.
    pub observed_at_ms: i64,
    /// Expiration time as Unix milliseconds, later than observation time.
    pub expires_at_ms: i64,
    /// Optional invalidation reason token.
    pub invalidation: Option<String>,
}

/// Creates the evidence table and indexes used for expiry and hash lookup.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS free_access_evidence (
           provider_id TEXT NOT NULL,
           model_id TEXT NOT NULL,
           credential_binding TEXT NOT NULL,
           region TEXT NOT NULL,
           revision INTEGER NOT NULL,
           content_hash TEXT NOT NULL,
           evidence_json BLOB NOT NULL,
           observed_at_ms INTEGER NOT NULL,
           expires_at_ms INTEGER NOT NULL,
           invalidation TEXT,
           PRIMARY KEY(provider_id, model_id, credential_binding, region)
         );
         CREATE INDEX IF NOT EXISTS idx_free_access_evidence_expiry
           ON free_access_evidence(expires_at_ms);
         CREATE INDEX IF NOT EXISTS idx_free_access_evidence_hash
           ON free_access_evidence(content_hash);",
    )
}

/// Stores one latest snapshot per provider/model/credential/region scope.
///
/// `true` means a new revision was published. Replaying the same revision or
/// submitting a gap/stale revision returns `false` without changing storage.
pub fn put(
    connection: &Connection,
    record: &FreeAccessEvidenceRecord,
) -> Result<bool, &'static str> {
    validate_record(record)?;
    let transaction = connection.unchecked_transaction().map_err(|_| "sqlite")?;
    let existing_revision = transaction
        .query_row(
            "SELECT revision FROM free_access_evidence
             WHERE provider_id=?1 AND model_id=?2 AND credential_binding=?3 AND region=?4",
            params![
                record.provider_id,
                record.model_id,
                record.credential_binding,
                record.region
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|_| "sqlite")?;

    match existing_revision {
        Some(current) if record.revision != current.saturating_add(1) => return Ok(false),
        None if record.revision != 1 => return Ok(false),
        _ => {}
    }

    if existing_revision.is_none() {
        let count = transaction
            .query_row("SELECT COUNT(*) FROM free_access_evidence", [], |row| {
                row.get::<_, u32>(0)
            })
            .map_err(|_| "sqlite")?;
        if count >= MAX_EVIDENCE_ROWS {
            return Err("free access evidence limit");
        }
    }

    let changed = transaction
        .execute(
            "INSERT INTO free_access_evidence
             (provider_id,model_id,credential_binding,region,revision,content_hash,
              evidence_json,observed_at_ms,expires_at_ms,invalidation)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(provider_id,model_id,credential_binding,region)
             DO UPDATE SET revision=excluded.revision,
               content_hash=excluded.content_hash,
               evidence_json=excluded.evidence_json,
               observed_at_ms=excluded.observed_at_ms,
               expires_at_ms=excluded.expires_at_ms,
               invalidation=excluded.invalidation
             WHERE excluded.revision = free_access_evidence.revision + 1",
            params![
                record.provider_id,
                record.model_id,
                record.credential_binding,
                record.region,
                record.revision,
                record.content_hash,
                record.evidence_json,
                record.observed_at_ms,
                record.expires_at_ms,
                record.invalidation,
            ],
        )
        .map(|changed| changed == 1)
        .map_err(|_| "sqlite")?;
    transaction.commit().map_err(|_| "sqlite")?;
    Ok(changed)
}

/// Loads one record for the complete provider, model, account, and region scope.
pub fn get(
    connection: &Connection,
    provider_id: &str,
    model_id: &str,
    credential_binding: &str,
    region: &str,
) -> rusqlite::Result<Option<FreeAccessEvidenceRecord>> {
    connection
        .query_row(
            "SELECT provider_id,model_id,credential_binding,region,revision,
                    content_hash,evidence_json,observed_at_ms,expires_at_ms,invalidation
             FROM free_access_evidence
             WHERE provider_id=?1 AND model_id=?2 AND credential_binding=?3 AND region=?4",
            params![provider_id, model_id, credential_binding, region],
            |row| {
                Ok(FreeAccessEvidenceRecord {
                    provider_id: row.get(0)?,
                    model_id: row.get(1)?,
                    credential_binding: row.get(2)?,
                    region: row.get(3)?,
                    revision: row.get(4)?,
                    content_hash: row.get(5)?,
                    evidence_json: row.get(6)?,
                    observed_at_ms: row.get(7)?,
                    expires_at_ms: row.get(8)?,
                    invalidation: row.get(9)?,
                })
            },
        )
        .optional()
}

/// Returns the number of stored scopes.
pub fn count(connection: &Connection) -> rusqlite::Result<u32> {
    connection.query_row("SELECT COUNT(*) FROM free_access_evidence", [], |row| {
        row.get(0)
    })
}

/// Deletes all evidence scoped to one credential binding after key rotation or removal.
pub fn delete_credential_scope(
    connection: &Connection,
    credential_binding: &str,
) -> Result<u32, &'static str> {
    if !valid_credential_binding(credential_binding) {
        return Err("invalid credential binding");
    }
    let transaction = connection.unchecked_transaction().map_err(|_| "sqlite")?;
    let deleted = transaction
        .execute(
            "DELETE FROM free_access_evidence WHERE credential_binding=?1",
            [credential_binding],
        )
        .map_err(|_| "sqlite")?;
    transaction.commit().map_err(|_| "sqlite")?;
    u32::try_from(deleted).map_err(|_| "sqlite")
}

/// Returns the most recent observation timestamp across all models in one credential scope.
pub fn latest_observed_at_for_credential(
    connection: &Connection,
    credential_binding: &str,
) -> rusqlite::Result<Option<i64>> {
    connection.query_row(
        "SELECT MAX(observed_at_ms) FROM free_access_evidence WHERE credential_binding=?1",
        [credential_binding],
        |row| row.get(0),
    )
}

fn validate_record(record: &FreeAccessEvidenceRecord) -> Result<(), &'static str> {
    if !valid_token(&record.provider_id, MAX_SCOPE_TOKEN_BYTES)
        || !valid_token(&record.model_id, MAX_SCOPE_TOKEN_BYTES)
        || !valid_credential_binding(&record.credential_binding)
        || !valid_token(&record.region, MAX_REGION_BYTES)
        || record.revision <= 0
        || !valid_hash(&record.content_hash)
        || record.evidence_json.is_empty()
        || record.evidence_json.len() > MAX_EVIDENCE_JSON_BYTES
        || record.observed_at_ms <= 0
        || record.expires_at_ms <= record.observed_at_ms
        || record
            .invalidation
            .as_deref()
            .is_some_and(|value| !valid_token(value, 128))
    {
        return Err("invalid free access evidence");
    }

    let value: serde_json::Value = serde_json::from_slice(&record.evidence_json)
        .map_err(|_| "invalid free access evidence")?;
    if !safe_metadata_json(&value, 0) {
        return Err("invalid free access evidence");
    }
    Ok(())
}

fn valid_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value == value.trim()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-/".contains(&byte))
}

fn valid_credential_binding(value: &str) -> bool {
    valid_token(value, MAX_CREDENTIAL_BINDING_BYTES)
        && !value.to_ascii_lowercase().contains("secret")
        && !value.to_ascii_lowercase().contains("bearer")
        && !value.to_ascii_lowercase().contains("token")
        && !value.to_ascii_lowercase().starts_with("sk-")
        && !value.to_ascii_lowercase().starts_with("gsk_")
        && !value.to_ascii_lowercase().starts_with("aiza")
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_metadata_json(value: &serde_json::Value, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => true,
        serde_json::Value::String(text) => {
            text.len() <= 512
                && !text.contains("http://")
                && !text.contains("https://")
                && !text.to_ascii_lowercase().contains("sk-")
        }
        serde_json::Value::Array(items) => {
            items.len() <= 64 && items.iter().all(|item| safe_metadata_json(item, depth + 1))
        }
        serde_json::Value::Object(fields) => {
            fields.len() <= 64
                && fields.iter().all(|(key, item)| {
                    let lower = key.to_ascii_lowercase();
                    !lower.contains("prompt")
                        && !lower.contains("secret")
                        && !lower.contains("password")
                        && !lower.contains("token")
                        && !lower.contains("header")
                        && !lower.contains("response")
                        && safe_metadata_json(item, depth + 1)
                })
        }
    }
}

#[cfg(test)]
#[path = "free_access_evidence_store_tests.rs"]
mod tests;
