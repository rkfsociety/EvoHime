CREATE TABLE IF NOT EXISTS worker_jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    worker_job_id text,
    task text NOT NULL,
    payload_json jsonb NOT NULL,
    status text NOT NULL DEFAULT 'queued',
    attempts integer NOT NULL DEFAULT 0,
    result_json jsonb,
    error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz
);

CREATE INDEX IF NOT EXISTS worker_jobs_status_idx ON worker_jobs(status, created_at);
