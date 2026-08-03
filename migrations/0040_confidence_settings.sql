-- Store confidence gate settings (thresholds) in database

CREATE TABLE IF NOT EXISTS confidence_settings (
    id SERIAL PRIMARY KEY,
    operator_id UUID NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
    setting_key VARCHAR(100) NOT NULL,
    setting_value JSONB NOT NULL,
    version INT DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(operator_id, setting_key)
);

-- Default settings for new operators
INSERT INTO confidence_settings (operator_id, setting_key, setting_value)
SELECT id, 'confidence_thresholds', '{"version":"1","risk_none":{"proceed":0.65,"ask":0.40},"risk_low":{"proceed":0.70,"ask":0.45},"risk_medium":{"proceed":0.75,"ask":0.50},"risk_high":{"proceed":0.85,"ask":0.65,"require":0.30},"missing_signal_ask_threshold":0.5}'::jsonb
FROM operators
WHERE id NOT IN (SELECT operator_id FROM confidence_settings WHERE setting_key = 'confidence_thresholds')
ON CONFLICT (operator_id, setting_key) DO NOTHING;

CREATE INDEX IF NOT EXISTS idx_confidence_settings_operator ON confidence_settings(operator_id);
