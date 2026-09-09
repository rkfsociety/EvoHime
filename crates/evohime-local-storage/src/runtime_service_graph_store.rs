use rusqlite::{params, Connection, OptionalExtension, Transaction};

pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS runtime_service_graph (graph_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(graph_id, revision), UNIQUE(graph_id, idempotency_key)); CREATE TABLE IF NOT EXISTS runtime_service_graph_pin (run_id TEXT PRIMARY KEY, graph_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, pinned_at_ms INTEGER NOT NULL);")
}

pub fn save(
    c: &Connection,
    id: &str,
    revision: u64,
    hash: &str,
    json: &[u8],
    key: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let existing: Option<(u64, String)> = c.query_row("SELECT revision, content_hash FROM runtime_service_graph WHERE graph_id=?1 AND idempotency_key=?2", params![id, key], |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
    if let Some((existing_revision, existing_hash)) = existing {
        if existing_revision == revision && existing_hash == hash {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "runtime graph idempotency conflict".into(),
        ));
    }
    let current: Option<u64> = c
        .query_row(
            "SELECT MAX(revision) FROM runtime_service_graph WHERE graph_id=?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if revision != current.unwrap_or(0) + 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "runtime graph revision conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO runtime_service_graph VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, revision, hash, json, key, now],
    )?;
    Ok(())
}

pub fn pin(
    c: &Connection,
    run_id: &str,
    id: &str,
    revision: u64,
    hash: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let existing: Option<(String, u64, String)> = c.query_row("SELECT graph_id, revision, content_hash FROM runtime_service_graph_pin WHERE run_id=?1", params![run_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()?;
    if let Some((existing_id, existing_revision, existing_hash)) = existing {
        if existing_id == id && existing_revision == revision && existing_hash == hash {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "runtime graph run already pinned".into(),
        ));
    }
    c.execute(
        "INSERT INTO runtime_service_graph_pin VALUES(?1,?2,?3,?4,?5)",
        params![run_id, id, revision, hash, now],
    )?;
    Ok(())
}

pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT json FROM runtime_service_graph WHERE graph_id=?1 ORDER BY revision DESC LIMIT 1",
        params![id],
        |row| row.get(0),
    )
    .optional()
}
