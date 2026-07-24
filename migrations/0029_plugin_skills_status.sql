-- Plugin skills enable/disable management (Stage 7.9)
-- Tracks which skills from installed plugins are enabled (default: enabled)
-- Disabled skills are filtered out of agent system prompt

CREATE TABLE IF NOT EXISTS plugin_skills (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operator_id UUID NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
    plugin_id TEXT NOT NULL,         -- plugin name from .evohime/plugins/{plugin_id}
    skill_name TEXT NOT NULL,        -- skill directory name (e.g. "using-superpowers")
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE(operator_id, plugin_id, skill_name)
);

CREATE INDEX idx_plugin_skills_operator ON plugin_skills(operator_id);
CREATE INDEX idx_plugin_skills_plugin ON plugin_skills(plugin_id);
