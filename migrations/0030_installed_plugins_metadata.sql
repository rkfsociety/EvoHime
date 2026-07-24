-- Plugin install metadata tracking (Stage 7.8)
-- Extends filesystem-based lock file with operator-scoped DB metadata for:
-- - pin tracking (commit_sha, version override)
-- - signature verification (future: Ed25519 signatures)
-- - uninstall status (active, uninstalled but recoverable, quarantined)

CREATE TABLE IF NOT EXISTS installed_plugins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operator_id UUID NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    pinned_commit TEXT,              -- git commit SHA, if explicitly pinned (overrides catalog tag)
    pinned_version TEXT,             -- semantic version pin (e.g. "1.2.3"), if set
    signature_hash TEXT,             -- Ed25519 signature hash of installed tree (7.8 future)
    status TEXT NOT NULL DEFAULT 'active',  -- 'active' | 'uninstalled' | 'quarantined'
    installed_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE(operator_id, name)
);

CREATE INDEX idx_installed_plugins_operator ON installed_plugins(operator_id);
CREATE INDEX idx_installed_plugins_status ON installed_plugins(status);

-- Record when a plugin is uninstalled (soft-delete, keeps history for restore)
ALTER TABLE installed_plugins ADD COLUMN IF NOT EXISTS uninstalled_at TIMESTAMPTZ;
