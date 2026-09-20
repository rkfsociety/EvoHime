//! Durable, metadata-only storage for scoped free-access evidence.
//!
//! The Core contract owns the semantic evidence. This store owns only its
//! bounded serialized snapshot and its revision fence; it never accepts raw
//! provider responses, prompts or credential material.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_EVIDENCE_JSON_BYTES: usize = 128 * 1024;
pub const MAX_EVIDENCE_ROWS: u32 = 2_048;
pub const MAX_SCOPE_TOKEN_BYTES: usize = 256;
pub const MAX_REGION_BYTES: usize = 64;
pub const MAX_CREDENTIAL_BINDING_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreeAccessEvidenceRecord {
    pub provider_id: String,
    pub model_id: String,
    pub credential_binding: String,
    pub region: String,
    pub revision: i64,
    pub content_hash: String,
    pub evidence_json: Vec<u8>,
    pub observed_at_ms: i64,
    pub expires_at_ms: i64,
    pub invalidation: Option<String>,
}

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

pub fn count(connection: &Connection) -> rusqlite::Result<u32> {
    connection.query_row("SELECT COUNT(*) FROM free_access_evidence", [], |row| {
        row.get(0)
    })
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
mod tests {
    use super::*;

    fn record(scope: &str, revision: i64) -> FreeAccessEvidenceRecord {
        FreeAccessEvidenceRecord {
            provider_id: "openrouter".into(),
            model_id: "provider/model:free".into(),
            credential_binding: scope.into(),
            region: "global".into(),
            revision,
            content_hash: "a".repeat(64),
            evidence_json: br#"{"observed_state":"verified_free_limited"}"#.to_vec(),
            observed_at_ms: 1_000,
            expires_at_ms: 2_000,
            invalidation: None,
        }
    }

    #[test]
    fn scoped_revision_write_is_fenced_and_idempotent() {
        let connection = Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema");
        assert!(put(&connection, &record("cred:a", 1)).expect("first write"));
        assert!(!put(&connection, &record("cred:a", 1)).expect("duplicate write"));
        assert!(!put(&connection, &record("cred:a", 3)).expect("revision gap"));
        assert!(put(&connection, &record("cred:a", 2)).expect("next revision"));
        assert!(put(&connection, &record("cred:b", 1)).expect("other scope"));
        assert_eq!(count(&connection).expect("count"), 2);
    }

    #[test]
    fn raw_provider_material_is_rejected_before_storage() {
        let connection = Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema");
        let mut unsafe_record = record("cred:a", 1);
        unsafe_record.evidence_json =
            br#"{"provider_response":"https://provider.test","prompt":"private"}"#.to_vec();
        assert_eq!(
            put(&connection, &unsafe_record),
            Err("invalid free access evidence")
        );
        assert_eq!(count(&connection).expect("count"), 0);
    }

    #[test]
    fn secret_like_scope_is_rejected() {
        let connection = Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema");
        let unsafe_record = record("sk-live-secret", 1);
        assert_eq!(
            put(&connection, &unsafe_record),
            Err("invalid free access evidence")
        );
    }
}
