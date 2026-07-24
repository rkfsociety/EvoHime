use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InstalledPluginRow {
    pub id: Uuid,
    pub operator_id: Uuid,
    pub name: String,
    pub pinned_commit: Option<String>,
    pub pinned_version: Option<String>,
    pub signature_hash: Option<String>,
    pub status: String,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub uninstalled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewInstalledPlugin {
    pub name: String,
    pub pinned_commit: Option<String>,
    pub pinned_version: Option<String>,
    pub signature_hash: Option<String>,
}

/// Record a newly installed plugin (status='active').
pub async fn insert_installed_plugin(
    pool: &PgPool,
    operator_id: Uuid,
    plugin: &NewInstalledPlugin,
) -> Result<InstalledPluginRow, StorageError> {
    let row = sqlx::query_as::<_, InstalledPluginRow>(
        "INSERT INTO installed_plugins (operator_id, name, pinned_commit, pinned_version, signature_hash, status)
         VALUES ($1, $2, $3, $4, $5, 'active')
         ON CONFLICT (operator_id, name) DO UPDATE
         SET status = EXCLUDED.status,
             pinned_commit = EXCLUDED.pinned_commit,
             pinned_version = EXCLUDED.pinned_version,
             signature_hash = EXCLUDED.signature_hash,
             updated_at = now()
         RETURNING *"
    )
    .bind(operator_id)
    .bind(&plugin.name)
    .bind(&plugin.pinned_commit)
    .bind(&plugin.pinned_version)
    .bind(&plugin.signature_hash)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Get metadata for an installed plugin.
pub async fn get_installed_plugin(
    pool: &PgPool,
    operator_id: Uuid,
    name: &str,
) -> Result<Option<InstalledPluginRow>, StorageError> {
    let row = sqlx::query_as::<_, InstalledPluginRow>(
        "SELECT * FROM installed_plugins
         WHERE operator_id = $1 AND name = $2",
    )
    .bind(operator_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// List all active plugins for an operator.
pub async fn list_installed_plugins(
    pool: &PgPool,
    operator_id: Uuid,
) -> Result<Vec<InstalledPluginRow>, StorageError> {
    let rows = sqlx::query_as::<_, InstalledPluginRow>(
        "SELECT * FROM installed_plugins
         WHERE operator_id = $1 AND status = 'active'
         ORDER BY name",
    )
    .bind(operator_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Mark a plugin as uninstalled (soft-delete: status='uninstalled', record uninstalled_at).
pub async fn mark_plugin_uninstalled(
    pool: &PgPool,
    operator_id: Uuid,
    name: &str,
) -> Result<Option<InstalledPluginRow>, StorageError> {
    let row = sqlx::query_as::<_, InstalledPluginRow>(
        "UPDATE installed_plugins
         SET status = 'uninstalled', uninstalled_at = now(), updated_at = now()
         WHERE operator_id = $1 AND name = $2 AND status = 'active'
         RETURNING *",
    )
    .bind(operator_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Update pinned version/commit for a plugin (re-activate if was uninstalled).
pub async fn update_plugin_pin(
    pool: &PgPool,
    operator_id: Uuid,
    name: &str,
    pinned_commit: Option<String>,
    pinned_version: Option<String>,
) -> Result<Option<InstalledPluginRow>, StorageError> {
    let row = sqlx::query_as::<_, InstalledPluginRow>(
        "UPDATE installed_plugins
         SET status = 'active',
             pinned_commit = $3,
             pinned_version = $4,
             uninstalled_at = NULL,
             updated_at = now()
         WHERE operator_id = $1 AND name = $2
         RETURNING *",
    )
    .bind(operator_id)
    .bind(name)
    .bind(pinned_commit)
    .bind(pinned_version)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Delete plugin metadata entirely (irreversible).
pub async fn delete_installed_plugin(
    pool: &PgPool,
    operator_id: Uuid,
    name: &str,
) -> Result<bool, StorageError> {
    let result = sqlx::query(
        "DELETE FROM installed_plugins
         WHERE operator_id = $1 AND name = $2",
    )
    .bind(operator_id)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::connect_integration_pool;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_insert_and_get_installed_plugin() {
        let pool = connect_integration_pool().await.expect("pool");
        let operator_id = Uuid::nil();
        let plugin = NewInstalledPlugin {
            name: "test-plugin".to_string(),
            pinned_commit: Some("abc123".to_string()),
            pinned_version: Some("1.0.0".to_string()),
            signature_hash: None,
        };
        let row = insert_installed_plugin(&pool, operator_id, &plugin)
            .await
            .expect("insert");
        assert_eq!(row.name, "test-plugin");
        assert_eq!(row.status, "active");
        assert_eq!(row.pinned_commit, Some("abc123".to_string()));

        let retrieved = get_installed_plugin(&pool, operator_id, "test-plugin")
            .await
            .expect("get")
            .expect("found");
        assert_eq!(retrieved.id, row.id);
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_mark_uninstalled() {
        let pool = connect_integration_pool().await.expect("pool");
        let operator_id = Uuid::nil();
        let plugin = NewInstalledPlugin {
            name: "test-uninstall".to_string(),
            pinned_commit: None,
            pinned_version: None,
            signature_hash: None,
        };
        insert_installed_plugin(&pool, operator_id, &plugin)
            .await
            .expect("insert");

        let uninstalled = mark_plugin_uninstalled(&pool, operator_id, "test-uninstall")
            .await
            .expect("mark")
            .expect("found");
        assert_eq!(uninstalled.status, "uninstalled");
        assert!(uninstalled.uninstalled_at.is_some());
    }
}
