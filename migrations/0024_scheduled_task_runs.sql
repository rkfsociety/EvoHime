ALTER TABLE scheduled_tasks
    ADD COLUMN IF NOT EXISTS failure_count BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_run_status TEXT,
    ADD COLUMN IF NOT EXISTS last_run_error TEXT;

ALTER TABLE scheduled_tasks
    DROP CONSTRAINT IF EXISTS scheduled_tasks_last_run_status_check;

ALTER TABLE scheduled_tasks
    ADD CONSTRAINT scheduled_tasks_last_run_status_check
    CHECK (last_run_status IS NULL OR last_run_status IN ('dispatched', 'failed'));

CREATE TABLE IF NOT EXISTS scheduled_task_runs (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    scheduled_task_id UUID NOT NULL REFERENCES scheduled_tasks(id) ON DELETE CASCADE,
    due_at            TIMESTAMPTZ NOT NULL,
    trigger_kind      TEXT NOT NULL CHECK (trigger_kind IN ('cron', 'manual')),
    status            TEXT NOT NULL CHECK (status IN ('dispatched', 'failed')),
    session_id        UUID REFERENCES sessions(id) ON DELETE SET NULL,
    task_id           UUID REFERENCES tasks(id) ON DELETE SET NULL,
    error             TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at      TIMESTAMPTZ,
    UNIQUE (scheduled_task_id, due_at, trigger_kind)
);

CREATE INDEX IF NOT EXISTS scheduled_task_runs_task_idx
    ON scheduled_task_runs (scheduled_task_id, created_at DESC);
