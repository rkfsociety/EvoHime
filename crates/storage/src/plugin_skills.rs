use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PluginSkillRow {
    pub id: Uuid,
    pub operator_id: Uuid,
    pub plugin_id: String,
    pub skill_name: String,
    pub enabled: bool,
}

/// Get set of disabled skill names for a plugin (Stage 7.9).
pub async fn get_disabled_skills(
    pool: &PgPool,
    operator_id: Uuid,
    plugin_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT skill_name FROM plugin_skills WHERE operator_id = $1 AND plugin_id = $2 AND enabled = false"
    )
    .bind(operator_id)
    .bind(plugin_id)
    .fetch_all(pool)
    .await
}

/// Toggle skill enable/disable status (Stage 7.9).
pub async fn toggle_skill_status(
    pool: &PgPool,
    operator_id: Uuid,
    plugin_id: &str,
    skill_name: &str,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query_scalar::<_, bool>(
        r#"
        INSERT INTO plugin_skills (operator_id, plugin_id, skill_name, enabled)
        VALUES ($1, $2, $3, false)
        ON CONFLICT (operator_id, plugin_id, skill_name) DO UPDATE
        SET enabled = NOT plugin_skills.enabled, updated_at = now()
        RETURNING enabled
        "#,
    )
    .bind(operator_id)
    .bind(plugin_id)
    .bind(skill_name)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// List all skill statuses for a plugin (Stage 7.9).
pub async fn list_plugin_skills(
    pool: &PgPool,
    operator_id: Uuid,
    plugin_id: &str,
) -> Result<Vec<PluginSkillRow>, sqlx::Error> {
    sqlx::query_as::<_, PluginSkillRow>(
        "SELECT * FROM plugin_skills WHERE operator_id = $1 AND plugin_id = $2 ORDER BY skill_name",
    )
    .bind(operator_id)
    .bind(plugin_id)
    .fetch_all(pool)
    .await
}

/// Ensure all discovered skills exist in DB with default enabled status (Stage 7.9).
pub async fn ensure_plugin_skills_exist(
    pool: &PgPool,
    operator_id: Uuid,
    plugin_id: &str,
    skill_names: &[&str],
) -> Result<(), sqlx::Error> {
    for skill_name in skill_names {
        sqlx::query(
            r#"
            INSERT INTO plugin_skills (operator_id, plugin_id, skill_name, enabled)
            VALUES ($1, $2, $3, true)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(operator_id)
        .bind(plugin_id)
        .bind(skill_name)
        .execute(pool)
        .await?;
    }
    Ok(())
}
