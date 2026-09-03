use rusqlite::{params, Connection};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS prompt_cache_metrics(cache_key TEXT NOT NULL,hit INTEGER NOT NULL,input_tokens INTEGER NOT NULL,cached_tokens INTEGER NOT NULL,created_at_ms INTEGER NOT NULL,PRIMARY KEY(cache_key,created_at_ms));")
}
pub fn record(
    c: &Connection,
    key: &str,
    hit: bool,
    input: u32,
    cached: u32,
    now: i64,
) -> rusqlite::Result<()> {
    c.execute("INSERT OR REPLACE INTO prompt_cache_metrics(cache_key,hit,input_tokens,cached_tokens,created_at_ms) VALUES(?1,?2,?3,?4,?5)",params![key,hit as i64,input,cached,now])?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metrics_are_durable() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        record(&c, "k", true, 10, 8, 1).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM prompt_cache_metrics", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
