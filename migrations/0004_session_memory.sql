CREATE TABLE IF NOT EXISTS session_memory (
    id bigserial PRIMARY KEY,
    session_id uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    source_task_id uuid NULL REFERENCES tasks(id) ON DELETE SET NULL,
    note text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_session_memory_session_created
    ON session_memory (session_id, created_at);
