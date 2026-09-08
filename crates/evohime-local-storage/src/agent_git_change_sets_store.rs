use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_JSON_BYTES: usize = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum IdempotencyClaim {
    Claimed,
    Pending,
    Completed(Vec<u8>),
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_git_change_sets (id TEXT PRIMARY KEY, version INTEGER NOT NULL, revision INTEGER NOT NULL DEFAULT 1, content_hash TEXT NOT NULL, state_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS agent_git_commit_candidates (id TEXT PRIMARY KEY, change_set_id TEXT NOT NULL, diff_hash TEXT NOT NULL, state_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, FOREIGN KEY(change_set_id) REFERENCES agent_git_change_sets(id)); CREATE INDEX IF NOT EXISTS idx_agent_git_candidates_change_set ON agent_git_commit_candidates(change_set_id, created_at_ms DESC); CREATE TABLE IF NOT EXISTS agent_git_change_set_idempotency (idempotency_key TEXT PRIMARY KEY, response_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL);")
}

pub fn install_idempotency_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_git_change_set_idempotency (idempotency_key TEXT PRIMARY KEY, response_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL);")
}

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
pub fn get_change_set(connection: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT state_json FROM agent_git_change_sets WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
}

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
pub fn get_candidate(connection: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT state_json FROM agent_git_commit_candidates WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
}

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

pub fn release_idempotent(connection: &Connection, key: &str) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "DELETE FROM agent_git_change_set_idempotency WHERE idempotency_key=?1 AND length(response_json)=0",
        [key],
    )?;
    Ok(changed == 1)
}

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
mod tests {
    use super::*;

    #[test]
    fn stores_change_set_and_candidate() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        put_change_set(&c, "s", 1, "h", b"{}", 1).unwrap();
        put_candidate(&c, "c", "s", "d", b"{}", 2).unwrap();
        assert_eq!(get_change_set(&c, "s").unwrap(), Some(b"{}".to_vec()));
        assert_eq!(get_candidate(&c, "c").unwrap(), Some(b"{}".to_vec()));
        assert!(put_idempotent(&c, "request-1", b"{}", 3).unwrap());
        assert!(!put_idempotent(&c, "request-1", b"different", 4).unwrap());
        assert_eq!(
            get_idempotent(&c, "request-1").unwrap(),
            Some(b"{}".to_vec())
        );
    }

    #[test]
    fn idempotency_claim_is_single_owner_and_pending_is_not_replayed() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert_eq!(
            claim_idempotent(&c, "request-1", 1).unwrap(),
            IdempotencyClaim::Claimed
        );
        assert_eq!(
            claim_idempotent(&c, "request-1", 2).unwrap(),
            IdempotencyClaim::Pending
        );
        assert!(complete_idempotent(&c, "request-1", b"{}", 3).unwrap());
        assert_eq!(
            claim_idempotent(&c, "request-1", 4).unwrap(),
            IdempotencyClaim::Completed(b"{}".to_vec())
        );
        assert!(!release_idempotent(&c, "request-1").unwrap());
    }

    #[test]
    fn paired_transition_rolls_back_when_candidate_write_is_rejected() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        put_change_set(&c, "s", 1, &"a".repeat(64), b"{}", 1).unwrap();
        let result = update_change_set_and_put_candidate(
            &c,
            "s",
            1,
            1,
            &"b".repeat(64),
            b"updated",
            "candidate",
            &"d".repeat(64),
            &vec![b'x'; MAX_JSON_BYTES + 1],
            2,
        );
        assert!(result.is_err());
        assert_eq!(get_change_set(&c, "s").unwrap(), Some(b"{}".to_vec()));
    }

    #[test]
    fn schema_94_migrates_revision_and_idempotency_atomically() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE agent_git_change_sets (
                    id TEXT PRIMARY KEY,
                    version INTEGER NOT NULL,
                    content_hash TEXT NOT NULL,
                    state_json BLOB NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );
                CREATE TABLE agent_git_commit_candidates (
                    id TEXT PRIMARY KEY,
                    change_set_id TEXT NOT NULL,
                    diff_hash TEXT NOT NULL,
                    state_json BLOB NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );
                PRAGMA user_version = 93;",
            )
            .unwrap();
        let transaction = connection.transaction().unwrap();
        crate::migrations::v094::apply(&transaction, 93).unwrap();
        transaction.commit().unwrap();

        let revision: String = connection
            .query_row(
                "SELECT dflt_value FROM pragma_table_info('agent_git_change_sets') WHERE name='revision'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(revision, "1");
        let idempotency_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agent_git_change_set_idempotency'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(idempotency_table, 1);
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 94);
    }
}
