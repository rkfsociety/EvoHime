-- 8.1: Tree-of-Thoughts planning history tracking.
-- Stores candidate plans, scoring details, and reasoning for each task's planning decisions.

CREATE TABLE IF NOT EXISTS planning_history (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id uuid NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    session_id uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    candidates jsonb NOT NULL, -- Array of PlanCandidate objects
    chosen_plan_id text,
    reasoning text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT planning_history_reasoning_length CHECK (length(reasoning) <= 512)
);

CREATE INDEX IF NOT EXISTS idx_planning_history_task_id ON planning_history (task_id);
CREATE INDEX IF NOT EXISTS idx_planning_history_created_at ON planning_history (created_at);
