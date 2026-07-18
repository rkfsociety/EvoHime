use crate::{app::AppState, ApiError};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use evohime_storage::sites::{create_site, delete_site, list_sites, update_site, SiteRow};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct SiteQuery {
    pub workspace_path: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct SiteInput {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_status")]
    pub status: String,
}
#[derive(Debug, Serialize)]
pub struct SiteResponse {
    pub id: Uuid,
    pub workspace_path: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

fn default_status() -> String {
    "draft".to_string()
}
fn scope(state: &AppState, requested: Option<&str>) -> Result<String, ApiError> {
    let default_path = state.workspace_root.to_string_lossy().to_string();
    let path = requested.unwrap_or(&default_path).trim();
    if path.is_empty() {
        return Err(ApiError::BadRequest("workspace_path обязателен".into()));
    }
    let candidate = std::path::Path::new(path)
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("workspace_path должен существовать".into()))?;
    if candidate != state.workspace_root {
        return Err(ApiError::BadRequest(
            "workspace_path вне разрешённого workspace".into(),
        ));
    }
    Ok(candidate.to_string_lossy().to_string())
}
fn validate(input: &SiteInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() || input.name.len() > 120 {
        return Err(ApiError::BadRequest(
            "name должен быть от 1 до 120 символов".into(),
        ));
    }
    if input.slug.is_empty()
        || input.slug.len() > 80
        || !input
            .slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ApiError::BadRequest(
            "slug допускает только a-z, 0-9 и дефис".into(),
        ));
    }
    if input.description.len() > 4000 {
        return Err(ApiError::BadRequest("description слишком длинный".into()));
    }
    if input.status != "draft" && input.status != "published" {
        return Err(ApiError::BadRequest(
            "status должен быть draft или published".into(),
        ));
    }
    Ok(())
}
fn response(row: SiteRow) -> SiteResponse {
    SiteResponse {
        id: row.id,
        workspace_path: row.workspace_path,
        name: row.name,
        slug: row.slug,
        description: row.description,
        status: row.status,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SiteQuery>,
) -> Result<Json<Vec<SiteResponse>>, ApiError> {
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    Ok(Json(
        list_sites(&state.pool, &workspace)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .into_iter()
            .map(response)
            .collect(),
    ))
}
pub async fn create(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SiteQuery>,
    Json(input): Json<SiteInput>,
) -> Result<(StatusCode, Json<SiteResponse>), ApiError> {
    validate(&input)?;
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    let row = create_site(
        &state.pool,
        &workspace,
        input.name.trim(),
        &input.slug,
        input.description.trim(),
    )
    .await
    .map_err(map_db)?;
    Ok((StatusCode::CREATED, Json(response(row))))
}
pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<SiteQuery>,
    Json(input): Json<SiteInput>,
) -> Result<Json<SiteResponse>, ApiError> {
    validate(&input)?;
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    let row = update_site(
        &state.pool,
        id,
        &workspace,
        input.name.trim(),
        &input.slug,
        input.description.trim(),
        &input.status,
    )
    .await
    .map_err(map_db)?
    .ok_or_else(|| ApiError::NotFound("site не найден".into()))?;
    Ok(Json(response(row)))
}
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<SiteQuery>,
) -> Result<StatusCode, ApiError> {
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    if !delete_site(&state.pool, id, &workspace)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::NotFound("site не найден".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
fn map_db(error: evohime_storage::StorageError) -> ApiError {
    if let evohime_storage::StorageError::Database(sqlx::Error::Database(db)) = &error {
        if db.constraint().is_some() {
            return ApiError::BadRequest("slug уже существует в этом workspace".into());
        }
    }
    ApiError::Internal(error.to_string())
}
