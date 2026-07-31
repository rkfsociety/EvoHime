CREATE TABLE reflection_events (
    id BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    task_id UUID NOT NULL REFERENCES task_history(id) ON DELETE CASCADE,
    tool_call_id UUID,
    reflection_type VARCHAR(50) NOT NULL,
    reflection_action VARCHAR(50) NOT NULL,
    success_score NUMERIC(3, 2) NOT NULL CHECK (success_score >= 0 AND success_score <= 1),
    error_patterns JSONB NOT NULL DEFAULT '[]',
    confidence NUMERIC(3, 2) NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
    reasoning TEXT NOT NULL,
    recommendation TEXT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_reflection_task_id ON reflection_events(task_id);
CREATE INDEX idx_reflection_timestamp ON reflection_events(timestamp);
CREATE INDEX idx_reflection_tool_call ON reflection_events(tool_call_id);
