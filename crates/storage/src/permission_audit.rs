//! Durable permission approval audit rows (Stage 7.23).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PermissionAuditRow {
    pub id: i64,
    pub approval_id: Uuid,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub tool_name: String,
    pub permission: String,
    pub scope: String,
    pub decision: String,
    pub at_ms: i64,
    pub remembered_path: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewPermissionAudit {
    pub approval_id: Uuid,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub tool_name: String,
    pub permission: String,
    pub scope: String,
    pub decision: String,
    pub at_ms: i64,
    pub remembered_path: bool,
}

pub async fn insert_permission_audit(
    pool: &PgPool,
    entry: &NewPermissionAudit,
) -> Result<PermissionAuditRow, StorageError> {
    Ok(sqlx::query_as::<_, PermissionAuditRow>(
        r#"
        INSERT INTO permission_approval_audit (
            approval_id, task_id, session_id, tool_name, permission, scope,
            decision, at_ms, remembered_path
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, approval_id, task_id, session_id, tool_name, permission, scope,
                  decision, at_ms, remembered_path, created_at
        "#,
    )
    .bind(entry.approval_id)
    .bind(entry.task_id)
    .bind(entry.session_id)
    .bind(&entry.tool_name)
    .bind(&entry.permission)
    .bind(&entry.scope)
    .bind(&entry.decision)
    .bind(entry.at_ms)
    .bind(entry.remembered_path)
    .fetch_one(pool)
    .await?)
}

pub async fn list_permission_audit(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<PermissionAuditRow>, StorageError> {
    let limit = limit.clamp(1, 1_000);
    Ok(sqlx::query_as::<_, PermissionAuditRow>(
        r#"
        SELECT id, approval_id, task_id, session_id, tool_name, permission, scope,
               decision, at_ms, remembered_path, created_at
        FROM permission_approval_audit
        ORDER BY at_ms DESC, id DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    async fn connect_pool() -> Option<PgPool> {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://evohime:evohime@localhost:5432/evohime".into());
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()?;
        crate::run_migrations(&pool).await.ok()?;
        Some(pool)
    }

    #[tokio::test]
    async fn inserts_and_lists_permission_audit() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping permission audit integration test: database unavailable");
            return;
        };

        let approval_id = Uuid::new_v4();
        let inserted = insert_permission_audit(
            &pool,
            &NewPermissionAudit {
                approval_id,
                task_id: Uuid::new_v4(),
                session_id: Some(Uuid::new_v4()),
                tool_name: "filesystem.write".into(),
                permission: "filesystem_write".into(),
                scope: "tmp/audit.txt".into(),
                decision: "granted".into(),
                at_ms: 1_700_000_000_000,
                remembered_path: true,
            },
        )
        .await
        .expect("insert");

        assert_eq!(inserted.approval_id, approval_id);
        assert!(inserted.remembered_path);

        let listed = list_permission_audit(&pool, 50).await.expect("list");
        assert!(listed.iter().any(|row| row.id == inserted.id));
    }
}
