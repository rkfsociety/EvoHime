//! HTTP API for Memory panel overrides (roadmap 6.22 / 6.24).

use crate::{app::AppState, ApiError};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use evohime_memory::{
    admit_memory_item, embed_text, normalize_content, redact_secrets, AdmitOutcome,
};
use evohime_storage::{
    delete_memory_item, get_memory_item, list_memory_items_overview,
    resolve_memory_conflict as resolve_storage_conflict, update_memory_item_fields_with_embedding,
    MemoryItemRow, MemoryKind, MemoryScope, MemoryStatus, NewMemoryItem,
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

#[derive(Debug, Deserialize)]
pub struct ResolveMemoryConflictRequest {
    pub winner_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct ResolveMemoryConflictResponse {
    pub winner: MemoryItemRow,
    pub loser: MemoryItemRow,
}

#[derive(Debug, Deserialize)]
pub struct CreateMemoryRequest {
    pub content: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub scope_key: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub importance: Option<f64>,
    #[serde(default)]
    pub pinned: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CreateMemoryResponse {
    pub outcome: String,
    pub item: Option<MemoryItemRow>,
    pub existing_id: Option<Uuid>,
    pub reason: Option<String>,
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
    let scope = match query
        .scope
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
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

pub async fn create_memory(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateMemoryRequest>,
) -> Result<(StatusCode, Json<CreateMemoryResponse>), ApiError> {
    let scope = body
        .scope
        .as_deref()
        .unwrap_or(MemoryScope::Global.as_str());
    let scope = MemoryScope::parse(scope)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown memory scope: {scope}")))?;
    let kind = body.kind.as_deref().unwrap_or(MemoryKind::Fact.as_str());
    let kind = MemoryKind::parse(kind)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown memory kind: {kind}")))?;
    let content = body.content.trim();
    if content.is_empty() {
        return Err(ApiError::BadRequest("content must not be empty".into()));
    }
    let item = NewMemoryItem {
        scope,
        scope_key: body
            .scope_key
            .unwrap_or_else(|| "local".to_string())
            .trim()
            .to_string(),
        kind,
        status: MemoryStatus::Candidate,
        content: content.to_string(),
        content_json: None,
        confidence: body.confidence.unwrap_or(0.5),
        importance: body.importance.unwrap_or(0.5),
        pinned: body.pinned.unwrap_or(false),
        source_session_id: None,
        source_task_id: None,
        source_label: Some("manual".into()),
        supersedes: None,
        valid_until: None,
        validity_hint: None,
        embedding: None,
        embedding_version: 0,
    };

    let response = match admit_memory_item(&state.pool, item)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
    {
        AdmitOutcome::Inserted(item) => CreateMemoryResponse {
            outcome: "inserted".into(),
            item: Some(item),
            existing_id: None,
            reason: None,
        },
        AdmitOutcome::Duplicate { existing_id } => CreateMemoryResponse {
            outcome: "duplicate".into(),
            item: None,
            existing_id: Some(existing_id),
            reason: Some("matching memory already exists".into()),
        },
        AdmitOutcome::Conflict {
            existing_id,
            item,
            reason,
        } => CreateMemoryResponse {
            outcome: "conflict".into(),
            item: Some(item),
            existing_id: Some(existing_id),
            reason: Some(reason),
        },
        AdmitOutcome::Rejected { reason } => CreateMemoryResponse {
            outcome: "rejected".into(),
            item: None,
            existing_id: None,
            reason: Some(reason),
        },
    };
    Ok((StatusCode::CREATED, Json(response)))
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
    let status = match body
        .status
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
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

pub async fn resolve_memory_conflict(
    State(state): State<Arc<AppState>>,
    Path(conflict_id): Path<Uuid>,
    Json(body): Json<ResolveMemoryConflictRequest>,
) -> Result<Json<ResolveMemoryConflictResponse>, ApiError> {
    let conflict = get_memory_item(&state.pool, conflict_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("memory conflict not found".into()))?;
    if conflict.status != MemoryStatus::Conflict.as_str() {
        return Err(ApiError::BadRequest("memory item is not a conflict".into()));
    }
    let related_id = conflict
        .supersedes
        .ok_or_else(|| ApiError::BadRequest("conflict has no related memory item".into()))?;
    let (winner, loser) =
        resolve_storage_conflict(&state.pool, conflict_id, related_id, body.winner_id)
            .await
            .map_err(|error| match error {
                evohime_storage::StorageError::InvalidMemory(message) => {
                    ApiError::BadRequest(message)
                }
                other => ApiError::Internal(other.to_string()),
            })?
            .ok_or_else(|| ApiError::BadRequest("conflict pair is stale or incompatible".into()))?;
    Ok(Json(ResolveMemoryConflictResponse { winner, loser }))
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
