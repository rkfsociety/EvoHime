ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS workspace_path text NULL;
