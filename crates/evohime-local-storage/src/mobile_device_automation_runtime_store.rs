use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Maximum accepted serialized mobile automation runtime state size.
const MAX_RUNTIME_JSON_BYTES: usize = 64 * 1024;

/// Creates the revisioned mobile automation runtime table transactionally.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS mobile_device_automation_runtime (id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(id,revision), UNIQUE(id,idempotency_key));")
}
/// Persists a mobile automation runtime revision with replay protection.
///
/// JSON larger than 64 KiB is rejected. A repeated idempotency key is accepted
/// only when its revision and content hash match the original write.
///
/// # Errors
///
/// Returns a SQLite error for oversized data, conflicting key reuse, or a
/// database failure.
pub fn save(
    c: &Connection,
    id: &str,
    r: u64,
    h: &str,
    j: &[u8],
    k: &str,
    n: i64,
) -> rusqlite::Result<()> {
    if j.len() > MAX_RUNTIME_JSON_BYTES {
        return Err(rusqlite::Error::InvalidParameterName(
            "mobile automation runtime JSON too large".into(),
        ));
    }
    let old:Option<(u64,String)>=c.query_row("SELECT revision,content_hash FROM mobile_device_automation_runtime WHERE id=?1 AND idempotency_key=?2",params![id,k],|x|Ok((x.get(0)?,x.get(1)?))).optional()?;
    if let Some((or, oh)) = old {
        if or == r && oh == h {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "idempotency_conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO mobile_device_automation_runtime VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, r, h, j, k, n],
    )?;
    Ok(())
}
/// Loads the JSON runtime snapshot at the highest revision for `id`.
///
/// # Errors
///
/// Returns a SQLite error if the query fails.
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT json FROM mobile_device_automation_runtime WHERE id=?1 ORDER BY revision DESC LIMIT 1",params![id],|x|x.get(0)).optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_runtime_json_is_rejected_before_storage() {
        let mut connection = Connection::open_in_memory().unwrap();
        let transaction = connection.transaction().unwrap();
        install_schema(&transaction).unwrap();
        transaction.commit().unwrap();
        assert!(save(
            &connection,
            "runtime",
            1,
            "hash",
            &vec![b'x'; MAX_RUNTIME_JSON_BYTES + 1],
            "key",
            1,
        )
        .is_err());
        assert!(current(&connection, "runtime").unwrap().is_none());
    }
}
