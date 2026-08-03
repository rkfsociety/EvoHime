-- Stage 8.4: Meta-cognitive confidence signals infrastructure

-- Tool execution statistics for success rate calculation
CREATE TABLE IF NOT EXISTS tool_execution_stats (
    id BIGSERIAL PRIMARY KEY,
    tool_name VARCHAR(50) NOT NULL,
    operation_type VARCHAR(100),
    success BOOLEAN NOT NULL,
    error_category VARCHAR(50),
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    workspace_path TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_tool_stats_tool_created ON tool_execution_stats(tool_name, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tool_stats_task ON tool_execution_stats(task_id);
CREATE INDEX IF NOT EXISTS idx_tool_stats_workspace ON tool_execution_stats(workspace_path);

-- Extend reflection_events with revision metadata
ALTER TABLE reflection_events ADD COLUMN IF NOT EXISTS revision_type VARCHAR(50)
    CHECK (revision_type IS NULL OR revision_type IN ('minor', 'major', 'repeated_failure'));
ALTER TABLE reflection_events ADD COLUMN IF NOT EXISTS confidence_delta NUMERIC(3, 2)
    CHECK (confidence_delta IS NULL OR (confidence_delta >= -1 AND confidence_delta <= 1));

CREATE INDEX IF NOT EXISTS idx_reflection_revision_type ON reflection_events(task_id, revision_type) WHERE revision_type IS NOT NULL;

-- Extend memory_items with model confidence at creation time
ALTER TABLE memory_items ADD COLUMN IF NOT EXISTS model_confidence_at_creation NUMERIC(3, 2)
    DEFAULT 0.5
    CHECK (model_confidence_at_creation >= 0 AND model_confidence_at_creation <= 1);

-- Confidence computation audit log
CREATE TABLE IF NOT EXISTS confidence_audit_log (
    id BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    session_id UUID REFERENCES sessions(id) ON DELETE CASCADE,
    confidence_score NUMERIC(3, 2) NOT NULL CHECK (confidence_score >= 0 AND confidence_score <= 1),
    risk_level VARCHAR(20) NOT NULL CHECK (risk_level IN ('none', 'low', 'medium', 'high')),
    confidence_version VARCHAR(10) NOT NULL DEFAULT '1',
    breakdown JSONB NOT NULL DEFAULT '{}',
    -- breakdown structure: {model: {score, reliability}, experience: {...}, tools: {...}, reflection: {...}}
    reliability_scores JSONB NOT NULL DEFAULT '{}',
    -- {model: "high"|"medium"|"low"|"very_low", experience: ..., tools: ..., reflection: ...}
    missing_signals TEXT[] DEFAULT ARRAY[]::TEXT[],
    decision VARCHAR(20) NOT NULL CHECK (decision IN ('proceed', 'ask', 'require_approval')),
    force_approved BOOLEAN DEFAULT FALSE,
    force_approval_reason TEXT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_confidence_audit_task ON confidence_audit_log(task_id);
CREATE INDEX IF NOT EXISTS idx_confidence_audit_session ON confidence_audit_log(session_id);
CREATE INDEX IF NOT EXISTS idx_confidence_audit_timestamp ON confidence_audit_log(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_confidence_audit_risk_decision ON confidence_audit_log(risk_level, decision);
