use rusqlite::{params, Connection, OptionalExtension};

/// Maximum serialized payload size accepted by this store (one mebibyte).
pub const MAX_JSON_BYTES: usize = 1024 * 1024;

/// Result of attempting to reserve an idempotency key.
#[derive(Debug, PartialEq, Eq)]
pub enum IdempotencyClaim {
    /// The caller inserted the key and owns the operation.
    Claimed,
    /// Another caller owns the key and has not stored a response yet.
    Pending,
    /// The operation already completed with the stored response bytes.
    Completed(Vec<u8>),
}

/// Creates the change-set, candidate, and idempotency tables and indexes.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_git_change_sets (id TEXT PRIMARY KEY, version INTEGER NOT NULL, revision INTEGER NOT NULL DEFAULT 1, content_hash TEXT NOT NULL, state_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS agent_git_commit_candidates (id TEXT PRIMARY KEY, change_set_id TEXT NOT NULL, diff_hash TEXT NOT NULL, state_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, FOREIGN KEY(change_set_id) REFERENCES agent_git_change_sets(id)); CREATE INDEX IF NOT EXISTS idx_agent_git_candidates_change_set ON agent_git_commit_candidates(change_set_id, created_at_ms DESC); CREATE TABLE IF NOT EXISTS agent_git_change_set_idempotency (idempotency_key TEXT PRIMARY KEY, response_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL);")
}

/// Creates only the idempotency table for callers that manage other tables separately.
pub fn install_idempotency_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_git_change_set_idempotency (idempotency_key TEXT PRIMARY KEY, response_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL);")
}

/// Inserts a change set at revision one unless its identifier already exists.
///
/// Returns `true` when inserted and `false` when the identifier was already present.
pub fn put_change_set(
    connection: &Connection,
    id: &str,
    version: u32,
    content_hash: &str,
    json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<bool> {
    if json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "change set too large"),
        )));
    }
    let changed = connection.execute("INSERT INTO agent_git_change_sets(id,version,revision,content_hash,state_json,created_at_ms) VALUES(?1,?2,1,?3,?4,?5) ON CONFLICT(id) DO NOTHING", params![id, version, content_hash, json, created_at_ms])?;
    Ok(changed == 1)
}
/// Loads the serialized change set for `id`, if present.
pub fn get_change_set(connection: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT state_json FROM agent_git_change_sets WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
}

/// Replaces a change set only if its current revision equals `expected_revision`.
///
/// A successful update increments the revision and returns `true`; a missing row or
/// revision conflict returns `false`.
pub fn update_change_set(
    connection: &Connection,
    id: &str,
    expected_revision: u64,
    version: u32,
    content_hash: &str,
    json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<bool> {
    if json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "change set too large"),
        )));
    }
    let changed = connection.execute(
        "UPDATE agent_git_change_sets SET version=?2, revision=revision+1, content_hash=?3, state_json=?4, created_at_ms=?5 WHERE id=?1 AND revision=?6",
        params![id, version, content_hash, json, created_at_ms, expected_revision],
    )?;
    Ok(changed == 1)
}
/// Inserts or replaces a serialized commit candidate.
pub fn put_candidate(
    connection: &Connection,
    id: &str,
    change_set_id: &str,
    diff_hash: &str,
    json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<()> {
    if json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "candidate too large"),
        )));
    }
    connection.execute("INSERT INTO agent_git_commit_candidates(id,change_set_id,diff_hash,state_json,created_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET change_set_id=excluded.change_set_id,diff_hash=excluded.diff_hash,state_json=excluded.state_json,created_at_ms=excluded.created_at_ms", params![id, change_set_id, diff_hash, json, created_at_ms])?;
    Ok(())
}

// These paired writes are the public transactional boundary for two records;
// keeping the column-wise arguments explicit makes the CAS and candidate
// payloads auditable at the call site.
/// Atomically updates a change set and inserts its commit candidate.
///
/// Returns `false` without writing either record when the revision compare-and-swap
/// fails. Database errors are returned and the transaction is rolled back.
#[allow(clippy::too_many_arguments)]
pub fn update_change_set_and_put_candidate(
    connection: &Connection,
    change_set_id: &str,
    expected_revision: u64,
    version: u32,
    content_hash: &str,
    change_set_json: &[u8],
    candidate_id: &str,
    candidate_diff_hash: &str,
    candidate_json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<bool> {
    let transaction = connection.unchecked_transaction()?;
    if !update_change_set(
        &transaction,
        change_set_id,
        expected_revision,
        version,
        content_hash,
        change_set_json,
        created_at_ms,
    )? {
        transaction.rollback()?;
        return Ok(false);
    }
    put_candidate(
        &transaction,
        candidate_id,
        change_set_id,
        candidate_diff_hash,
        candidate_json,
        created_at_ms,
    )?;
    transaction.commit()?;
    Ok(true)
}

// See the paired insert above: the update variant intentionally preserves the
// same explicit transaction contract.
/// Atomically updates a change set and an existing commit candidate.
///
/// Returns `false` without writing either record when the change-set revision does
/// not match or the candidate does not exist.
#[allow(clippy::too_many_arguments)]
pub fn update_change_set_and_update_candidate(
    connection: &Connection,
    change_set_id: &str,
    expected_revision: u64,
    version: u32,
    content_hash: &str,
    change_set_json: &[u8],
    candidate_id: &str,
    candidate_diff_hash: &str,
    candidate_json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<bool> {
    let transaction = connection.unchecked_transaction()?;
    if !update_change_set(
        &transaction,
        change_set_id,
        expected_revision,
        version,
        content_hash,
        change_set_json,
        created_at_ms,
    )? {
        transaction.rollback()?;
        return Ok(false);
    }
    if !update_candidate(
        &transaction,
        candidate_id,
        candidate_diff_hash,
        candidate_json,
        created_at_ms,
    )? {
        transaction.rollback()?;
        return Ok(false);
    }
    transaction.commit()?;
    Ok(true)
}
/// Loads a serialized candidate by identifier, if present.
pub fn get_candidate(connection: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT state_json FROM agent_git_commit_candidates WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
}

/// Loads the most recently created candidate for a change set, if any.
pub fn get_latest_candidate(
    connection: &Connection,
    change_set_id: &str,
) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT state_json FROM agent_git_commit_candidates WHERE change_set_id=?1 ORDER BY created_at_ms DESC, id DESC LIMIT 1",
            [change_set_id],
            |r| r.get(0),
        )
        .optional()
}

/// Replaces a candidate's diff hash and serialized state.
///
/// Returns `true` when a row was updated and `false` when no candidate has `id`.
pub fn update_candidate(
    connection: &Connection,
    id: &str,
    diff_hash: &str,
    json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<bool> {
    if json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "candidate too large"),
        )));
    }
    let changed = connection.execute(
        "UPDATE agent_git_commit_candidates SET diff_hash=?2,state_json=?3,created_at_ms=?4 WHERE id=?1",
        params![id, diff_hash, json, created_at_ms],
    )?;
    Ok(changed == 1)
}

/// Stores a completed idempotency response only when the key is unused.
///
/// Returns `false` when a response or pending claim already occupies the key.
pub fn put_idempotent(
    connection: &Connection,
    key: &str,
    response_json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<bool> {
    if response_json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "idempotency response too large",
            ),
        )));
    }
    let changed = connection.execute(
        "INSERT INTO agent_git_change_set_idempotency(idempotency_key,response_json,created_at_ms) VALUES(?1,?2,?3) ON CONFLICT(idempotency_key) DO NOTHING",
        params![key, response_json, created_at_ms],
    )?;
    Ok(changed == 1)
}

/// Reserves an idempotency key, distinguishing a pending claim from a saved result.
pub fn claim_idempotent(
    connection: &Connection,
    key: &str,
    created_at_ms: i64,
) -> rusqlite::Result<IdempotencyClaim> {
    let changed = connection.execute(
        "INSERT INTO agent_git_change_set_idempotency(idempotency_key,response_json,created_at_ms) VALUES(?1, X'', ?2) ON CONFLICT(idempotency_key) DO NOTHING",
        params![key, created_at_ms],
    )?;
    if changed == 1 {
        return Ok(IdempotencyClaim::Claimed);
    }
    let existing = get_idempotent(connection, key)?.unwrap_or_default();
    if existing.is_empty() {
        Ok(IdempotencyClaim::Pending)
    } else {
        Ok(IdempotencyClaim::Completed(existing))
    }
}

/// Fills a pending claim with a non-empty response, if it is still pending.
pub fn complete_idempotent(
    connection: &Connection,
    key: &str,
    response_json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<bool> {
    if response_json.is_empty() || response_json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "idempotency response too large or empty",
            ),
        )));
    }
    let changed = connection.execute(
        "UPDATE agent_git_change_set_idempotency SET response_json=?2,created_at_ms=?3 WHERE idempotency_key=?1 AND length(response_json)=0",
        params![key, response_json, created_at_ms],
    )?;
    Ok(changed == 1)
}

/// Removes a pending claim; completed responses are left intact.
pub fn release_idempotent(connection: &Connection, key: &str) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "DELETE FROM agent_git_change_set_idempotency WHERE idempotency_key=?1 AND length(response_json)=0",
        [key],
    )?;
    Ok(changed == 1)
}

/// Loads the stored idempotency response, including an empty pending marker.
pub fn get_idempotent(connection: &Connection, key: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT response_json FROM agent_git_change_set_idempotency WHERE idempotency_key=?1",
            [key],
            |r| r.get(0),
        )
        .optional()
}

#[cfg(test)]
#[path = "agent_git_change_sets_store_tests.rs"]
mod tests;
