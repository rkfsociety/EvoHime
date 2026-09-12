use rusqlite::{params, Connection};
const MAX_LINEAGES: i64 = 256;
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS checkpoint_fork_lineages (fork_run_id TEXT PRIMARY KEY, source_checkpoint_id TEXT NOT NULL, parent_run_id TEXT NOT NULL, lineage_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    source: &str,
    parent: &str,
    j: &[u8],
    now: i64,
) -> rusqlite::Result<()> {
    if j.len() > 256 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "lineage too large"),
        )));
    }
    c.execute(
        "INSERT INTO checkpoint_fork_lineages(fork_run_id,source_checkpoint_id,parent_run_id,lineage_json,created_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(fork_run_id) DO NOTHING",
        params![id, source, parent, j, now],
    )?;
    Ok(())
}
pub fn list(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s = c.prepare(
        "SELECT lineage_json FROM checkpoint_fork_lineages ORDER BY fork_run_id LIMIT ?1",
    )?;
    let rows = s.query_map([MAX_LINEAGES], |r| r.get(0))?.collect();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_lineage_is_immutable_on_retry() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        put(
            &c,
            "fork",
            "checkpoint-1",
            "parent-1",
            br#"{"source":1}"#,
            1,
        )
        .unwrap();
        put(
            &c,
            "fork",
            "checkpoint-2",
            "parent-2",
            br#"{"source":2}"#,
            2,
        )
        .unwrap();
        assert_eq!(list(&c).unwrap(), vec![br#"{"source":1}"#.to_vec()]);
    }

    #[test]
    fn listing_is_bounded() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        for index in 0..300 {
            put(
                &c,
                &format!("fork-{index:03}"),
                "checkpoint",
                "parent",
                b"{}",
                index,
            )
            .unwrap();
        }
        assert_eq!(list(&c).unwrap().len(), MAX_LINEAGES as usize);
    }
}
