-- Durable plugin marketplace audit trail (Stage 7.113, Wave 4B: Trust & Reputation).
-- Records every install/update/uninstall/pin event plus force-override of a
-- risk-scan block, so operators can reconstruct "who installed what, when,
-- and did they bypass a warning" without grepping server logs.

CREATE TABLE IF NOT EXISTS plugin_audit (
    id bigserial PRIMARY KEY,
    operator_id uuid NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
    plugin_name text NOT NULL,
    action text NOT NULL,               -- 'install' | 'update' | 'uninstall' | 'pin' | 'force_override'
    trust_level text,                   -- trust level at time of action, if known
    risk_findings_count integer NOT NULL DEFAULT 0,
    force_used boolean NOT NULL DEFAULT false,
    details text,                       -- free-form context (e.g. pinned commit/version, error summary)
    at_ms bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_plugin_audit_at_ms
    ON plugin_audit (at_ms DESC);

CREATE INDEX IF NOT EXISTS idx_plugin_audit_plugin_name
    ON plugin_audit (plugin_name);

CREATE INDEX IF NOT EXISTS idx_plugin_audit_operator
    ON plugin_audit (operator_id);
