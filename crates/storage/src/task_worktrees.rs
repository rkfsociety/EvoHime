//! Isolated git worktrees allocated to concurrently-running tasks (Stage 7.107).

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

#[derive(Debug, Clone, FromRow)]
pub struct TaskWorktreeRow {
    pub task_id: Uuid,
    pub base_commit_sha: String,
    pub worktree_path: String,
    pub primary_workspace_root: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewTaskWorktree {
    pub task_id: Uuid,
    pub base_commit_sha: String,
    pub worktree_path: String,
    pub primary_workspace_root: String,
}

pub async fn insert_task_worktree(
    pool: &PgPool,
    entry: &NewTaskWorktree,
) -> Result<TaskWorktreeRow, StorageError> {
    Ok(sqlx::query_as::<_, TaskWorktreeRow>(
        r#"
        INSERT INTO task_worktrees (task_id, base_commit_sha, worktree_path, primary_workspace_root)
        VALUES ($1, $2, $3, $4)
        RETURNING task_id, base_commit_sha, worktree_path, primary_workspace_root, created_at
        "#,
    )
    .bind(entry.task_id)
    .bind(&entry.base_commit_sha)
    .bind(&entry.worktree_path)
    .bind(&entry.primary_workspace_root)
    .fetch_one(pool)
    .await?)
}

pub async fn get_task_worktree(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Option<TaskWorktreeRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskWorktreeRow>(
        r#"
        SELECT task_id, base_commit_sha, worktree_path, primary_workspace_root, created_at
        FROM task_worktrees
        WHERE task_id = $1
        "#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn delete_task_worktree(pool: &PgPool, task_id: Uuid) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM task_worktrees WHERE task_id = $1")
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_task_worktrees(pool: &PgPool) -> Result<Vec<TaskWorktreeRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskWorktreeRow>(
        r#"
        SELECT task_id, base_commit_sha, worktree_path, primary_workspace_root, created_at
        FROM task_worktrees
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?)
}

#[derive(Debug, Clone, FromRow)]
struct TaskWorktreeWithStatusRow {
    task_id: Uuid,
    base_commit_sha: String,
    worktree_path: String,
    primary_workspace_root: String,
    created_at: DateTime<Utc>,
    task_status: String,
}

#[derive(Debug, Clone)]
pub struct TaskWorktreeWithStatus {
    pub row: TaskWorktreeRow,
    pub task_status: String,
}

/// Every `task_worktrees` row alongside its owning task's *current* status.
/// Startup cleanup uses `task_status` (not the transient, restart-scoped set
/// `recover_after_restart` returns) to decide whether a row is still needed —
/// see the design doc's Cleanup section for why that distinction matters
/// (an approval-paused task's worktree must never be swept just because it
/// wasn't mid-crash at the moment of *this* restart).
pub async fn list_task_worktrees_with_status(
    pool: &PgPool,
) -> Result<Vec<TaskWorktreeWithStatus>, StorageError> {
    let rows = sqlx::query_as::<_, TaskWorktreeWithStatusRow>(
        r#"
        SELECT tw.task_id, tw.base_commit_sha, tw.worktree_path, tw.primary_workspace_root,
               tw.created_at, t.status AS task_status
        FROM task_worktrees tw
        JOIN tasks t ON t.id = tw.task_id
        ORDER BY tw.created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TaskWorktreeWithStatus {
            row: TaskWorktreeRow {
                task_id: row.task_id,
                base_commit_sha: row.base_commit_sha,
                worktree_path: row.worktree_path,
                primary_workspace_root: row.primary_workspace_root,
                created_at: row.created_at,
            },
            task_status: row.task_status,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn connect_pool() -> Option<PgPool> {
        crate::connect_integration_pool().await
    }

    async fn seed_task(pool: &PgPool) -> Uuid {
        let session_id: Uuid =
            sqlx::query_scalar("INSERT INTO sessions DEFAULT VALUES RETURNING id")
                .fetch_one(pool)
                .await
                .expect("insert session");
        sqlx::query_scalar(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session_id)
        .fetch_one(pool)
        .await
        .expect("insert task")
    }

    #[tokio::test]
    async fn inserts_gets_lists_and_deletes_a_row() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping task_worktrees integration test: database unavailable");
            return;
        };

        let task_id = seed_task(&pool).await;
        let inserted = insert_task_worktree(
            &pool,
            &NewTaskWorktree {
                task_id,
                base_commit_sha: "deadbeef".to_string(),
                worktree_path: "/tmp/evohime-worktrees/example".to_string(),
                primary_workspace_root: "/tmp/example-repo".to_string(),
            },
        )
        .await
        .expect("insert");
        assert_eq!(inserted.task_id, task_id);

        let fetched = get_task_worktree(&pool, task_id)
            .await
            .expect("get")
            .expect("row present");
        assert_eq!(fetched.base_commit_sha, "deadbeef");

        let listed = list_task_worktrees(&pool).await.expect("list");
        assert!(listed.iter().any(|row| row.task_id == task_id));

        delete_task_worktree(&pool, task_id).await.expect("delete");
        assert!(get_task_worktree(&pool, task_id)
            .await
            .expect("get after delete")
            .is_none());
    }

    #[tokio::test]
    async fn list_with_status_reports_the_owning_tasks_current_status() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping task_worktrees integration test: database unavailable");
            return;
        };

        let task_id = seed_task(&pool).await; // seed_task inserts with status 'running'
        insert_task_worktree(
            &pool,
            &NewTaskWorktree {
                task_id,
                base_commit_sha: "deadbeef".to_string(),
                worktree_path: "/tmp/evohime-worktrees/example".to_string(),
                primary_workspace_root: "/tmp/example-repo".to_string(),
            },
        )
        .await
        .expect("insert");

        let with_status = list_task_worktrees_with_status(&pool)
            .await
            .expect("list with status");
        let found = with_status
            .iter()
            .find(|entry| entry.row.task_id == task_id)
            .expect("row present");
        assert_eq!(found.task_status, "running");

        sqlx::query("UPDATE tasks SET status = 'paused' WHERE id = $1")
            .bind(task_id)
            .execute(&pool)
            .await
            .expect("update status");

        let with_status = list_task_worktrees_with_status(&pool)
            .await
            .expect("list with status after update");
        let found = with_status
            .iter()
            .find(|entry| entry.row.task_id == task_id)
            .expect("row present after update");
        assert_eq!(found.task_status, "paused");

        delete_task_worktree(&pool, task_id).await.expect("cleanup");
    }
}
