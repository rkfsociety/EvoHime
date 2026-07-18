CREATE TABLE IF NOT EXISTS sites (
    id UUID PRIMARY KEY,
    workspace_path TEXT NOT NULL,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_path, slug)
);

CREATE INDEX IF NOT EXISTS sites_workspace_updated_idx
    ON sites (workspace_path, updated_at DESC);
