//! Durable metadata store for remote task bridge records.

use rusqlite::{params, Connection, OptionalExtension};

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS remote_task_records (remote_task_id TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL, status TEXT NOT NULL, content_hash TEXT NOT NULL, record_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}

pub fn put_record(
    c: &Connection,
    id: &str,
    version: u64,
    status: &str,
    hash: &str,
    json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT INTO remote_task_records(remote_task_id,version,status,content_hash,record_json,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(remote_task_id) DO UPDATE SET version=excluded.version,status=excluded.status,content_hash=excluded.content_hash,record_json=excluded.record_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.version > remote_task_records.version", params![id, version as i64, status, hash, json, now_ms])? > 0)
}

pub fn get_record(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT record_json FROM remote_task_records WHERE remote_task_id=?1",
        [id],
        |row| row.get(0),
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_record_does_not_replace_newer_record() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put_record(&c, "r", 2, "running", "h2", b"two", 2).unwrap());
        assert!(!put_record(&c, "r", 1, "pending", "h1", b"one", 3).unwrap());
        assert_eq!(get_record(&c, "r").unwrap(), Some(b"two".to_vec()));
    }
}
