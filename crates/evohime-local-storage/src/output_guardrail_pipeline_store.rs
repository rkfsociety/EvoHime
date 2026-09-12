use rusqlite::{params, Connection};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS output_guardrail_pipelines (id TEXT PRIMARY KEY, version INTEGER NOT NULL, content_hash TEXT NOT NULL, pipeline_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    version: u32,
    hash: &str,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<()> {
    if json.len() > 512 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "pipeline too large"),
        )));
    }
    c.execute(
        "INSERT INTO output_guardrail_pipelines(id,version,content_hash,pipeline_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET version=excluded.version,content_hash=excluded.content_hash,pipeline_json=excluded.pipeline_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.version >= output_guardrail_pipelines.version",
        params![id, version, hash, json, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_version_cannot_replace_pipeline() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        put(&c, "pipeline", 2, "new", br#"{"version":2}"#, 2).unwrap();
        put(&c, "pipeline", 1, "old", br#"{"version":1}"#, 3).unwrap();
        let stored: (i64, Vec<u8>) = c
            .query_row(
                "SELECT version,pipeline_json FROM output_guardrail_pipelines WHERE id='pipeline'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (2, br#"{"version":2}"#.to_vec()));
    }
}
