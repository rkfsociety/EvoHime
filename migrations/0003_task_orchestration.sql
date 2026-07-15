CREATE TABLE IF NOT EXISTS task_steps (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id uuid NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    step_index integer NOT NULL,
    tool_name text NOT NULL,
    input_json jsonb NOT NULL DEFAULT '{}'::jsonb,
    depends_on uuid[] NOT NULL DEFAULT '{}',
    status text NOT NULL DEFAULT 'pending',
    output text,
    error text,
    started_at timestamptz,
    completed_at timestamptz,
    UNIQUE (task_id, step_index)
);

CREATE TABLE IF NOT EXISTS task_checkpoints (
    task_id uuid PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    next_step integer NOT NULL DEFAULT 0,
    state_json jsonb NOT NULL DEFAULT '{}'::jsonb,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_task_steps_task_status ON task_steps(task_id, status);
