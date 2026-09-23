use rusqlite::{params, Connection, OptionalExtension, Transaction};

const MAX_JSON_BYTES: usize = 64 * 1024;

/// Creates revisioned optimizer state and run-pinning tables in the transaction.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS agent_program_optimizer (program_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(program_id, revision), UNIQUE(program_id, idempotency_key)); CREATE TABLE IF NOT EXISTS agent_program_optimizer_run (run_id TEXT PRIMARY KEY, program_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
/// Stores the next optimizer revision with idempotency and content-hash checks.
///
/// Repeating the same key, revision, and hash is treated as success. Conflicting
/// reuse of a key or a revision other than the next sequential revision is rejected.
pub fn save(
    c: &Connection,
    id: &str,
    revision: u64,
    hash: &str,
    json: &[u8],
    key: &str,
    now: i64,
) -> rusqlite::Result<()> {
    if json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::InvalidParameterName(
            "optimizer JSON too large".into(),
        ));
    }
    let existing: Option<(u64,String)> = c.query_row("SELECT revision,content_hash FROM agent_program_optimizer WHERE program_id=?1 AND idempotency_key=?2", params![id,key], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((r, h)) = existing {
        if r == revision && h == hash {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "optimizer idempotency conflict".into(),
        ));
    }
    let current: Option<u64> = c
        .query_row(
            "SELECT MAX(revision) FROM agent_program_optimizer WHERE program_id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if revision != current.unwrap_or(0) + 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "optimizer revision conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO agent_program_optimizer VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, revision, hash, json, key, now],
    )?;
    Ok(())
}
/// Returns the serialized state at the greatest stored revision for `id`.
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT json FROM agent_program_optimizer WHERE program_id=?1 ORDER BY revision DESC LIMIT 1",params![id],|r|r.get(0)).optional()
}
/// Pins a run to one optimizer revision and its content hash.
///
/// An identical repeat is idempotent; a run already pinned to different values
/// returns an error.
pub fn pin(
    c: &Connection,
    run_id: &str,
    id: &str,
    revision: u64,
    hash: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let existing: Option<(String,u64,String)>=c.query_row("SELECT program_id,revision,content_hash FROM agent_program_optimizer_run WHERE run_id=?1",params![run_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    if let Some((eid, er, eh)) = existing {
        if eid == id && er == revision && eh == hash {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "optimizer run already pinned".into(),
        ));
    }
    c.execute(
        "INSERT INTO agent_program_optimizer_run VALUES(?1,?2,?3,?4,?5)",
        params![run_id, id, revision, hash, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_optimizer_json_is_rejected_before_storage() {
        let mut connection = Connection::open_in_memory().unwrap();
        let transaction = connection.transaction().unwrap();
        install_schema(&transaction).unwrap();
        transaction.commit().unwrap();
        assert!(save(
            &connection,
            "program",
            1,
            "hash",
            &vec![b'x'; MAX_JSON_BYTES + 1],
            "key",
            1,
        )
        .is_err());
        assert!(current(&connection, "program").unwrap().is_none());
    }
}
