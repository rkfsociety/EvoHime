use crate::StorageError;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct SiteRow {
    pub id: Uuid,
    pub workspace_path: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn list_sites(pool: &PgPool, workspace_path: &str) -> Result<Vec<SiteRow>, StorageError> {
    Ok(sqlx::query_as::<_, SiteRow>(
        "SELECT id, workspace_path, name, slug, description, status, created_at, updated_at FROM sites WHERE workspace_path = $1 ORDER BY updated_at DESC",
    )
    .bind(workspace_path)
    .fetch_all(pool)
    .await?)
}

pub async fn create_site(
    pool: &PgPool,
    workspace_path: &str,
    name: &str,
    slug: &str,
    description: &str,
) -> Result<SiteRow, StorageError> {
    Ok(sqlx::query_as::<_, SiteRow>(
        "INSERT INTO sites (id, workspace_path, name, slug, description) VALUES ($1, $2, $3, $4, $5) RETURNING id, workspace_path, name, slug, description, status, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(workspace_path)
    .bind(name)
    .bind(slug)
    .bind(description)
    .fetch_one(pool)
    .await?)
}

pub async fn update_site(
    pool: &PgPool,
    id: Uuid,
    workspace_path: &str,
    name: &str,
    slug: &str,
    description: &str,
    status: &str,
) -> Result<Option<SiteRow>, StorageError> {
    Ok(sqlx::query_as::<_, SiteRow>(
        "UPDATE sites SET name = $3, slug = $4, description = $5, status = $6, updated_at = now() WHERE id = $1 AND workspace_path = $2 RETURNING id, workspace_path, name, slug, description, status, created_at, updated_at",
    )
    .bind(id)
    .bind(workspace_path)
    .bind(name)
    .bind(slug)
    .bind(description)
    .bind(status)
    .fetch_optional(pool)
    .await?)
}

pub async fn publish_site(
    pool: &PgPool,
    id: Uuid,
    workspace_path: &str,
) -> Result<Option<SiteRow>, StorageError> {
    Ok(sqlx::query_as::<_, SiteRow>(
        "UPDATE sites SET status = 'published', updated_at = now() WHERE id = $1 AND workspace_path = $2 RETURNING id, workspace_path, name, slug, description, status, created_at, updated_at",
    )
    .bind(id)
    .bind(workspace_path)
    .fetch_optional(pool)
    .await?)
}

pub async fn delete_site(
    pool: &PgPool,
    id: Uuid,
    workspace_path: &str,
) -> Result<bool, StorageError> {
    Ok(
        sqlx::query("DELETE FROM sites WHERE id = $1 AND workspace_path = $2")
            .bind(id)
            .bind(workspace_path)
            .execute(pool)
            .await?
            .rows_affected()
            == 1,
    )
}
