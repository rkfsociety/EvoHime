//! Durable metadata storage for hardware fit evidence.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_JSON_BYTES: usize = 128 * 1024;
pub const MAX_OBSERVATIONS: u32 = 2048;

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS hardware_fit_observations (
           observation_id TEXT PRIMARY KEY NOT NULL,
           revision INTEGER NOT NULL,
           content_hash TEXT NOT NULL,
           observation_json BLOB NOT NULL,
           source_class TEXT NOT NULL,
           updated_at_ms INTEGER NOT NULL,
           UNIQUE(observation_id, revision)
         );
         CREATE INDEX IF NOT EXISTS idx_hardware_fit_observations_hash
           ON hardware_fit_observations(content_hash);
         CREATE TABLE IF NOT EXISTS hardware_fit_catalog_snapshots (
           catalog_revision INTEGER PRIMARY KEY NOT NULL,
           snapshot_hash TEXT NOT NULL,
           snapshot_json BLOB NOT NULL,
           trusted INTEGER NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );",
    )
}

pub fn put(
    connection: &Connection,
    id: &str,
    revision: i64,
    content_hash: &str,
    json: &[u8],
    source_class: &str,
    updated_at_ms: i64,
) -> Result<bool, &'static str> {
    if id.trim().is_empty()
        || revision <= 0
        || content_hash.trim().is_empty()
        || json.is_empty()
        || json.len() > MAX_JSON_BYTES
        || source_class.trim().is_empty()
        || updated_at_ms <= 0
    {
        return Err("invalid hardware fit observation");
    }
    let existing = connection
        .query_row(
            "SELECT COUNT(*) FROM hardware_fit_observations WHERE observation_id != ?1",
            params![id],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|_| "sqlite")?;
    if existing >= MAX_OBSERVATIONS {
        return Err("hardware fit observation limit");
    }
    connection
        .execute(
            "INSERT INTO hardware_fit_observations
         (observation_id,revision,content_hash,observation_json,source_class,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(observation_id) DO UPDATE SET revision=excluded.revision,
           content_hash=excluded.content_hash, observation_json=excluded.observation_json,
           source_class=excluded.source_class, updated_at_ms=excluded.updated_at_ms
         WHERE excluded.revision = hardware_fit_observations.revision + 1",
            params![
                id,
                revision,
                content_hash,
                json,
                source_class,
                updated_at_ms
            ],
        )
        .map(|count| count == 1)
        .map_err(|_| "sqlite")
}

pub fn get(connection: &Connection, id: &str) -> rusqlite::Result<Option<(i64, Vec<u8>, String)>> {
    connection.query_row(
        "SELECT revision, observation_json, content_hash FROM hardware_fit_observations WHERE observation_id=?1",
        params![id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()
}

pub fn count(connection: &Connection) -> rusqlite::Result<u32> {
    connection.query_row(
        "SELECT COUNT(*) FROM hardware_fit_observations",
        [],
        |row| row.get(0),
    )
}

pub fn put_catalog_snapshot(
    connection: &Connection,
    revision: i64,
    hash: &str,
    json: &[u8],
    trusted: bool,
    updated_at_ms: i64,
) -> Result<bool, &'static str> {
    if revision <= 0
        || hash.trim().is_empty()
        || json.is_empty()
        || json.len() > MAX_JSON_BYTES
        || updated_at_ms <= 0
    {
        return Err("invalid hardware fit catalog");
    }
    let existing_trusted = connection
        .query_row(
            "SELECT trusted FROM hardware_fit_catalog_snapshots WHERE catalog_revision = ?1",
            params![revision],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map_err(|_| "sqlite")?;
    if existing_trusted == Some(true) && !trusted {
        return Ok(false);
    }
    connection
        .execute(
            "INSERT OR REPLACE INTO hardware_fit_catalog_snapshots
         (catalog_revision,snapshot_hash,snapshot_json,trusted,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5)",
            params![revision, hash, json, trusted, updated_at_ms],
        )
        .map(|count| count == 1)
        .map_err(|_| "sqlite")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_write_is_idempotent_and_fenced() {
        let connection = Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema");
        assert!(put(&connection, "obs", 1, "hash", b"{}", "import", 1).expect("put"));
        assert!(!put(&connection, "obs", 1, "hash", b"{}", "import", 2).expect("duplicate"));
        assert!(!put(&connection, "obs", 3, "hash", b"{}", "import", 3).expect("stale"));
        assert_eq!(count(&connection).expect("count"), 1);
    }

    #[test]
    fn untrusted_snapshot_cannot_replace_trusted_revision() {
        let connection = Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema");
        assert!(put_catalog_snapshot(&connection, 1, "trusted", b"{}", true, 1).expect("put"));
        assert!(
            !put_catalog_snapshot(&connection, 1, "untrusted", b"{}", false, 2).expect("reject")
        );
        let hash: String = connection
            .query_row(
                "SELECT snapshot_hash FROM hardware_fit_catalog_snapshots",
                [],
                |row| row.get(0),
            )
            .expect("hash");
        assert_eq!(hash, "trusted");
    }
}
