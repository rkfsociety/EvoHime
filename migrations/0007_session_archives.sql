ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS archived_at timestamptz NULL;

CREATE INDEX IF NOT EXISTS idx_sessions_archived_created
    ON sessions (archived_at, created_at DESC);
