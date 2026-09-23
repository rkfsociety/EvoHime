use rusqlite::{params, Connection, OptionalExtension, Transaction};
/// Maximum serialized record size accepted by [`put`].
pub const MAX_JSON_BYTES: usize = 256 * 1024;
/// Creates the revisioned language-intelligence record table.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS language_intelligence_records (id TEXT PRIMARY KEY NOT NULL, kind TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, record_json BLOB NOT NULL, status TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_language_intelligence_kind ON language_intelligence_records(kind, updated_at_ms);")
}
/// Inserts or advances a bounded record when its revision is newer or an identical replay.
#[allow(clippy::too_many_arguments)]
pub fn put(
    connection: &Connection,
    id: &str,
    kind: &str,
    revision: u64,
    hash: &str,
    json: &[u8],
    status: &str,
    now_ms: i64,
) -> Result<bool, &'static str> {
    if id.trim().is_empty()
        || kind.trim().is_empty()
        || revision == 0
        || hash.trim().is_empty()
        || json.is_empty()
        || json.len() > MAX_JSON_BYTES
        || status.trim().is_empty()
        || now_ms <= 0
    {
        return Err("invalid language intelligence record");
    }
    connection.execute("INSERT INTO language_intelligence_records(id,kind,revision,content_hash,record_json,status,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,content_hash=excluded.content_hash,record_json=excluded.record_json,status=excluded.status,updated_at_ms=excluded.updated_at_ms WHERE excluded.revision > language_intelligence_records.revision OR (excluded.revision = language_intelligence_records.revision AND excluded.content_hash = language_intelligence_records.content_hash)", params![id, kind, revision as i64, hash, json, status, now_ms]).map(|n| n == 1).map_err(|_| "sqlite")
}
/// Record kind, serialized payload, lifecycle status, and revision.
pub type StoredRecord = (String, Vec<u8>, String, u64);
/// Loads a record's kind, serialized payload, status, and revision by ID.
#[allow(clippy::type_complexity)]
pub fn get(
    connection: &Connection,
    id: &str,
) -> rusqlite::Result<Option<(String, Vec<u8>, String, u64)>> {
    connection.query_row("SELECT kind,record_json,status,revision FROM language_intelligence_records WHERE id=?1", params![id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get::<_,i64>(3)? as u64))).optional()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_revision_cannot_replace_record() {
        let c = Connection::open_in_memory().unwrap();
        let t = c.unchecked_transaction().unwrap();
        install_schema(&t).unwrap();
        t.commit().unwrap();
        assert!(put(&c, "x", "descriptor", 2, "h", b"{}", "active", 1).unwrap());
        assert!(!put(&c, "x", "descriptor", 1, "old", b"{}", "draft", 2).unwrap());
        assert_eq!(get(&c, "x").unwrap().unwrap().3, 2);
    }
}
