-- Ensure pgcrypto is available for uuid generation
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Versioned task execution graphs (v1, v2, ... when plan is regenerated after reflection)
CREATE TABLE IF NOT EXISTS task_execution_graphs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    version INT NOT NULL DEFAULT 1,  -- Incremented if plan is regenerated
    -- JSON array of step IDs in topological order: ["step-1", "step-2", "step-3"]
    topological_order JSONB NOT NULL,
    -- JSON object: { "step-id": ["dep-1", "dep-2"] } (for validation/display)
    dependency_map JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(task_id, session_id, version)
);

CREATE INDEX IF NOT EXISTS idx_task_execution_graphs_task_id ON task_execution_graphs(task_id);
CREATE INDEX IF NOT EXISTS idx_task_execution_graphs_task_version ON task_execution_graphs(task_id, version DESC);

-- Separate table for per-step execution state (not JSONB blob, enables efficient queries and indexing)
CREATE TABLE IF NOT EXISTS task_execution_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    graph_id UUID NOT NULL REFERENCES task_execution_graphs(id) ON DELETE CASCADE,
    step_id VARCHAR(255) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'pending', -- pending|running|completed|failed
    started_at TIMESTAMP WITH TIME ZONE,
    completed_at TIMESTAMP WITH TIME ZONE,
    error_message TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(graph_id, step_id)
);

CREATE INDEX IF NOT EXISTS idx_task_execution_steps_graph_id ON task_execution_steps(graph_id);
CREATE INDEX IF NOT EXISTS idx_task_execution_steps_status ON task_execution_steps(status) WHERE status IN ('pending', 'running');

COMMENT ON TABLE task_execution_graphs IS 'Versioned task dependency graphs. Version increments when plan is regenerated after reflection or error.';
COMMENT ON TABLE task_execution_steps IS 'Per-step execution tracking. Enables efficient status queries and per-step audit trail.';
