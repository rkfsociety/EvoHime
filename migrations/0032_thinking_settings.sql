-- Wave 3B: Extended Reasoning settings (thinking budget, enabled/disabled, model preferences)

CREATE TABLE IF NOT EXISTS thinking_settings (
    id SERIAL PRIMARY KEY,

    -- Thinking configuration
    enabled BOOLEAN NOT NULL DEFAULT false,
    budget_tokens INTEGER DEFAULT 5000,  -- Default budget for thinking
    max_budget_tokens INTEGER DEFAULT 32000,  -- Maximum allowed budget

    -- UI preferences
    show_thinking BOOLEAN NOT NULL DEFAULT true,  -- Display thinking in UI
    thinking_verbosity VARCHAR(50) DEFAULT 'compact',  -- 'compact', 'full', 'summary'

    -- Cost tracking
    monthly_thinking_cost_limit DECIMAL(10, 2) DEFAULT 50.00,  -- USD
    warning_threshold_percent INT DEFAULT 80,  -- Alert at 80% of budget

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Track thinking usage per session for cost estimation
CREATE TABLE IF NOT EXISTS thinking_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    operator_id UUID NOT NULL REFERENCES operators(id) ON DELETE CASCADE,

    task_id UUID REFERENCES tasks(id) ON DELETE SET NULL,

    -- Usage metrics
    thinking_tokens INTEGER NOT NULL,
    thinking_duration_ms INTEGER,  -- How long reasoning took
    final_content_tokens INTEGER,  -- Tokens in final answer

    -- Cost estimation (OpenAI pricing: ~0.000003 per thinking token)
    estimated_cost_usd DECIMAL(10, 6),

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_thinking_usage_session_id ON thinking_usage(session_id);
CREATE INDEX idx_thinking_usage_operator_id ON thinking_usage(operator_id);
CREATE INDEX idx_thinking_usage_created_at ON thinking_usage(created_at);
