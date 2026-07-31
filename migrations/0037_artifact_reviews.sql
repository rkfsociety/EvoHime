CREATE TABLE IF NOT EXISTS artifact_reviews (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id uuid NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    session_id uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    artifact_kind text NOT NULL CHECK (artifact_kind IN ('spec', 'plan')),
    round_number integer NOT NULL CHECK (round_number >= 1),
    original_content text NOT NULL,
    reviewer_comments jsonb NOT NULL, -- array of {route_name, comments, failed}
    synthesized_feedback text NOT NULL,
    revised_content text NOT NULL,
    self_check_iterations integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS artifact_reviews_task_id_idx ON artifact_reviews (task_id);
