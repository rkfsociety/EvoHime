-- 7.107: Worktree-aware multi-checkout agent.
-- Tracks isolated git worktrees allocated to concurrently-running tasks so
-- merge-back and server-restart cleanup can find them without scanning disk.

CREATE TABLE IF NOT EXISTS task_worktrees (
    task_id uuid PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    base_commit_sha text NOT NULL,
    worktree_path text NOT NULL UNIQUE,
    primary_workspace_root text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_task_worktrees_created_at ON task_worktrees (created_at);
