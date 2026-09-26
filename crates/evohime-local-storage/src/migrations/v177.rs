use rusqlite::Transaction;

/// Installs bounded image-generation job metadata without modifying existing rows.
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 177 {
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS image_generation_jobs (
                job_id TEXT PRIMARY KEY NOT NULL,
                task_id TEXT NOT NULL,
                idempotency_key TEXT NOT NULL,
                request_hash TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN (
                    'preflight','queued','dispatched','completed','failed','cancelled','unknown_outcome'
                )),
                revision INTEGER NOT NULL CHECK(revision > 0),
                snapshot_json BLOB NOT NULL,
                result_json BLOB,
                error_code TEXT,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                UNIQUE(task_id,idempotency_key)
            );
            CREATE INDEX IF NOT EXISTS idx_image_generation_jobs_task
                ON image_generation_jobs(task_id,created_at_ms);
            CREATE INDEX IF NOT EXISTS idx_image_generation_jobs_recovery
                ON image_generation_jobs(state,updated_at_ms);
            PRAGMA user_version = 177;",
        )?;
    }
    Ok(())
}
