//! Durable metadata-only storage for provider profiles and catalog snapshots.
//!
//! A row is an atomic profile/catalog pair scoped to provider, opaque
//! credential binding and region. Raw catalog responses, prompts and secret
//! material are rejected before SQLite writes.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_PROFILE_JSON_BYTES: usize = 16 * 1024;
pub const MAX_CATALOG_JSON_BYTES: usize = 512 * 1024;
pub const MAX_PROVIDER_PROFILE_ROWS: u32 = 256;
pub const MAX_CATALOG_ENTRIES: usize = 2_048;
pub const MAX_SCOPE_BYTES: usize = 256;
pub const MAX_REGION_BYTES: usize = 64;
pub const MAX_CATALOG_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProfileCatalogRecord {
    pub provider_id: String,
    pub credential_binding: String,
    pub region: String,
    pub revision: i64,
    pub profile_content_hash: String,
    pub profile_json: Vec<u8>,
    pub catalog_content_hash: String,
    pub catalog_json: Vec<u8>,
    pub updated_at_ms: i64,
    pub state: String,
    pub observed_at_ms: i64,
    pub expires_at_ms: i64,
    pub failure_code: Option<String>,
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS provider_profile_catalog_snapshots (
           provider_id TEXT NOT NULL,
           credential_binding TEXT NOT NULL,
           region TEXT NOT NULL,
           revision INTEGER NOT NULL,
           profile_content_hash TEXT NOT NULL,
           profile_json BLOB NOT NULL,
           catalog_content_hash TEXT NOT NULL,
           catalog_json BLOB NOT NULL,
           updated_at_ms INTEGER NOT NULL,
           state TEXT NOT NULL DEFAULT 'fresh',
           observed_at_ms INTEGER NOT NULL DEFAULT 1,
           expires_at_ms INTEGER NOT NULL DEFAULT 2,
           failure_code TEXT,
           PRIMARY KEY(provider_id, credential_binding, region)
         );
         CREATE INDEX IF NOT EXISTS idx_provider_profile_catalog_revision
           ON provider_profile_catalog_snapshots(revision);
         CREATE INDEX IF NOT EXISTS idx_provider_profile_catalog_updated
           ON provider_profile_catalog_snapshots(updated_at_ms);",
    )
}

/// Publishes one complete metadata snapshot. The revision check, row bound and
/// write share one transaction so a partial profile/catalog pair is impossible.
pub fn put(
    connection: &Connection,
    record: &ProviderProfileCatalogRecord,
) -> Result<bool, &'static str> {
    validate_record(record)?;
    let transaction = connection.unchecked_transaction().map_err(|_| "sqlite")?;
    let existing_revision = transaction
        .query_row(
            "SELECT revision FROM provider_profile_catalog_snapshots
             WHERE provider_id=?1 AND credential_binding=?2 AND region=?3",
            params![record.provider_id, record.credential_binding, record.region],
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
            .query_row(
                "SELECT COUNT(*) FROM provider_profile_catalog_snapshots",
                [],
                |row| row.get::<_, u32>(0),
            )
            .map_err(|_| "sqlite")?;
        if count >= MAX_PROVIDER_PROFILE_ROWS {
            return Err("provider profile catalog limit");
        }
    }

    let changed = transaction
        .execute(
            "INSERT INTO provider_profile_catalog_snapshots
             (provider_id,credential_binding,region,revision,profile_content_hash,
              profile_json,catalog_content_hash,catalog_json,updated_at_ms,state,
              observed_at_ms,expires_at_ms,failure_code)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT(provider_id,credential_binding,region)
             DO UPDATE SET revision=excluded.revision,
               profile_content_hash=excluded.profile_content_hash,
               profile_json=excluded.profile_json,
               catalog_content_hash=excluded.catalog_content_hash,
               catalog_json=excluded.catalog_json,
               updated_at_ms=excluded.updated_at_ms,
               state=excluded.state,
               observed_at_ms=excluded.observed_at_ms,
               expires_at_ms=excluded.expires_at_ms,
               failure_code=excluded.failure_code
             WHERE excluded.revision =
               provider_profile_catalog_snapshots.revision + 1",
            params![
                record.provider_id,
                record.credential_binding,
                record.region,
                record.revision,
                record.profile_content_hash,
                record.profile_json,
                record.catalog_content_hash,
                record.catalog_json,
                record.updated_at_ms,
                record.state,
                record.observed_at_ms,
                record.expires_at_ms,
                record.failure_code,
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
    credential_binding: &str,
    region: &str,
) -> rusqlite::Result<Option<ProviderProfileCatalogRecord>> {
    connection
        .query_row(
            "SELECT provider_id,credential_binding,region,revision,
                    profile_content_hash,profile_json,catalog_content_hash,
                    catalog_json,updated_at_ms,state,observed_at_ms,expires_at_ms,
                    failure_code
             FROM provider_profile_catalog_snapshots
             WHERE provider_id=?1 AND credential_binding=?2 AND region=?3",
            params![provider_id, credential_binding, region],
            |row| {
                Ok(ProviderProfileCatalogRecord {
                    provider_id: row.get(0)?,
                    credential_binding: row.get(1)?,
                    region: row.get(2)?,
                    revision: row.get(3)?,
                    profile_content_hash: row.get(4)?,
                    profile_json: row.get(5)?,
                    catalog_content_hash: row.get(6)?,
                    catalog_json: row.get(7)?,
                    updated_at_ms: row.get(8)?,
                    state: row.get(9)?,
                    observed_at_ms: row.get(10)?,
                    expires_at_ms: row.get(11)?,
                    failure_code: row.get(12)?,
                })
            },
        )
        .optional()
}

pub fn count(connection: &Connection) -> rusqlite::Result<u32> {
    connection.query_row(
        "SELECT COUNT(*) FROM provider_profile_catalog_snapshots",
        [],
        |row| row.get(0),
    )
}

fn validate_record(record: &ProviderProfileCatalogRecord) -> Result<(), &'static str> {
    if !valid_token(&record.provider_id, MAX_SCOPE_BYTES)
        || !valid_credential_binding(&record.credential_binding)
        || !valid_token(&record.region, MAX_REGION_BYTES)
        || record.revision <= 0
        || !valid_hash(&record.profile_content_hash)
        || !valid_hash(&record.catalog_content_hash)
        || record.profile_json.is_empty()
        || record.profile_json.len() > MAX_PROFILE_JSON_BYTES
        || record.catalog_json.is_empty()
        || record.catalog_json.len() > MAX_CATALOG_JSON_BYTES
        || record.updated_at_ms <= 0
        || !valid_state(&record.state)
        || record.observed_at_ms <= 0
        || record.expires_at_ms <= record.observed_at_ms
        || record.expires_at_ms.saturating_sub(record.observed_at_ms) > MAX_CATALOG_TTL_MS
        || record
            .failure_code
            .as_deref()
            .is_some_and(|value| !valid_failure_code(value))
    {
        return Err("invalid provider profile catalog");
    }

    let state_consistent = match record.state.as_str() {
        "fresh" => record.failure_code.is_none(),
        "stale" => true,
        "unavailable" => record
            .failure_code
            .as_deref()
            .is_some_and(|code| code != "credential_rejected" && code != "discovery_unsupported"),
        "credential_rejected" => record.failure_code.as_deref() == Some("credential_rejected"),
        "discovery_unsupported" => record.failure_code.as_deref() == Some("discovery_unsupported"),
        _ => false,
    };
    if !state_consistent {
        return Err("invalid provider profile catalog");
    }

    let profile: serde_json::Value = serde_json::from_slice(&record.profile_json)
        .map_err(|_| "invalid provider profile catalog")?;
    if !safe_profile_json(&profile, 0, false) {
        return Err("invalid provider profile catalog");
    }

    let catalog: serde_json::Value = serde_json::from_slice(&record.catalog_json)
        .map_err(|_| "invalid provider profile catalog")?;
    if !catalog
        .as_array()
        .is_some_and(|entries| entries.len() <= MAX_CATALOG_ENTRIES)
        || !safe_catalog_json(&catalog, 0)
    {
        return Err("invalid provider profile catalog");
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
    valid_token(value, MAX_SCOPE_BYTES) && !contains_secret_like_material(value)
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_state(value: &str) -> bool {
    matches!(
        value,
        "fresh" | "stale" | "unavailable" | "credential_rejected" | "discovery_unsupported"
    )
}

fn valid_failure_code(value: &str) -> bool {
    matches!(
        value,
        "network"
            | "timeout"
            | "credential_rejected"
            | "rate_limited"
            | "malformed_response"
            | "response_too_large"
            | "entry_limit_exceeded"
            | "protocol_mismatch"
            | "discovery_unsupported"
            | "unknown"
    )
}

fn contains_secret_like_material(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("secret")
        || lower.contains("bearer")
        || lower.contains("sk-")
        || lower.contains("gsk_")
        || lower.contains("aiza")
}

fn safe_profile_json(value: &serde_json::Value, depth: usize, endpoint: bool) -> bool {
    if depth > 8 {
        return false;
    }
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => true,
        serde_json::Value::String(text) => {
            text.len() <= 512
                && (!endpoint || valid_endpoint(text))
                && (endpoint || (!text.contains("http://") && !text.contains("https://")))
                && !contains_secret_like_material(text)
        }
        serde_json::Value::Array(items) => {
            items.len() <= 64
                && items
                    .iter()
                    .all(|item| safe_profile_json(item, depth + 1, false))
        }
        serde_json::Value::Object(fields) => {
            fields.len() <= 64
                && fields.iter().all(|(key, item)| {
                    let lower = key.to_ascii_lowercase();
                    !is_forbidden_key(&lower)
                        && safe_profile_json(item, depth + 1, lower == "endpoint")
                })
        }
    }
}

fn safe_catalog_json(value: &serde_json::Value, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => true,
        serde_json::Value::String(text) => {
            let lower = text.to_ascii_lowercase();
            text.len() <= 512
                && !lower.contains("http://")
                && !lower.contains("https://")
                && !contains_secret_like_material(text)
        }
        serde_json::Value::Array(items) => {
            items.len() <= MAX_CATALOG_ENTRIES
                && items.iter().all(|item| safe_catalog_json(item, depth + 1))
        }
        serde_json::Value::Object(fields) => {
            fields.len() <= 64
                && fields.iter().all(|(key, item)| {
                    !is_forbidden_key(&key.to_ascii_lowercase())
                        && safe_catalog_json(item, depth + 1)
                })
        }
    }
}

fn is_forbidden_key(key: &str) -> bool {
    key.contains("prompt")
        || key.contains("secret")
        || key.contains("password")
        || (key.contains("token") && !matches!(key, "context_tokens" | "max_output_tokens"))
        || key.contains("header")
        || key.contains("response")
        || key.contains("body")
}

fn valid_endpoint(value: &str) -> bool {
    value.len() <= 512
        && value == value.trim()
        && (value.starts_with("https://") || value.starts_with("http://"))
        && !value.contains(['?', '#', '@'])
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        && value
            .split_once("://")
            .is_some_and(|(_, authority)| !authority.is_empty() && !authority.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(scope: &str, revision: i64) -> ProviderProfileCatalogRecord {
        ProviderProfileCatalogRecord {
            provider_id: "openrouter".into(),
            credential_binding: scope.into(),
            region: "global".into(),
            revision,
            profile_content_hash: "a".repeat(64),
            profile_json: br#"{
              "schema_version":1,
              "provider_id":"openrouter",
              "transport":"openai_compatible",
              "endpoint":"https://openrouter.ai/api/v1",
              "region":"global",
              "credential_binding":"credential:openrouter",
              "revision":1
            }"#
            .to_vec(),
            catalog_content_hash: "b".repeat(64),
            catalog_json: br#"[{"model_id":"provider/model","limits":{"context_tokens":4096}}]"#
                .to_vec(),
            updated_at_ms: 1_000,
            state: "fresh".into(),
            observed_at_ms: 1_000,
            expires_at_ms: 2_000,
            failure_code: None,
        }
    }

    #[test]
    fn scoped_revision_write_is_atomic_and_idempotent() {
        let connection = Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema");
        assert!(put(&connection, &record("cred:a", 1)).expect("first write"));
        assert!(!put(&connection, &record("cred:a", 1)).expect("duplicate write"));
        assert!(!put(&connection, &record("cred:a", 3)).expect("revision gap"));
        assert!(put(&connection, &record("cred:a", 2)).expect("next revision"));
        assert!(put(&connection, &record("cred:b", 1)).expect("other scope"));
        assert_eq!(count(&connection).expect("count"), 2);
        assert_eq!(
            get(&connection, "openrouter", "cred:a", "global")
                .expect("read")
                .expect("snapshot")
                .revision,
            2
        );
    }

    #[test]
    fn rejects_raw_catalog_material_and_invalid_profile_endpoint() {
        let connection = Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema");
        let mut unsafe_record = record("credential:a", 1);
        unsafe_record.catalog_json =
            br#"[{"provider_response":"https://provider.test","prompt":"private"}]"#.to_vec();
        assert_eq!(
            put(&connection, &unsafe_record),
            Err("invalid provider profile catalog")
        );

        let mut bad_endpoint = record("credential:a", 1);
        bad_endpoint.profile_json = br#"{
          "provider_id":"openrouter",
          "endpoint":"https://provider.test/v1?api_key=secret"
        }"#
        .to_vec();
        assert_eq!(
            put(&connection, &bad_endpoint),
            Err("invalid provider profile catalog")
        );
        assert_eq!(count(&connection).expect("count"), 0);
    }

    #[test]
    fn rejects_secret_like_scope_and_oversized_model_catalog() {
        let connection = Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema");
        assert_eq!(
            put(&connection, &record("credential:sk-live", 1)),
            Err("invalid provider profile catalog")
        );

        let mut oversized = record("credential:a", 1);
        oversized.catalog_json = serde_json::to_vec(
            &(0..=MAX_CATALOG_ENTRIES)
                .map(|index| serde_json::json!({"model_id": format!("model-{index}")}))
                .collect::<Vec<_>>(),
        )
        .expect("catalog json");
        assert_eq!(
            put(&connection, &oversized),
            Err("invalid provider profile catalog")
        );
    }
}
