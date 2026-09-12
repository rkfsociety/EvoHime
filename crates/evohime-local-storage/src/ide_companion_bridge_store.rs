use rusqlite::{params, Connection, OptionalExtension, Transaction};
const MAX_BRIDGE_JSON_BYTES: usize = 64 * 1024;
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS ide_companion_bridge (id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(id,revision), UNIQUE(id,idempotency_key));")
}
pub fn save(
    c: &Connection,
    id: &str,
    r: u64,
    h: &str,
    j: &[u8],
    k: &str,
    n: i64,
) -> rusqlite::Result<()> {
    if j.len() > MAX_BRIDGE_JSON_BYTES {
        return Err(rusqlite::Error::InvalidParameterName(
            "IDE bridge JSON too large".into(),
        ));
    }
    let old:Option<(u64,String)>=c.query_row("SELECT revision,content_hash FROM ide_companion_bridge WHERE id=?1 AND idempotency_key=?2",params![id,k],|x|Ok((x.get(0)?,x.get(1)?))).optional()?;
    if let Some((or, oh)) = old {
        if or == r && oh == h {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "idempotency_conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO ide_companion_bridge VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, r, h, j, k, n],
    )?;
    Ok(())
}
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT json FROM ide_companion_bridge WHERE id=?1 ORDER BY revision DESC LIMIT 1",
        params![id],
        |x| x.get(0),
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_bridge_json_is_rejected_before_storage() {
        let mut connection = Connection::open_in_memory().unwrap();
        let transaction = connection.transaction().unwrap();
        install_schema(&transaction).unwrap();
        transaction.commit().unwrap();
        assert!(save(
            &connection,
            "bridge",
            1,
            "hash",
            &vec![b'x'; MAX_BRIDGE_JSON_BYTES + 1],
            "key",
            1,
        )
        .is_err());
        assert!(current(&connection, "bridge").unwrap().is_none());
    }
}
