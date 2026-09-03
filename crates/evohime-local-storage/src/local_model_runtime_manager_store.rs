use rusqlite::{params, Connection, OptionalExtension, Result};

type StoredRecord = (String, u64, String, Vec<u8>);

pub fn install_schema(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS local_model_manager_state (state_id TEXT PRIMARY KEY, version INTEGER NOT NULL, content_hash TEXT NOT NULL, state_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS local_model_manager_records (record_id TEXT PRIMARY KEY, record_kind TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, record_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    version: u64,
    hash: &str,
    json: &[u8],
    now_ms: i64,
) -> Result<bool> {
    if version == 1 {
        return Ok(c.execute("INSERT OR IGNORE INTO local_model_manager_state(state_id,version,content_hash,state_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5)", params![id,version,hash,json,now_ms])? == 1);
    }
    Ok(c.execute("UPDATE local_model_manager_state SET version=?2,content_hash=?3,state_json=?4,updated_at_ms=?5 WHERE state_id=?1 AND version=?2-1", params![id,version,hash,json,now_ms])? == 1)
}
pub fn get(c: &Connection, id: &str) -> Result<Option<(u64, String, Vec<u8>)>> {
    c.query_row(
        "SELECT version,content_hash,state_json FROM local_model_manager_state WHERE state_id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
}

pub fn put_record(
    c: &Connection,
    record_id: &str,
    record_kind: &str,
    revision: u64,
    hash: &str,
    json: &[u8],
    now_ms: i64,
) -> Result<bool> {
    if revision == 1 {
        return Ok(c.execute("INSERT OR IGNORE INTO local_model_manager_records(record_id,record_kind,revision,content_hash,record_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6)", params![record_id, record_kind, revision, hash, json, now_ms])? == 1);
    }
    Ok(c.execute("UPDATE local_model_manager_records SET record_kind=?2,revision=?3,content_hash=?4,record_json=?5,updated_at_ms=?6 WHERE record_id=?1 AND revision=?3-1", params![record_id, record_kind, revision, hash, json, now_ms])? == 1)
}

pub fn get_record(c: &Connection, record_id: &str) -> Result<Option<StoredRecord>> {
    c.query_row(
        "SELECT record_kind,revision,content_hash,record_json FROM local_model_manager_records WHERE record_id=?1",
        [record_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).optional()
}

pub fn list_records(c: &Connection, record_kind: &str, limit: u32) -> Result<Vec<StoredRecord>> {
    let mut statement = c.prepare("SELECT record_id,revision,content_hash,record_json FROM local_model_manager_records WHERE record_kind=?1 ORDER BY record_id LIMIT ?2")?;
    let rows = statement.query_map(params![record_kind, i64::from(limit.min(256))], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })?;
    rows.collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_fence() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put(&c, "policy", 1, "h", b"{}", 1).unwrap());
        assert!(!put(&c, "policy", 3, "h", b"{}", 2).unwrap());
        assert!(put_record(&c, "model:m:1", "model", 1, "h", b"{}", 1).unwrap());
        assert!(!put_record(&c, "model:m:1", "model", 3, "h", b"{}", 2).unwrap());
        assert_eq!(get_record(&c, "model:m:1").unwrap().unwrap().0, "model");
    }
}
