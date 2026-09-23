//! Durable metadata registry; actual worktree bytes remain in Git.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Durable metadata identifying a task's Git worktree and lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWorktreeRecord {
    /// Stable worktree record identifier.
    pub worktree_id: String,
    /// Codex task associated with the worktree.
    pub task_id: String,
    /// Repository identity or isolation scope.
    pub repository_scope: String,
    /// Branch checked out in the worktree.
    pub branch: String,
    /// Git ref from which the worktree was created.
    pub root_ref: String,
    /// Base commit recorded when the worktree was created.
    pub base_commit: String,
    /// Lifecycle state such as planned or ready.
    pub state: String,
    /// Optimistic-lock version.
    pub version: u64,
    /// Unique key making creation retries safe.
    pub idempotency_key: String,
    /// Last metadata update timestamp in milliseconds.
    pub updated_at_ms: i64,
}
/// Creates the task worktree metadata table and task lookup index.
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS task_worktrees (worktree_id TEXT PRIMARY KEY, task_id TEXT NOT NULL, repository_scope TEXT NOT NULL, branch TEXT NOT NULL, root_ref TEXT NOT NULL, base_commit TEXT NOT NULL, state TEXT NOT NULL, version INTEGER NOT NULL, idempotency_key TEXT NOT NULL UNIQUE, updated_at_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_task_worktrees_task ON task_worktrees(task_id);")
}
/// Inserts a worktree record once; duplicate identifiers or idempotency keys return `false`.
pub fn create(c: &Connection, r: &TaskWorktreeRecord) -> rusqlite::Result<bool> {
    Ok(c.execute(
        "INSERT OR IGNORE INTO task_worktrees VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            r.worktree_id,
            r.task_id,
            r.repository_scope,
            r.branch,
            r.root_ref,
            r.base_commit,
            r.state,
            r.version as i64,
            r.idempotency_key,
            r.updated_at_ms
        ],
    )? == 1)
}
/// Loads a worktree record by its identifier, if present.
pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<TaskWorktreeRecord>> {
    c.query_row("SELECT worktree_id,task_id,repository_scope,branch,root_ref,base_commit,state,version,idempotency_key,updated_at_ms FROM task_worktrees WHERE worktree_id=?1", params![id], |row| Ok(TaskWorktreeRecord { worktree_id: row.get(0)?,task_id: row.get(1)?,repository_scope: row.get(2)?,branch: row.get(3)?,root_ref: row.get(4)?,base_commit: row.get(5)?,state: row.get(6)?,version: row.get::<_,i64>(7)? as u64,idempotency_key: row.get(8)?,updated_at_ms: row.get(9)? })).optional()
}

/// Returns the most recently updated ready worktree for a task, if present.
pub fn get_ready_for_task(
    c: &Connection,
    task_id: &str,
) -> rusqlite::Result<Option<TaskWorktreeRecord>> {
    c.query_row("SELECT worktree_id,task_id,repository_scope,branch,root_ref,base_commit,state,version,idempotency_key,updated_at_ms FROM task_worktrees WHERE task_id=?1 AND state='ready' ORDER BY updated_at_ms DESC LIMIT 1", params![task_id], |row| Ok(TaskWorktreeRecord { worktree_id: row.get(0)?, task_id: row.get(1)?, repository_scope: row.get(2)?, branch: row.get(3)?, root_ref: row.get(4)?, base_commit: row.get(5)?, state: row.get(6)?, version: row.get::<_, i64>(7)? as u64, idempotency_key: row.get(8)?, updated_at_ms: row.get(9)? })).optional()
}
/// Changes a worktree state only when its stored version matches `expected_version`.
pub fn transition(
    c: &Connection,
    id: &str,
    expected_version: u64,
    state: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("UPDATE task_worktrees SET state=?1,version=version+1,updated_at_ms=?2 WHERE worktree_id=?3 AND version=?4", params![state,now,id,expected_version as i64])? == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_and_stale_are_fenced() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let r = TaskWorktreeRecord {
            worktree_id: "w".into(),
            task_id: "t".into(),
            repository_scope: "r".into(),
            branch: "b".into(),
            root_ref: "root".into(),
            base_commit: "a".into(),
            state: "planned".into(),
            version: 1,
            idempotency_key: "i".into(),
            updated_at_ms: 1,
        };
        assert!(create(&c, &r).unwrap());
        assert!(!create(&c, &r).unwrap());
        assert!(!transition(&c, "w", 0, "ready", 2).unwrap());
        assert!(transition(&c, "w", 1, "ready", 2).unwrap());
    }
}
