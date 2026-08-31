//! Durable catalog for validated execution policy profiles.
//! Runtime handles, output and leases are intentionally not represented here.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_JSON_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPolicyProfileRecord {
    pub profile_id: String,
    pub version: i64,
    pub profile_hash: String,
    pub profile_json: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionPolicyProfileStoreError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("profile record is invalid")]
    Invalid,
}

pub fn install_schema(connection: &rusqlite::Transaction<'_>) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS execution_policy_profiles (
            profile_id TEXT PRIMARY KEY,
            version INTEGER NOT NULL,
            profile_hash TEXT NOT NULL,
            profile_json BLOB NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )
}

pub fn save(
    connection: &Connection,
    record: &ExecutionPolicyProfileRecord,
) -> Result<(), ExecutionPolicyProfileStoreError> {
    if record.profile_id.is_empty()
        || record.version <= 0
        || record.profile_hash.len() != 64
        || record.profile_json.is_empty()
        || record.profile_json.len() > MAX_JSON_BYTES
    {
        return Err(ExecutionPolicyProfileStoreError::Invalid);
    }
    connection.execute(
        "INSERT INTO execution_policy_profiles(profile_id, version, profile_hash, profile_json)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(profile_id) DO UPDATE SET version=excluded.version,
         profile_hash=excluded.profile_hash, profile_json=excluded.profile_json,
         updated_at=strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![
            record.profile_id,
            record.version,
            record.profile_hash,
            record.profile_json
        ],
    )?;
    Ok(())
}

pub fn get(
    connection: &Connection,
    profile_id: &str,
) -> Result<Option<ExecutionPolicyProfileRecord>, ExecutionPolicyProfileStoreError> {
    connection
        .query_row(
            "SELECT profile_id, version, profile_hash, profile_json
             FROM execution_policy_profiles WHERE profile_id = ?1",
            [profile_id],
            |row| {
                Ok(ExecutionPolicyProfileRecord {
                    profile_id: row.get(0)?,
                    version: row.get(1)?,
                    profile_hash: row.get(2)?,
                    profile_json: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_round_trips_without_runtime_state() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE execution_policy_profiles(
                 profile_id TEXT PRIMARY KEY, version INTEGER NOT NULL,
                 profile_hash TEXT NOT NULL, profile_json BLOB NOT NULL,
                 updated_at TEXT NOT NULL DEFAULT '' )",
            )
            .unwrap();
        let record = ExecutionPolicyProfileRecord {
            profile_id: "restricted-process-v1".into(),
            version: 1,
            profile_hash: "a".repeat(64),
            profile_json: br#"{}"#.to_vec(),
        };
        save(&connection, &record).unwrap();
        assert_eq!(get(&connection, &record.profile_id).unwrap(), Some(record));
        assert!(get(&connection, "runtime-handle").unwrap().is_none());
    }
}
