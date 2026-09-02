use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS workflow_optimization_runs (run_id TEXT PRIMARY KEY, run_json BLOB NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS workflow_optimization_candidates (candidate_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, candidate_json BLOB NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS workflow_optimization_evaluations (evaluation_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, evaluation_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put_run(
    c: &Connection,
    id: &str,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO workflow_optimization_runs(run_id,run_json,content_hash,updated_at_ms) VALUES(?1,?2,?3,?4)",params![id,json,hash,now])?==1)
}
pub fn get_run(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT run_json FROM workflow_optimization_runs WHERE run_id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()
}
pub fn put_candidate(
    c: &Connection,
    id: &str,
    run: &str,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO workflow_optimization_candidates(candidate_id,run_id,candidate_json,content_hash,updated_at_ms) VALUES(?1,?2,?3,?4,?5)",params![id,run,json,hash,now])?==1)
}
