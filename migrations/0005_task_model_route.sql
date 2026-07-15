ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS model_route text NULL;
