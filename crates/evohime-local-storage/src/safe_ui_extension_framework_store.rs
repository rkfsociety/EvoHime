use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS safe_ui_extensions (extension_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, lifecycle TEXT NOT NULL, extension_json BLOB NOT NULL, manifest_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    revision: u64,
    state: &str,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO safe_ui_extensions(extension_id,revision,lifecycle,extension_json,manifest_hash,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6)",params![id,revision,state,json,hash,now])?==1)
}
pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT extension_json FROM safe_ui_extensions WHERE extension_id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()
}

pub fn replace(
    c: &Connection,
    id: &str,
    revision: u64,
    state: &str,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute(
        "UPDATE safe_ui_extensions SET revision=?2,lifecycle=?3,extension_json=?4,manifest_hash=?5,updated_at_ms=?6 WHERE extension_id=?1",
        params![id, revision, state, json, hash, now],
    )? == 1)
}
