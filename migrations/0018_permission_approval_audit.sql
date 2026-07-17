-- Durable permission approval audit (Stage 7.23).

CREATE TABLE IF NOT EXISTS permission_approval_audit (
    id bigserial PRIMARY KEY,
    approval_id uuid NOT NULL,
    task_id uuid NOT NULL,
    session_id uuid NULL,
    tool_name text NOT NULL,
    permission text NOT NULL,
    scope text NOT NULL,
    decision text NOT NULL,
    at_ms bigint NOT NULL,
    remembered_path boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_permission_approval_audit_at_ms
    ON permission_approval_audit (at_ms DESC);

CREATE INDEX IF NOT EXISTS idx_permission_approval_audit_approval
    ON permission_approval_audit (approval_id);
