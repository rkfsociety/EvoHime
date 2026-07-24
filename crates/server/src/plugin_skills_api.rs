use crate::{app::AppState, auth::OperatorIdentity, ApiError};
use axum::{
    extract::{Extension, Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSkillStatus {
    pub plugin_id: String,
    pub skill_name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSkillsResponse {
    pub plugin_id: String,
    pub skills: Vec<PluginSkillStatus>,
}

/// List all skill statuses for a plugin (Stage 7.9).
pub async fn list_plugin_skills(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<OperatorIdentity>,
    Path(plugin_id): Path<String>,
) -> Result<Json<PluginSkillsResponse>, ApiError> {
    let skills = evohime_storage::list_plugin_skills(&state.pool, identity.id, &plugin_id)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to load plugin skills: {}", e)))?;

    let status_list = skills
        .into_iter()
        .map(|row| PluginSkillStatus {
            plugin_id: row.plugin_id.clone(),
            skill_name: row.skill_name,
            enabled: row.enabled,
        })
        .collect();

    Ok(Json(PluginSkillsResponse {
        plugin_id,
        skills: status_list,
    }))
}

/// Toggle skill enable/disable status (Stage 7.9).
pub async fn toggle_plugin_skill(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<OperatorIdentity>,
    Path((plugin_id, skill_name)): Path<(String, String)>,
) -> Result<Json<PluginSkillStatus>, ApiError> {
    let enabled = evohime_storage::toggle_skill_status(&state.pool, identity.id, &plugin_id, &skill_name)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to toggle skill status: {}", e)))?;

    Ok(Json(PluginSkillStatus {
        plugin_id,
        skill_name,
        enabled,
    }))
}

/// Ensure all discovered skills exist in DB (Stage 7.9).
#[allow(dead_code)]
pub async fn ensure_plugin_skills(
    state: &Arc<AppState>,
    operator_id: uuid::Uuid,
    plugin_id: &str,
    skill_names: &[&str],
) -> Result<(), ApiError> {
    evohime_storage::ensure_plugin_skills_exist(&state.pool, operator_id, plugin_id, skill_names)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to ensure plugin skills: {}", e)))
}
