//! Cloud sync push run history (Stage 7.99, wave 1).

use crate::StorageError;
use chrono::{DateTime, Duration, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

pub const SYNC_STATUS_RUNNING: &str = "running";
pub const SYNC_STATUS_SUCCESS: &str = "success";
pub const SYNC_STATUS_FAILED: &str = "failed";

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct SyncRunRow {
    pub id: Uuid,
    pub operator_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: String,
    pub bytes_total: Option<i64>,
    pub checksum: Option<String>,
    pub error: Option<String>,
}

const COLUMNS: &str =
    "id, operator_id, started_at, finished_at, status, bytes_total, checksum, error";

pub fn is_terminal_sync_status(status: &str) -> bool {
    matches!(status, SYNC_STATUS_SUCCESS | SYNC_STATUS_FAILED)
}

pub async fn start_sync_run(pool: &PgPool, operator_id: Uuid) -> Result<SyncRunRow, StorageError> {
    Ok(sqlx::query_as::<_, SyncRunRow>(&format!(
        "INSERT INTO sync_runs (operator_id) VALUES ($1) RETURNING {COLUMNS}"
    ))
    .bind(operator_id)
    .fetch_one(pool)
    .await?)
}

pub async fn finish_sync_run(
    pool: &PgPool,
    run_id: Uuid,
    status: &str,
    bytes_total: Option<i64>,
    checksum: Option<&str>,
    error: Option<&str>,
) -> Result<Option<SyncRunRow>, StorageError> {
    if !is_terminal_sync_status(status) {
        return Err(StorageError::InvalidSync(format!(
            "sync run status must be terminal, got {status}"
        )));
    }
    Ok(sqlx::query_as::<_, SyncRunRow>(&format!(
        "UPDATE sync_runs
         SET finished_at = now(), status = $2, bytes_total = $3, checksum = $4, error = $5
         WHERE id = $1 AND status = 'running'
         RETURNING {COLUMNS}"
    ))
    .bind(run_id)
    .bind(status)
    .bind(bytes_total)
    .bind(checksum)
    .bind(error)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_sync_runs(
    pool: &PgPool,
    operator_id: Uuid,
    limit: i64,
) -> Result<Vec<SyncRunRow>, StorageError> {
    Ok(sqlx::query_as::<_, SyncRunRow>(&format!(
        "SELECT {COLUMNS} FROM sync_runs
         WHERE operator_id = $1
         ORDER BY started_at DESC, id DESC
         LIMIT $2"
    ))
    .bind(operator_id)
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await?)
}

/// Latest non-stale `running` run for the operator, if any.
///
/// A `running` run older than `stale_after` is treated as abandoned
/// (e.g. the server restarted mid-push) and does not block a new push.
pub async fn find_active_sync_run(
    pool: &PgPool,
    operator_id: Uuid,
    stale_after: Duration,
) -> Result<Option<SyncRunRow>, StorageError> {
    let cutoff = Utc::now() - stale_after;
    Ok(sqlx::query_as::<_, SyncRunRow>(&format!(
        "SELECT {COLUMNS} FROM sync_runs
         WHERE operator_id = $1 AND status = 'running' AND started_at > $2
         ORDER BY started_at DESC, id DESC
         LIMIT 1"
    ))
    .bind(operator_id)
    .bind(cutoff)
    .fetch_optional(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_statuses_are_success_and_failed_only() {
        assert!(is_terminal_sync_status(SYNC_STATUS_SUCCESS));
        assert!(is_terminal_sync_status(SYNC_STATUS_FAILED));
        assert!(!is_terminal_sync_status(SYNC_STATUS_RUNNING));
        assert!(!is_terminal_sync_status("done"));
    }

    #[tokio::test]
    async fn sync_run_lifecycle_and_operator_isolation() {
        let Some(pool) = crate::connect_integration_pool().await else {
            eprintln!("skipping sync run test: database unavailable");
            return;
        };
        let (first, _) = crate::create_operator(
            &pool,
            &format!("sync-a-{}", Uuid::new_v4()),
            crate::OperatorRole::Member,
        )
        .await
        .expect("first operator");
        let (second, _) = crate::create_operator(
            &pool,
            &format!("sync-b-{}", Uuid::new_v4()),
            crate::OperatorRole::Member,
        )
        .await
        .expect("second operator");

        let run = start_sync_run(&pool, first.id).await.expect("start run");
        assert_eq!(run.status, SYNC_STATUS_RUNNING);
        assert!(run.finished_at.is_none());

        let active = find_active_sync_run(&pool, first.id, Duration::minutes(10))
            .await
            .expect("find active");
        assert_eq!(active.map(|row| row.id), Some(run.id));
        assert!(find_active_sync_run(&pool, second.id, Duration::minutes(10))
            .await
            .expect("other operator active")
            .is_none());

        assert!(finish_sync_run(&pool, run.id, SYNC_STATUS_RUNNING, None, None, None)
            .await
            .is_err());
        let finished = finish_sync_run(
            &pool,
            run.id,
            SYNC_STATUS_SUCCESS,
            Some(1024),
            Some("abc123"),
            None,
        )
        .await
        .expect("finish run")
        .expect("run row");
        assert_eq!(finished.status, SYNC_STATUS_SUCCESS);
        assert_eq!(finished.bytes_total, Some(1024));
        assert!(finished.finished_at.is_some());

        // Double-finish is a no-op: the run is no longer `running`.
        assert!(finish_sync_run(&pool, run.id, SYNC_STATUS_FAILED, None, None, Some("late"))
            .await
            .expect("double finish")
            .is_none());

        let runs = list_sync_runs(&pool, first.id, 10).await.expect("list runs");
        assert!(runs.iter().any(|row| row.id == run.id));
        assert!(list_sync_runs(&pool, second.id, 10)
            .await
            .expect("other operator list")
            .iter()
            .all(|row| row.id != run.id));
    }

    #[tokio::test]
    async fn stale_running_run_does_not_block_new_push() {
        let Some(pool) = crate::connect_integration_pool().await else {
            eprintln!("skipping stale sync run test: database unavailable");
            return;
        };
        let (operator, _) = crate::create_operator(
            &pool,
            &format!("sync-stale-{}", Uuid::new_v4()),
            crate::OperatorRole::Member,
        )
        .await
        .expect("operator");
        let run = start_sync_run(&pool, operator.id).await.expect("start run");
        sqlx::query("UPDATE sync_runs SET started_at = now() - interval '1 hour' WHERE id = $1")
            .bind(run.id)
            .execute(&pool)
            .await
            .expect("age run");

        assert!(find_active_sync_run(&pool, operator.id, Duration::minutes(10))
            .await
            .expect("find active")
            .is_none());
    }
}
