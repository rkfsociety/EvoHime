ALTER TABLE worker_jobs
    ADD COLUMN IF NOT EXISTS max_attempts integer NOT NULL DEFAULT 3;

UPDATE worker_jobs
SET max_attempts = 3
WHERE max_attempts IS NULL OR max_attempts < 1;
