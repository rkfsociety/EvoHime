use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS core_bus_events (event_id TEXT PRIMARY KEY, event_json BLOB NOT NULL, content_hash TEXT NOT NULL, state TEXT NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS core_bus_deliveries (subscription_id TEXT NOT NULL, event_id TEXT NOT NULL, state TEXT NOT NULL, attempt INTEGER NOT NULL DEFAULT 0, last_error TEXT, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(subscription_id,event_id)); CREATE TABLE IF NOT EXISTS core_bus_dead_letters (subscription_id TEXT NOT NULL, event_id TEXT NOT NULL, attempt INTEGER NOT NULL, error_class TEXT NOT NULL, payload_summary_hash TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(subscription_id,event_id));")
}
pub fn put_event(
    c: &Connection,
    id: &str,
    json: &[u8],
    hash: &str,
    state: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO core_bus_events(event_id,event_json,content_hash,state,created_at_ms) VALUES(?1,?2,?3,?4,?5)",params![id,json,hash,state,now])?==1)
}
pub fn get_event(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT event_json FROM core_bus_events WHERE event_id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()
}
pub fn put_delivery(
    c: &Connection,
    subscription: &str,
    event: &str,
    state: &str,
    attempt: u32,
    error: Option<&str>,
    now: i64,
) -> rusqlite::Result<()> {
    c.execute("INSERT INTO core_bus_deliveries(subscription_id,event_id,state,attempt,last_error,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(subscription_id,event_id) DO UPDATE SET state=excluded.state,attempt=excluded.attempt,last_error=excluded.last_error,updated_at_ms=excluded.updated_at_ms",params![subscription,event,state,attempt,error,now])?;
    Ok(())
}
pub fn put_dead_letter(
    c: &Connection,
    subscription: &str,
    event: &str,
    attempt: u32,
    error: &str,
    hash: &str,
    now: i64,
) -> rusqlite::Result<()> {
    c.execute("INSERT OR IGNORE INTO core_bus_dead_letters(subscription_id,event_id,attempt,error_class,payload_summary_hash,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6)",params![subscription,event,attempt,error,hash,now])?;
    Ok(())
}
