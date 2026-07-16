CREATE TABLE IF NOT EXISTS global_memory (
    id bigserial PRIMARY KEY,
    scope_key text NOT NULL,
    source_task_id uuid NULL REFERENCES tasks(id) ON DELETE SET NULL,
    note text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (scope_key, note)
);

CREATE INDEX IF NOT EXISTS idx_global_memory_scope_created
    ON global_memory (scope_key, created_at DESC);
