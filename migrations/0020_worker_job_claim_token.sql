-- Lease / claim token for worker job attempts (Stage 7.26).
-- Completions and retries must match claim_token to avoid dual-state races
-- between PostgreSQL rows and the Python in-memory queue.

ALTER TABLE worker_jobs
    ADD COLUMN IF NOT EXISTS claim_token uuid;

CREATE INDEX IF NOT EXISTS worker_jobs_claim_token_idx
    ON worker_jobs (claim_token)
    WHERE claim_token IS NOT NULL;
