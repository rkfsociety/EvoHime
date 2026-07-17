//! HTTP API for Memory panel overrides (roadmap 6.22 / 6.24).

use crate::{app::AppState, ApiError};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use evohime_memory::{embed_text, normalize_content, redact_secrets};
use evohime_storage::{
    delete_memory_item, get_memory_item, list_memory_items_overview,
    update_memory_item_fields_with_embedding, MemoryItemRow, MemoryScope, MemoryStatus,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ListMemoryQuery {
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub scope_key: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemoryRequest {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MemoryPrivacyInfo {
    pub redaction_enabled: bool,
    pub policy: String,
}

#[derive(Debug, Serialize)]
pub struct MemoryListResponse {
    pub items: Vec<MemoryItemRow>,
    pub privacy: MemoryPrivacyInfo,
}

fn privacy_info() -> MemoryPrivacyInfo {
    MemoryPrivacyInfo {
        redaction_enabled: true,
        policy: "Secrets, tokens, passwords, cookies and private keys are redacted and never stored. Retrieved memory is untrusted data, not system instructions.".into(),
    }
}

fn parse_statuses(raw: Option<&str>) -> Result<Vec<MemoryStatus>, ApiError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    if raw.eq_ignore_ascii_case("experiences") {
        return Ok(vec![
            MemoryStatus::Active,
            MemoryStatus::Candidate,
            MemoryStatus::Conflict,
        ]);
    }
    let mut out = Vec::new();
    for part in raw.split(',') {
        let status = MemoryStatus::parse(part).ok_or_else(|| {
            ApiError::BadRequest(format!("unknown memory status: {}", part.trim()))
        })?;
        out.push(status);
    }
    Ok(out)
}

pub async fn list_memory(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListMemoryQuery>,
) -> Result<Json<MemoryListResponse>, ApiError> {
    let scope = match query.scope.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        Some(raw) => Some(
            MemoryScope::parse(raw)
                .ok_or_else(|| ApiError::BadRequest(format!("unknown memory scope: {raw}")))?,
        ),
        None => None,
    };
    let mut statuses = parse_statuses(query.status.as_deref())?;
    // Experiences tab: filter by experience scope, keep status filter open unless set.
    let scope = if query
        .status
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("experiences"))
    {
        statuses = vec![
            MemoryStatus::Active,
            MemoryStatus::Candidate,
            MemoryStatus::Conflict,
        ];
        Some(MemoryScope::Experience)
    } else {
        scope
    };

    let items = list_memory_items_overview(
        &state.pool,
        scope,
        query.scope_key.as_deref(),
        &statuses,
        query.q.as_deref(),
        query.limit.unwrap_or(100),
    )
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Json(MemoryListResponse {
        items,
        privacy: privacy_info(),
    }))
}

pub async fn get_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<MemoryItemRow>, ApiError> {
    get_memory_item(&state.pool, id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .map(Json)
        .ok_or_else(|| ApiError::BadRequest("memory item not found".into()))
}

pub async fn update_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMemoryRequest>,
) -> Result<Json<MemoryItemRow>, ApiError> {
    let status = match body.status.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        Some(raw) => Some(
            MemoryStatus::parse(raw)
                .ok_or_else(|| ApiError::BadRequest(format!("unknown memory status: {raw}")))?,
        ),
        None => None,
    };

    let content = if let Some(raw) = body.content {
        let redacted = redact_secrets(&raw);
        let normalized = normalize_content(&redacted.text);
        if normalized.is_empty() || normalized == "[REDACTED]" {
            return Err(ApiError::BadRequest(
                "content empty or only secrets after redaction".into(),
            ));
        }
        Some(normalized)
    } else {
        None
    };

    if content.is_none() && status.is_none() && body.pinned.is_none() {
        return Err(ApiError::BadRequest(
            "provide content, status, and/or pinned".into(),
        ));
    }

    // Feedback-aware paths for reject / content correction.
    if status == Some(MemoryStatus::Rejected) && content.is_none() && body.pinned.is_none() {
        return evohime_memory::record_memory_rejected(&state.pool, id, None)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .map(|result| Json(result.row))
            .ok_or_else(|| ApiError::BadRequest("memory item not found".into()));
    }

    let (embedding, embedding_version) = if let Some(text) = content.as_deref() {
        let result = embed_text(text).await;
        if result.vector.is_empty() {
            (None, Some(0))
        } else {
            (Some(result.vector), Some(result.version))
        }
    } else {
        (None, None)
    };

    let updated = update_memory_item_fields_with_embedding(
        &state.pool,
        id,
        content.as_deref(),
        status,
        body.pinned,
        embedding.as_deref(),
        embedding_version,
    )
    .await
    .map_err(|error| match error {
        evohime_storage::StorageError::InvalidMemory(message) => ApiError::BadRequest(message),
        other => ApiError::Internal(other.to_string()),
    })?
    .ok_or_else(|| ApiError::BadRequest("memory item not found".into()))?;

    if content.is_some() {
        let _ = evohime_memory::record_memory_corrected(&state.pool, id, None).await;
        if let Some(row) = get_memory_item(&state.pool, id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
        {
            return Ok(Json(row));
        }
    }

    Ok(Json(updated))
}

pub async fn delete_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = delete_memory_item(&state.pool, id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::BadRequest("memory item not found".into()))
    }
}
