CREATE TABLE IF NOT EXISTS session_messages (
    id bigserial PRIMARY KEY,
    session_id uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    task_id uuid NULL REFERENCES tasks(id) ON DELETE SET NULL,
    role text NOT NULL,
    content text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_session_messages_session_created
    ON session_messages (session_id, created_at);
