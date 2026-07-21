CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id            UUID PRIMARY KEY,
    workspace_path TEXT NOT NULL,
    title         TEXT NOT NULL,
    prompt        TEXT NOT NULL,
    -- cron expression, e.g. "0 8 * * 1-5"
    cron_expr     TEXT NOT NULL,
    -- 'active' | 'paused'
    status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused')),
    -- null = never ran
    last_run_at   TIMESTAMPTZ,
    next_run_at   TIMESTAMPTZ NOT NULL,
    run_count     BIGINT NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS scheduled_tasks_workspace_next_idx
    ON scheduled_tasks (workspace_path, next_run_at ASC)
    WHERE status = 'active';
