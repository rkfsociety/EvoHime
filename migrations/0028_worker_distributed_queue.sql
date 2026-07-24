-- Distributed worker queue support for horizontal scaling (Stage 7.54).
-- Adds heartbeat tracking and claimed_at timestamp for detecting stale workers.

ALTER TABLE worker_jobs
    ADD COLUMN IF NOT EXISTS claimed_at timestamptz,
    ADD COLUMN IF NOT EXISTS heartbeat_at timestamptz;

CREATE INDEX IF NOT EXISTS worker_jobs_status_claimed_idx
    ON worker_jobs (status, claimed_at DESC)
    WHERE status IN ('running', 'retrying');

-- Index for recovery queries: stale workers not responding within timeout.
CREATE INDEX IF NOT EXISTS worker_jobs_heartbeat_idx
    ON worker_jobs (heartbeat_at)
    WHERE status = 'running';
