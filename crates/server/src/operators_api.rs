#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_name_must_be_bounded_and_nonempty() {
        assert!(validate_operator_name("alice").is_ok());
        assert!(validate_operator_name(" ").is_err());
        assert!(validate_operator_name(&"x".repeat(129)).is_err());
    }

    #[test]
    fn operator_role_accepts_owner_and_member_only() {
        assert_eq!(parse_operator_role("owner"), Some(OperatorRole::Owner));
        assert_eq!(parse_operator_role("member"), Some(OperatorRole::Member));
        assert_eq!(parse_operator_role("admin"), None);
    }
}
use crate::{auth::{require_owner, OperatorIdentity}, app::AppState, ApiError};
use axum::{extract::{Extension, Path, State}, http::StatusCode, Json};
use evohime_storage::{OperatorRole, OperatorRow};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub(crate) fn validate_operator_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 128 || name.chars().any(char::is_control) {
        return Err(ApiError::BadRequest("имя оператора должно быть от 1 до 128 символов".into()));
    }
    Ok(name.to_string())
}

pub(crate) fn parse_operator_role(value: &str) -> Option<OperatorRole> {
    OperatorRole::parse(value)
}

#[derive(Debug, Serialize)]
pub(crate) struct OperatorResponse {
    pub id: Uuid,
    pub name: String,
    pub role: String,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl OperatorResponse {
    fn from_row(row: OperatorRow, token: Option<String>) -> Self {
        Self { id: row.id, name: row.name, role: row.role, active: row.active, created_at: row.created_at, updated_at: row.updated_at, last_seen_at: row.last_seen_at, token }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateOperatorRequest {
    pub name: String,
    pub role: String,
}

pub(crate) async fn list(
    Extension(identity): Extension<OperatorIdentity>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<OperatorResponse>>, ApiError> {
    require_owner(&identity).map_err(|_| ApiError::Forbidden("требуется роль owner".into()))?;
    let rows = evohime_storage::list_operators(&state.pool)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(rows.into_iter().map(|row| OperatorResponse::from_row(row, None)).collect()))
}

pub(crate) async fn create(
    Extension(identity): Extension<OperatorIdentity>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateOperatorRequest>,
) -> Result<Json<OperatorResponse>, ApiError> {
    require_owner(&identity).map_err(|_| ApiError::Forbidden("требуется роль owner".into()))?;
    let name = validate_operator_name(&request.name)?;
    let role = parse_operator_role(&request.role)
        .ok_or_else(|| ApiError::BadRequest("role должен быть owner или member".into()))?;
    let (row, token) = evohime_storage::create_operator(&state.pool, &name, role)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(OperatorResponse::from_row(row, Some(token))))
}

pub(crate) async fn rotate(
    Extension(identity): Extension<OperatorIdentity>,
    State(state): State<Arc<AppState>>,
    Path(operator_id): Path<Uuid>,
) -> Result<Json<OperatorResponse>, ApiError> {
    require_owner(&identity).map_err(|_| ApiError::Forbidden("требуется роль owner".into()))?;
    let Some((row, token)) = evohime_storage::rotate_operator_token(&state.pool, operator_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
    else {
        return Err(ApiError::BadRequest("оператор не найден или отключён".into()));
    };
    Ok(Json(OperatorResponse::from_row(row, Some(token))))
}

pub(crate) async fn revoke(
    Extension(identity): Extension<OperatorIdentity>,
    State(state): State<Arc<AppState>>,
    Path(operator_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_owner(&identity).map_err(|_| ApiError::Forbidden("требуется роль owner".into()))?;
    let revoked = evohime_storage::revoke_operator(&state.pool, operator_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    if revoked.is_none() {
        return Err(ApiError::BadRequest("оператор не найден, уже отключён или это последний owner".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
