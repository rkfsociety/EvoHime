-- Cloud sync push run history (Stage 7.99, wave 1).

CREATE TABLE IF NOT EXISTS sync_runs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    operator_id uuid NOT NULL REFERENCES operators(id),
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz NULL,
    status text NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'success', 'failed')),
    bytes_total bigint NULL,
    checksum text NULL,
    error text NULL
);

CREATE INDEX IF NOT EXISTS idx_sync_runs_operator_started
    ON sync_runs (operator_id, started_at DESC);
