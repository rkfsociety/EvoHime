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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteListFilter {
    pub query: Option<String>,
    pub status: Option<String>,
}

pub fn site_search_pattern(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(format!("%{trimmed}%"))
}

pub async fn list_sites(pool: &PgPool, workspace_path: &str) -> Result<Vec<SiteRow>, StorageError> {
    list_sites_filtered(pool, workspace_path, &SiteListFilter::default()).await
}

pub async fn list_sites_filtered(
    pool: &PgPool,
    workspace_path: &str,
    filter: &SiteListFilter,
) -> Result<Vec<SiteRow>, StorageError> {
    let status = filter
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let query_pattern = filter
        .query
        .as_deref()
        .and_then(site_search_pattern);

    Ok(sqlx::query_as::<_, SiteRow>(
        r#"
        SELECT id, workspace_path, name, slug, description, status, created_at, updated_at
        FROM sites
        WHERE workspace_path = $1
          AND ($2::text IS NULL OR status = $2)
          AND (
            $3::text IS NULL
            OR name ILIKE $3
            OR slug ILIKE $3
            OR description ILIKE $3
          )
        ORDER BY updated_at DESC
        "#,
    )
    .bind(workspace_path)
    .bind(status)
    .bind(query_pattern)
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

#[cfg(test)]
mod tests {
    use super::{site_search_pattern, SiteListFilter};

    #[test]
    fn site_search_pattern_trims_and_wraps() {
        assert_eq!(site_search_pattern("  blog  "), Some("%blog%".into()));
        assert_eq!(site_search_pattern("   "), None);
        assert_eq!(site_search_pattern(""), None);
    }

    #[test]
    fn site_list_filter_defaults_to_all() {
        let filter = SiteListFilter::default();
        assert!(filter.query.is_none());
        assert!(filter.status.is_none());
    }
}
