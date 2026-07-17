-- Memory feedback counters and usage tracking (roadmap 6.23).

ALTER TABLE memory_items
    ADD COLUMN IF NOT EXISTS last_used_at timestamptz NULL,
    ADD COLUMN IF NOT EXISTS use_count integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS helpful_count integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS harmful_count integer NOT NULL DEFAULT 0;

ALTER TABLE memory_items
    DROP CONSTRAINT IF EXISTS memory_items_use_count_check,
    DROP CONSTRAINT IF EXISTS memory_items_helpful_count_check,
    DROP CONSTRAINT IF EXISTS memory_items_harmful_count_check;

ALTER TABLE memory_items
    ADD CONSTRAINT memory_items_use_count_check CHECK (use_count >= 0),
    ADD CONSTRAINT memory_items_helpful_count_check CHECK (helpful_count >= 0),
    ADD CONSTRAINT memory_items_harmful_count_check CHECK (harmful_count >= 0);

CREATE TABLE IF NOT EXISTS memory_feedback_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    memory_id uuid NOT NULL REFERENCES memory_items(id) ON DELETE CASCADE,
    task_id uuid NULL REFERENCES tasks(id) ON DELETE SET NULL,
    signal text NOT NULL,
    delta_confidence double precision NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT memory_feedback_signal_check CHECK (
        signal IN ('used', 'helpful', 'harmful', 'corrected', 'rejected', 'idle_decay')
    )
);

CREATE INDEX IF NOT EXISTS idx_memory_feedback_memory_created
    ON memory_feedback_events (memory_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_memory_items_last_used
    ON memory_items (last_used_at NULLS FIRST, confidence ASC);
