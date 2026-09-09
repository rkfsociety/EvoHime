use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 102 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS context_namespace_nodes (node_id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, node_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS context_namespace_projections (node_id TEXT NOT NULL, level TEXT NOT NULL, source_revision INTEGER NOT NULL, projection_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(node_id, level)); CREATE TABLE IF NOT EXISTS context_namespace_views (view_id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, view_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS context_namespace_traces (trace_id TEXT PRIMARY KEY NOT NULL, run_id TEXT NOT NULL, trace_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS context_namespace_idempotency (scope TEXT NOT NULL, idempotency_key TEXT NOT NULL, command_hash TEXT NOT NULL, response_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(scope, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_context_namespace_nodes_revision ON context_namespace_nodes(revision); CREATE INDEX IF NOT EXISTS idx_context_namespace_traces_run ON context_namespace_traces(run_id, created_at_ms DESC); PRAGMA user_version = 102;")?;
    }
    Ok(())
}
