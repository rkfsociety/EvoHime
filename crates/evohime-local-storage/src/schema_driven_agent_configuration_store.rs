//! Durable metadata-only configuration snapshots (schema v65).
use rusqlite::{params, Connection, OptionalExtension};

pub type ConfigurationStorageRow = (Vec<u8>, Vec<u8>, u64);

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS schema_agent_configurations (scope TEXT PRIMARY KEY NOT NULL, schema_json BLOB NOT NULL, snapshot_json BLOB NOT NULL, revision INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);")
}

pub fn save(
    c: &Connection,
    scope: &str,
    schema: &[u8],
    snapshot: &[u8],
    revision: u64,
    now: i64,
    expected: u64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT INTO schema_agent_configurations(scope,schema_json,snapshot_json,revision,updated_at_ms) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(scope) DO UPDATE SET schema_json=excluded.schema_json,snapshot_json=excluded.snapshot_json,revision=excluded.revision,updated_at_ms=excluded.updated_at_ms WHERE revision=?6", params![scope, schema, snapshot, revision as i64, now, expected as i64])? == 1)
}

pub fn load(c: &Connection, scope: &str) -> rusqlite::Result<Option<ConfigurationStorageRow>> {
    c.query_row(
        "SELECT schema_json,snapshot_json,revision FROM schema_agent_configurations WHERE scope=?1",
        [scope],
        |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64)),
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optimistic_revision_is_atomic() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(save(&c, "app", b"s", b"v", 1, 1, 0).unwrap());
        assert!(!save(&c, "app", b"s", b"v2", 2, 2, 0).unwrap());
        assert!(save(&c, "app", b"s", b"v2", 2, 2, 1).unwrap());
        assert_eq!(load(&c, "app").unwrap().unwrap().2, 2);
    }
}
