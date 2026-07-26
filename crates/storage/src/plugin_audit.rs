//! Durable plugin marketplace audit trail (Stage 7.113, Wave 4B: Trust & Reputation).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

pub const ACTION_INSTALL: &str = "install";
pub const ACTION_UPDATE: &str = "update";
pub const ACTION_UNINSTALL: &str = "uninstall";
pub const ACTION_PIN: &str = "pin";
pub const ACTION_FORCE_OVERRIDE: &str = "force_override";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PluginAuditRow {
    pub id: i64,
    pub operator_id: Uuid,
    pub plugin_name: String,
    pub action: String,
    pub trust_level: Option<String>,
    pub risk_findings_count: i32,
    pub force_used: bool,
    pub details: Option<String>,
    pub at_ms: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewPluginAudit {
    pub operator_id: Uuid,
    pub plugin_name: String,
    pub action: String,
    pub trust_level: Option<String>,
    pub risk_findings_count: i32,
    pub force_used: bool,
    pub details: Option<String>,
    pub at_ms: i64,
}

pub async fn insert_plugin_audit(
    pool: &PgPool,
    entry: &NewPluginAudit,
) -> Result<PluginAuditRow, StorageError> {
    Ok(sqlx::query_as::<_, PluginAuditRow>(
        r#"
        INSERT INTO plugin_audit (
            operator_id, plugin_name, action, trust_level, risk_findings_count,
            force_used, details, at_ms
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, operator_id, plugin_name, action, trust_level, risk_findings_count,
                  force_used, details, at_ms, created_at
        "#,
    )
    .bind(entry.operator_id)
    .bind(&entry.plugin_name)
    .bind(&entry.action)
    .bind(&entry.trust_level)
    .bind(entry.risk_findings_count)
    .bind(entry.force_used)
    .bind(&entry.details)
    .bind(entry.at_ms)
    .fetch_one(pool)
    .await?)
}

/// List recent plugin audit entries, newest first. `plugin_name` optionally
/// narrows to a single plugin's history.
pub async fn list_plugin_audit(
    pool: &PgPool,
    plugin_name: Option<&str>,
    limit: i64,
) -> Result<Vec<PluginAuditRow>, StorageError> {
    let limit = limit.clamp(1, 1_000);
    let rows = match plugin_name {
        Some(name) => {
            sqlx::query_as::<_, PluginAuditRow>(
                r#"
                SELECT id, operator_id, plugin_name, action, trust_level, risk_findings_count,
                       force_used, details, at_ms, created_at
                FROM plugin_audit
                WHERE plugin_name = $1
                ORDER BY at_ms DESC, id DESC
                LIMIT $2
                "#,
            )
            .bind(name)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, PluginAuditRow>(
                r#"
                SELECT id, operator_id, plugin_name, action, trust_level, risk_findings_count,
                       force_used, details, at_ms, created_at
                FROM plugin_audit
                ORDER BY at_ms DESC, id DESC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn connect_pool() -> Option<PgPool> {
        crate::connect_integration_pool().await
    }

    #[tokio::test]
    async fn inserts_and_lists_plugin_audit() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping plugin audit integration test: database unavailable");
            return;
        };

        let operator_id = crate::operators::BOOTSTRAP_OWNER_ID;
        let plugin_name = format!("test-plugin-{}", Uuid::new_v4());

        let inserted = insert_plugin_audit(
            &pool,
            &NewPluginAudit {
                operator_id,
                plugin_name: plugin_name.clone(),
                action: ACTION_INSTALL.to_string(),
                trust_level: Some("community".to_string()),
                risk_findings_count: 0,
                force_used: false,
                details: Some("installed from catalog".to_string()),
                at_ms: 1_700_000_000_000,
            },
        )
        .await
        .expect("insert");

        assert_eq!(inserted.plugin_name, plugin_name);
        assert_eq!(inserted.action, ACTION_INSTALL);
        assert!(!inserted.force_used);

        let listed = list_plugin_audit(&pool, Some(&plugin_name), 50)
            .await
            .expect("list");
        assert!(listed.iter().any(|row| row.id == inserted.id));
    }

    #[tokio::test]
    async fn force_override_recorded_distinctly() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping plugin audit integration test: database unavailable");
            return;
        };

        let operator_id = crate::operators::BOOTSTRAP_OWNER_ID;
        let plugin_name = format!("risky-plugin-{}", Uuid::new_v4());

        let inserted = insert_plugin_audit(
            &pool,
            &NewPluginAudit {
                operator_id,
                plugin_name: plugin_name.clone(),
                action: ACTION_FORCE_OVERRIDE.to_string(),
                trust_level: Some("unverified".to_string()),
                risk_findings_count: 2,
                force_used: true,
                details: Some("curl-pipe-shell; base64-shell".to_string()),
                at_ms: 1_700_000_001_000,
            },
        )
        .await
        .expect("insert");

        assert!(inserted.force_used);
        assert_eq!(inserted.risk_findings_count, 2);
    }
}
