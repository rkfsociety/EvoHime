//! Durable JSON records and append-only transition metadata for Human Work Items.
use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> Result<(), rusqlite::Error> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS human_work_items (id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, state TEXT NOT NULL, item_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS human_work_item_events (item_id TEXT NOT NULL, revision INTEGER NOT NULL, event_type TEXT NOT NULL, metadata_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(item_id, revision));")
}
pub fn save(
    c: &Connection,
    id: &str,
    revision: u64,
    state: &str,
    json: &[u8],
    event: &str,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    let tx = c.unchecked_transaction()?;
    let current: Option<u64> = tx
        .query_row(
            "SELECT revision FROM human_work_items WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    if current.is_some_and(|v| v >= revision) {
        return Ok(false);
    };
    tx.execute("INSERT INTO human_work_items VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,state=excluded.state,item_json=excluded.item_json,updated_at_ms=excluded.updated_at_ms",params![id,revision as i64,state,json,now])?;
    tx.execute(
        "INSERT INTO human_work_item_events VALUES(?1,?2,?3,?4,?5)",
        params![id, revision as i64, event, br#"{}"#, now],
    )?;
    tx.commit()?;
    Ok(true)
}
pub fn load_all_json(c: &Connection) -> Result<Vec<Vec<u8>>, rusqlite::Error> {
    let mut s = c.prepare("SELECT item_json FROM human_work_items ORDER BY id")?;
    let rows = s.query_map([], |r| r.get(0))?.collect();
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_is_durable_and_compare_and_set() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(save(&c, "a", 1, "waiting_for_human", br#"{}"#, "create", 1).unwrap());
        assert!(!save(&c, "a", 1, "waiting_for_human", br#"{}"#, "create", 2).unwrap());
        assert_eq!(load_all_json(&c).unwrap().len(), 1)
    }
}
