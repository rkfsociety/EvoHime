-- Distinguish push and pull attempts in cloud sync history (Stage 7.99, wave 3).

ALTER TABLE sync_runs
    ADD COLUMN IF NOT EXISTS direction text NOT NULL DEFAULT 'push'
    CHECK (direction IN ('push', 'pull'));
