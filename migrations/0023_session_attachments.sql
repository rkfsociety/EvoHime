CREATE TABLE IF NOT EXISTS session_attachments (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    task_id UUID REFERENCES tasks(id) ON DELETE SET NULL,
    workspace_path TEXT NOT NULL,
    original_name TEXT NOT NULL,
    stored_path TEXT NOT NULL,
    mime_type TEXT,
    size_bytes BIGINT NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS session_attachments_session_created_idx
    ON session_attachments (session_id, created_at DESC);

CREATE INDEX IF NOT EXISTS session_attachments_pending_idx
    ON session_attachments (session_id, created_at ASC)
    WHERE consumed_at IS NULL;
