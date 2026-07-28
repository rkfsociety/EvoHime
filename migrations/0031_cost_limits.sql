-- 7.108: Cost budgets & spend caps per day/model
-- Track daily spending limits and consumed tokens per model

CREATE TABLE cost_limits (
    id SERIAL PRIMARY KEY,
    model VARCHAR(255) NOT NULL UNIQUE,  -- e.g., "deepseek:free", "gpt-4"
    daily_cap_tokens BIGINT NOT NULL,    -- max tokens per day; 0 = unlimited
    reset_hour INT NOT NULL DEFAULT 0,   -- 0-23 UTC (when daily counter resets)
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Track daily spend (tokens consumed today)
CREATE TABLE cost_tracking (
    id SERIAL PRIMARY KEY,
    model VARCHAR(255) NOT NULL,
    date DATE NOT NULL,                  -- tracks which day (UTC)
    tokens_consumed BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(model, date),
    FOREIGN KEY(model) REFERENCES cost_limits(model) ON DELETE CASCADE
);

-- Index for quick lookup
CREATE INDEX idx_cost_tracking_model_date ON cost_tracking(model, date);
CREATE INDEX idx_cost_limits_model ON cost_limits(model);
