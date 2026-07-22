-- Local multi-operator registry and ownership scopes (Stage 7.98).

CREATE TABLE IF NOT EXISTS operators (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL UNIQUE,
    role text NOT NULL CHECK (role IN ('owner', 'member')),
    token_hash text UNIQUE,
    active boolean NOT NULL DEFAULT TRUE,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NULL
);

INSERT INTO operators (id, name, role, token_hash, active)
VALUES ('00000000-0000-0000-0000-000000000001', 'local-owner', 'owner', NULL, TRUE)
ON CONFLICT (id) DO NOTHING;

ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS operator_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001';
ALTER TABLE memory_items
    ADD COLUMN IF NOT EXISTS operator_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001';
ALTER TABLE scheduled_tasks
    ADD COLUMN IF NOT EXISTS operator_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001';
ALTER TABLE sites
    ADD COLUMN IF NOT EXISTS operator_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001';
ALTER TABLE permission_approval_audit
    ADD COLUMN IF NOT EXISTS operator_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001';

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'sessions_operator_id_fkey') THEN
        ALTER TABLE sessions ADD CONSTRAINT sessions_operator_id_fkey
            FOREIGN KEY (operator_id) REFERENCES operators(id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'memory_items_operator_id_fkey') THEN
        ALTER TABLE memory_items ADD CONSTRAINT memory_items_operator_id_fkey
            FOREIGN KEY (operator_id) REFERENCES operators(id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'scheduled_tasks_operator_id_fkey') THEN
        ALTER TABLE scheduled_tasks ADD CONSTRAINT scheduled_tasks_operator_id_fkey
            FOREIGN KEY (operator_id) REFERENCES operators(id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'sites_operator_id_fkey') THEN
        ALTER TABLE sites ADD CONSTRAINT sites_operator_id_fkey
            FOREIGN KEY (operator_id) REFERENCES operators(id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'permission_approval_audit_operator_id_fkey') THEN
        ALTER TABLE permission_approval_audit ADD CONSTRAINT permission_approval_audit_operator_id_fkey
            FOREIGN KEY (operator_id) REFERENCES operators(id);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_sessions_operator_created
    ON sessions (operator_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_items_operator_updated
    ON memory_items (operator_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_operator_created
    ON scheduled_tasks (operator_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_sites_operator_created
    ON sites (operator_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_permission_approval_audit_operator_at
    ON permission_approval_audit (operator_id, at_ms DESC);
