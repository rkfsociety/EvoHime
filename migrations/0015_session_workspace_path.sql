ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS workspace_path text NULL;

UPDATE sessions AS session
SET workspace_path = task.workspace_path
FROM (
    SELECT DISTINCT ON (session_id) session_id, workspace_path
    FROM tasks
    WHERE workspace_path IS NOT NULL
    ORDER BY session_id, created_at DESC
) AS task
WHERE session.id = task.session_id
  AND session.workspace_path IS NULL;
