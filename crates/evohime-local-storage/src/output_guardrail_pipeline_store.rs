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
        "INSERT OR REPLACE INTO output_guardrail_pipelines VALUES(?1,?2,?3,?4,?5)",
        params![id, version, hash, json, now],
    )?;
    Ok(())
}
