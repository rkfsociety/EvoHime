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

#[allow(clippy::too_many_arguments)]
pub fn replace(
    c: &Connection,
    id: &str,
    expected_revision: u64,
    revision: u64,
    state: &str,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute(
        "UPDATE safe_ui_extensions SET revision=?2,lifecycle=?3,extension_json=?4,manifest_hash=?5,updated_at_ms=?6 WHERE extension_id=?1 AND revision=?7",
        params![id, revision, state, json, hash, now, expected_revision],
    )? == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_requires_expected_revision() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put(&c, "ext", 1, "enabled", br#"{"revision":1}"#, "h1", 1).unwrap());
        assert!(!replace(&c, "ext", 0, 2, "disabled", br#"{"revision":2}"#, "h2", 2).unwrap());
        assert!(replace(&c, "ext", 1, 2, "disabled", br#"{"revision":2}"#, "h2", 3).unwrap());
        assert_eq!(get(&c, "ext").unwrap(), Some(br#"{"revision":2}"#.to_vec()));
    }
}
