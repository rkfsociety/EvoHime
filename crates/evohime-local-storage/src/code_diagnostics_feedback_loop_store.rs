use rusqlite::{params, Connection};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS code_diagnostics_providers (provider_id TEXT PRIMARY KEY, provider_json BLOB NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS code_diagnostics_snapshots (snapshot_id TEXT PRIMARY KEY, workspace_fingerprint TEXT NOT NULL, snapshot_json BLOB NOT NULL, content_hash TEXT NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS code_diagnostics_deltas (delta_id TEXT PRIMARY KEY, baseline_snapshot_id TEXT NOT NULL, current_snapshot_id TEXT NOT NULL, delta_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL);")
}
pub fn put_snapshot(
    c: &Connection,
    id: &str,
    workspace: &str,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO code_diagnostics_snapshots(snapshot_id,workspace_fingerprint,snapshot_json,content_hash,created_at_ms) VALUES(?1,?2,?3,?4,?5)",params![id,workspace,json,hash,now])?==1)
}
pub fn put_provider(
    c: &Connection,
    id: &str,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO code_diagnostics_providers(provider_id,provider_json,content_hash,updated_at_ms) VALUES(?1,?2,?3,?4)",params![id,json,hash,now])?==1)
}
pub fn get_snapshot(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT snapshot_json FROM code_diagnostics_snapshots WHERE snapshot_id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()
}
pub fn put_delta(
    c: &Connection,
    id: &str,
    baseline: &str,
    current: &str,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO code_diagnostics_deltas(delta_id,baseline_snapshot_id,current_snapshot_id,delta_json,created_at_ms) VALUES(?1,?2,?3,?4,?5)",params![id,baseline,current,json,now])?==1)
}
use rusqlite::OptionalExtension;
