//! Durable metadata-only storage for the multi-reviewer ensemble.

use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Maximum serialized profile and run payload size accepted by this store.
pub const MAX_JSON_BYTES: usize = 256 * 1024;

/// Creates profile, run, cluster, and adjudication tables for reviewer ensembles.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS reviewer_ensemble_profiles (id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, profile_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(id, revision)); CREATE TABLE IF NOT EXISTS reviewer_ensemble_runs (id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, profile_id TEXT NOT NULL, profile_revision INTEGER NOT NULL, content_hash TEXT NOT NULL, run_json BLOB NOT NULL, status TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS reviewer_ensemble_clusters (id TEXT PRIMARY KEY NOT NULL, run_id TEXT NOT NULL, content_hash TEXT NOT NULL, cluster_json BLOB NOT NULL, classification TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS reviewer_ensemble_adjudications (id TEXT PRIMARY KEY NOT NULL, cluster_id TEXT NOT NULL, content_hash TEXT NOT NULL, result_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_reviewer_ensemble_runs_profile ON reviewer_ensemble_runs(profile_id, profile_revision); CREATE INDEX IF NOT EXISTS idx_reviewer_ensemble_clusters_run ON reviewer_ensemble_clusters(run_id);")
}

/// Inserts a validated profile revision once; JSON payloads are limited to [`MAX_JSON_BYTES`].
pub fn put_profile(
    connection: &Connection,
    id: &str,
    revision: u64,
    hash: &str,
    json: &[u8],
    now_ms: i64,
) -> Result<bool, &'static str> {
    if id.trim().is_empty()
        || revision == 0
        || hash.trim().is_empty()
        || json.is_empty()
        || json.len() > MAX_JSON_BYTES
        || now_ms <= 0
    {
        return Err("invalid ensemble profile");
    }
    connection.execute("INSERT INTO reviewer_ensemble_profiles(id,revision,content_hash,profile_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id,revision) DO NOTHING", params![id, revision as i64, hash, json, now_ms]).map(|count| count == 1).map_err(|_| "sqlite")
}

/// Inserts or advances a run when the incoming revision is newer or an identical replay.
/// Rejects empty fields, invalid timestamps, and payloads larger than [`MAX_JSON_BYTES`].
#[allow(clippy::too_many_arguments)]
pub fn put_run(
    connection: &Connection,
    id: &str,
    profile_id: &str,
    revision: u64,
    hash: &str,
    json: &[u8],
    status: &str,
    now_ms: i64,
) -> Result<bool, &'static str> {
    if id.trim().is_empty()
        || profile_id.trim().is_empty()
        || revision == 0
        || hash.trim().is_empty()
        || json.is_empty()
        || json.len() > MAX_JSON_BYTES
        || status.trim().is_empty()
        || now_ms <= 0
    {
        return Err("invalid ensemble run");
    }
    connection.execute("INSERT INTO reviewer_ensemble_runs(id,revision,profile_id,profile_revision,content_hash,run_json,status,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,content_hash=excluded.content_hash,run_json=excluded.run_json,status=excluded.status,updated_at_ms=excluded.updated_at_ms WHERE excluded.revision > reviewer_ensemble_runs.revision OR (excluded.revision = reviewer_ensemble_runs.revision AND excluded.content_hash = reviewer_ensemble_runs.content_hash)", params![id, revision as i64, profile_id, revision as i64, hash, json, status, now_ms]).map(|count| count == 1).map_err(|_| "sqlite")
}

/// Loads a run's serialized record and status, if the run exists.
pub fn get_run(connection: &Connection, id: &str) -> rusqlite::Result<Option<(Vec<u8>, String)>> {
    connection
        .query_row(
            "SELECT run_json,status FROM reviewer_ensemble_runs WHERE id=?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn run_upsert_is_bounded_and_idempotent() {
        let c = Connection::open_in_memory().unwrap();
        let tx = c.unchecked_transaction().unwrap();
        install_schema(&tx).unwrap();
        tx.commit().unwrap();
        assert!(put_run(&c, "run", "p", 1, "hash", b"{}", "queued", 1).unwrap());
        assert!(put_run(&c, "run", "p", 1, "hash", b"{}", "running", 2).unwrap());
        assert_eq!(get_run(&c, "run").unwrap().unwrap().1, "running");
    }
}
