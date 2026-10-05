use rusqlite::Transaction;

/// Installs metadata-only local model adaptation job and publication records.
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 178 {
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS local_model_adaptation_jobs (
                job_id TEXT PRIMARY KEY NOT NULL,
                revision INTEGER NOT NULL CHECK(revision > 0),
                state TEXT NOT NULL,
                idempotency_key TEXT NOT NULL UNIQUE,
                request_hash TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                request_json BLOB NOT NULL,
                snapshot_json BLOB NOT NULL,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_local_model_adaptation_jobs_state
                ON local_model_adaptation_jobs(state,updated_at_ms);
            CREATE TABLE IF NOT EXISTS local_model_adaptation_publications (
                job_id TEXT PRIMARY KEY NOT NULL REFERENCES local_model_adaptation_jobs(job_id),
                model_id TEXT NOT NULL,
                model_revision INTEGER NOT NULL CHECK(model_revision > 0),
                artifact_relative_path TEXT NOT NULL,
                artifact_hash TEXT NOT NULL,
                artifact_size_bytes INTEGER NOT NULL CHECK(artifact_size_bytes > 0),
                registry_state TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_local_model_adaptation_publications_state
                ON local_model_adaptation_publications(registry_state,updated_at_ms);
            CREATE TABLE IF NOT EXISTS local_model_adaptation_benchmark_inputs (
                job_id TEXT PRIMARY KEY NOT NULL REFERENCES local_model_adaptation_jobs(job_id),
                input_hash TEXT NOT NULL,
                input_json BLOB NOT NULL,
                updated_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS local_model_adaptation_disk_reservations (
                job_id TEXT PRIMARY KEY NOT NULL REFERENCES local_model_adaptation_jobs(job_id),
                reserved_bytes INTEGER NOT NULL CHECK(reserved_bytes > 0),
                updated_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS benchmark_baseline_approvals (
                baseline_id TEXT PRIMARY KEY NOT NULL REFERENCES benchmark_baselines(baseline_id),
                owner_scope TEXT NOT NULL,
                run_id TEXT NOT NULL,
                report_sha256 TEXT NOT NULL,
                idempotency_key TEXT NOT NULL UNIQUE,
                approved_at_ms INTEGER NOT NULL
            );
            PRAGMA user_version = 178;",
        )?;
    }
    Ok(())
}
