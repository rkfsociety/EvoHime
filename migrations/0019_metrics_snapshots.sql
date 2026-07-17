-- Periodic pipeline + worker metrics snapshots (Stage 7.24).

CREATE TABLE IF NOT EXISTS metrics_snapshots (
    id bigserial PRIMARY KEY,
    captured_at timestamptz NOT NULL DEFAULT now(),
    pipeline jsonb NOT NULL,
    worker jsonb NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_metrics_snapshots_captured_at
    ON metrics_snapshots (captured_at DESC);
