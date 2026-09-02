use rusqlite::{params, Connection, OptionalExtension};

const MAX_QUEUE: i64 = 512;
const MAX_BYTES: i64 = 64 * 1024;
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS telemetry_consent (id INTEGER PRIMARY KEY CHECK(id=1), consent_json BLOB NOT NULL, revision INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS telemetry_queue (event_id TEXT PRIMARY KEY, category TEXT NOT NULL, event_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS telemetry_idempotency (idempotency_key TEXT PRIMARY KEY, operation TEXT NOT NULL);")
}

pub fn consent_revision(c: &Connection) -> rusqlite::Result<Option<u64>> {
    c.query_row(
        "SELECT revision FROM telemetry_consent WHERE id=1",
        [],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map(|value| value.map(|revision| revision as u64))
}

pub fn claim_idempotency(c: &Connection, key: &str, operation: &str) -> rusqlite::Result<bool> {
    Ok(c.execute(
        "INSERT OR IGNORE INTO telemetry_idempotency VALUES(?1,?2)",
        params![key, operation],
    )? == 1)
}
pub fn put_consent(c: &Connection, j: &[u8], revision: u64) -> rusqlite::Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO telemetry_consent VALUES(1,?1,?2)",
        params![j, revision as i64],
    )?;
    Ok(())
}
pub fn put_event(
    c: &Connection,
    id: &str,
    cat: &str,
    j: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    let count: i64 = c.query_row("SELECT COUNT(*) FROM telemetry_queue", [], |r| r.get(0))?;
    let bytes: i64 = c.query_row(
        "SELECT COALESCE(SUM(length(event_json)), 0) FROM telemetry_queue",
        [],
        |r| r.get(0),
    )?;
    if count >= MAX_QUEUE || bytes.saturating_add(j.len() as i64) > MAX_BYTES {
        return Ok(false);
    }
    let n = c.execute(
        "INSERT OR IGNORE INTO telemetry_queue VALUES(?1,?2,?3,?4)",
        params![id, cat, j, now],
    )?;
    Ok(n == 1)
}
pub fn list(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s = c.prepare(
        "SELECT event_json FROM telemetry_queue ORDER BY created_at_ms,event_id LIMIT 512",
    )?;
    let rows = s.query_map([], |r| r.get(0))?.collect();
    rows
}
pub fn clear(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch(
        "DELETE FROM telemetry_queue; DELETE FROM telemetry_consent; DELETE FROM telemetry_idempotency;",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduplicates_and_enforces_bounds() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put_event(&c, "one", "product", b"{}", 1).unwrap());
        assert!(!put_event(&c, "one", "product", b"{}", 1).unwrap());
        for index in 1..MAX_QUEUE {
            assert!(put_event(&c, &index.to_string(), "product", b"{}", index).unwrap());
        }
        assert!(!put_event(&c, "overflow", "product", b"{}", MAX_QUEUE).unwrap());
    }

    #[test]
    fn clear_removes_consent_and_events() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        put_consent(&c, b"{}", 1).unwrap();
        put_event(&c, "one", "product", b"{}", 1).unwrap();
        clear(&c).unwrap();
        assert!(list(&c).unwrap().is_empty());
        let count: i64 = c
            .query_row("SELECT COUNT(*) FROM telemetry_consent", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn idempotency_claim_is_single_use() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(claim_idempotency(&c, "k", "clear").unwrap());
        assert!(!claim_idempotency(&c, "k", "clear").unwrap());
    }
}
