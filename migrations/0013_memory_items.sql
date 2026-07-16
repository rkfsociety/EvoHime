-- Structured memory items (roadmap 6.16).
-- Legacy session_memory / global_memory tables remain for compatibility.

CREATE TABLE IF NOT EXISTS memory_items (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    scope text NOT NULL,
    scope_key text NOT NULL,
    kind text NOT NULL,
    status text NOT NULL DEFAULT 'candidate',
    content text NOT NULL,
    content_json jsonb NULL,
    confidence double precision NOT NULL DEFAULT 0.5,
    importance double precision NOT NULL DEFAULT 0.5,
    pinned boolean NOT NULL DEFAULT false,
    source_session_id uuid NULL REFERENCES sessions(id) ON DELETE SET NULL,
    source_task_id uuid NULL REFERENCES tasks(id) ON DELETE SET NULL,
    source_label text NULL,
    supersedes uuid NULL REFERENCES memory_items(id) ON DELETE SET NULL,
    valid_until timestamptz NULL,
    validity_hint text NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT memory_items_scope_check CHECK (
        scope IN ('session', 'workspace', 'project', 'global', 'experience')
    ),
    CONSTRAINT memory_items_kind_check CHECK (
        kind IN (
            'fact',
            'preference',
            'constraint',
            'failure_pattern',
            'success_pattern',
            'verification_rule',
            'playbook'
        )
    ),
    CONSTRAINT memory_items_status_check CHECK (
        status IN ('candidate', 'active', 'conflict', 'archived', 'rejected')
    ),
    CONSTRAINT memory_items_confidence_check CHECK (confidence >= 0 AND confidence <= 1),
    CONSTRAINT memory_items_importance_check CHECK (importance >= 0 AND importance <= 1),
    CONSTRAINT memory_items_content_nonempty CHECK (btrim(content) <> ''),
    CONSTRAINT memory_items_scope_key_nonempty CHECK (btrim(scope_key) <> '')
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_items_source_label_unique
    ON memory_items (source_label)
    WHERE source_label IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_memory_items_scope_key_status
    ON memory_items (scope, scope_key, status);

CREATE INDEX IF NOT EXISTS idx_memory_items_retrieval
    ON memory_items (scope, scope_key, status, pinned DESC, importance DESC, updated_at DESC);
