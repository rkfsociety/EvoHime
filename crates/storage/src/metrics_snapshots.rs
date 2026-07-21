//! Persisted pipeline/worker metrics snapshots (Stage 7.24).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};

use crate::StorageError;

#[derive(Debug, Clone, FromRow)]
pub struct MetricsSnapshotRow {
    pub id: i64,
    pub captured_at: DateTime<Utc>,
    pub pipeline: Value,
    pub worker: Value,
}

pub async fn insert_metrics_snapshot(
    pool: &PgPool,
    pipeline: &Value,
    worker: &Value,
) -> Result<MetricsSnapshotRow, StorageError> {
    Ok(sqlx::query_as::<_, MetricsSnapshotRow>(
        r#"
        INSERT INTO metrics_snapshots (pipeline, worker)
        VALUES ($1, $2)
        RETURNING id, captured_at, pipeline, worker
        "#,
    )
    .bind(pipeline)
    .bind(worker)
    .fetch_one(pool)
    .await?)
}

pub async fn list_metrics_snapshots(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<MetricsSnapshotRow>, StorageError> {
    let limit = limit.clamp(1, 1_000);
    Ok(sqlx::query_as::<_, MetricsSnapshotRow>(
        r#"
        SELECT id, captured_at, pipeline, worker
        FROM metrics_snapshots
        ORDER BY captured_at DESC, id DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

pub async fn latest_metrics_snapshot(
    pool: &PgPool,
) -> Result<Option<MetricsSnapshotRow>, StorageError> {
    Ok(sqlx::query_as::<_, MetricsSnapshotRow>(
        r#"
        SELECT id, captured_at, pipeline, worker
        FROM metrics_snapshots
        ORDER BY captured_at DESC, id DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?)
}

/// Drop oldest rows so at most `keep` remain.
pub async fn prune_metrics_snapshots(pool: &PgPool, keep: i64) -> Result<u64, StorageError> {
    let keep = keep.max(1);
    let result = sqlx::query(
        r#"
        DELETE FROM metrics_snapshots
        WHERE id IN (
            SELECT id FROM metrics_snapshots
            ORDER BY captured_at DESC, id DESC
            OFFSET $1
        )
        "#,
    )
    .bind(keep)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn connect_pool() -> Option<PgPool> {
        crate::connect_integration_pool().await
    }

    #[tokio::test]
    async fn inserts_lists_and_prunes_metrics_snapshots() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping metrics snapshot integration test: database unavailable");
            return;
        };

        let pipeline = json!({"tasks_started": 1});
        let worker = json!({"jobs_submitted": 2});
        let inserted = insert_metrics_snapshot(&pool, &pipeline, &worker)
            .await
            .expect("insert");
        assert_eq!(inserted.pipeline["tasks_started"], 1);

        let listed = list_metrics_snapshots(&pool, 10).await.expect("list");
        assert!(listed.iter().any(|row| row.id == inserted.id));

        let _ = prune_metrics_snapshots(&pool, 10_000).await.expect("prune");
    }
}
