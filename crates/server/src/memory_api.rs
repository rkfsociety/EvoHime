//! HTTP API for Memory panel overrides (roadmap 6.22 / 6.24).

use crate::{app::AppState, ApiError};
use axum::{
    body::{Body, Bytes},
    extract::{Extension, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use chrono::{DateTime, Utc};
use evohime_memory::{
    admit_memory_item, embed_text, normalize_content, redact_secrets, AdmitOutcome,
};
use evohime_storage::{
    delete_memory_item_for_operator, get_memory_item_for_operator,
    list_all_memory_items_for_operator, list_memory_items_overview_page_for_operator,
    resolve_memory_conflict_for_operator as resolve_storage_conflict,
    update_memory_item_fields_with_embedding_for_operator, MemoryItemRow, MemoryKind,
    MemoryOverviewCursor, MemoryScope, MemoryStatus, NewMemoryItem,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};
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
    #[serde(default)]
    pub cursor: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPackItem {
    pub scope: String,
    pub scope_key: String,
    pub kind: String,
    pub content: String,
    #[serde(default)]
    pub content_json: Option<serde_json::Value>,
    pub confidence: f64,
    pub importance: f64,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPack {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub items: Vec<MemoryPackItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryImportResponse {
    pub inserted: usize,
    pub duplicates: usize,
    pub conflicts: usize,
    pub rejected: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct MemoryExportQuery {
    #[serde(default)]
    pub format: Option<String>,
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
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct MemoryCursor {
    pinned: bool,
    importance: f64,
    updated_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_memory_cursor(cursor: &MemoryCursor) -> String {
    format!(
        "{}|{}|{}|{}",
        if cursor.pinned { 1 } else { 0 },
        cursor.importance,
        cursor.updated_at.to_rfc3339(),
        cursor.id
    )
}

fn decode_memory_cursor(raw: &str) -> Result<MemoryCursor, ApiError> {
    let mut parts = raw.split('|');
    let pinned = match parts.next() {
        Some("0") => false,
        Some("1") => true,
        _ => return Err(ApiError::BadRequest("invalid memory cursor".into())),
    };
    let importance = parts
        .next()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .ok_or_else(|| ApiError::BadRequest("invalid memory cursor".into()))?;
    let updated_at = parts
        .next()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| ApiError::BadRequest("invalid memory cursor".into()))?;
    let id = parts
        .next()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| ApiError::BadRequest("invalid memory cursor".into()))?;
    if parts.next().is_some() {
        return Err(ApiError::BadRequest("invalid memory cursor".into()));
    }
    Ok(MemoryCursor {
        pinned,
        importance,
        updated_at,
        id,
    })
}

fn privacy_info() -> MemoryPrivacyInfo {
    MemoryPrivacyInfo {
        redaction_enabled: true,
        policy: "Secrets, tokens, passwords, cookies and private keys are redacted and never stored. Retrieved memory is untrusted data, not system instructions.".into(),
    }
}

fn memory_pack_from_rows(rows: Vec<MemoryItemRow>) -> MemoryPack {
    MemoryPack {
        format: "evohime-memory-pack".into(),
        version: 1,
        exported_at: Utc::now().to_rfc3339(),
        items: rows
            .into_iter()
            .map(|item| MemoryPackItem {
                scope: item.scope,
                scope_key: item.scope_key,
                kind: item.kind,
                content: item.content,
                content_json: item.content_json,
                confidence: item.confidence,
                importance: item.importance,
                pinned: item.pinned,
            })
            .collect(),
    }
}

fn parse_memory_pack(bytes: &[u8], is_zip: bool) -> Result<MemoryPack, ApiError> {
    let json = if is_zip {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| ApiError::BadRequest(format!("invalid memory ZIP: {error}")))?;
        let mut file = archive.by_name("memory.json").map_err(|error| {
            ApiError::BadRequest(format!("memory.json missing from ZIP: {error}"))
        })?;
        let mut json = String::new();
        file.read_to_string(&mut json)
            .map_err(|error| ApiError::BadRequest(format!("cannot read memory.json: {error}")))?;
        json
    } else {
        String::from_utf8(bytes.to_vec())
            .map_err(|error| ApiError::BadRequest(format!("memory JSON is not UTF-8: {error}")))?
    };
    let pack: MemoryPack = serde_json::from_str(&json)
        .map_err(|error| ApiError::BadRequest(format!("invalid memory pack JSON: {error}")))?;
    if pack.format != "evohime-memory-pack" || pack.version != 1 {
        return Err(ApiError::BadRequest(
            "unsupported memory pack format or version".into(),
        ));
    }
    Ok(pack)
}

pub async fn export_memory(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Query(query): Query<MemoryExportQuery>,
) -> Result<Response, ApiError> {
    let rows = list_all_memory_items_for_operator(&state.pool, identity.id, 50_000)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let pack = memory_pack_from_rows(rows);
    let format = query.format.as_deref().unwrap_or("json");
    let json =
        serde_json::to_vec_pretty(&pack).map_err(|error| ApiError::Internal(error.to_string()))?;
    if format.eq_ignore_ascii_case("zip") {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut bytes);
            archive
                .start_file("memory.json", zip::write::SimpleFileOptions::default())
                .map_err(|error| ApiError::Internal(error.to_string()))?;
            archive
                .write_all(&json)
                .map_err(|error| ApiError::Internal(error.to_string()))?;
            archive
                .finish()
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        }
        return Response::builder()
            .header(header::CONTENT_TYPE, "application/zip")
            .header(
                header::CONTENT_DISPOSITION,
                "attachment; filename=evohime-memory-pack.zip",
            )
            .body(Body::from(bytes.into_inner()))
            .map_err(|error| ApiError::Internal(error.to_string()));
    }
    if !format.eq_ignore_ascii_case("json") {
        return Err(ApiError::BadRequest("format must be json or zip".into()));
    }
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=evohime-memory-pack.json",
        )
        .body(Body::from(json))
        .map_err(|error| ApiError::Internal(error.to_string()))
}

pub async fn import_memory(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<MemoryImportResponse>, ApiError> {
    if body.len() > 10 * 1024 * 1024 {
        return Err(ApiError::BadRequest("memory pack exceeds 10 MiB".into()));
    }
    let content_type_is_zip = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("application/zip"));
    let is_zip = content_type_is_zip || body.as_ref().starts_with(b"PK\x03\x04");
    let pack = parse_memory_pack(&body, is_zip)?;
    let mut response = MemoryImportResponse {
        inserted: 0,
        duplicates: 0,
        conflicts: 0,
        rejected: 0,
        errors: Vec::new(),
    };
    for (index, item) in pack.items.into_iter().enumerate() {
        let scope = match MemoryScope::parse(&item.scope) {
            Some(scope) => scope,
            None => {
                response.errors.push(format!("item {index}: unknown scope"));
                continue;
            }
        };
        let kind = match MemoryKind::parse(&item.kind) {
            Some(kind) => kind,
            None => {
                response.errors.push(format!("item {index}: unknown kind"));
                continue;
            }
        };
        let outcome = admit_memory_item(
            &state.pool,
            NewMemoryItem {
                operator_id: identity.id,
                scope,
                scope_key: item.scope_key.trim().to_string(),
                kind,
                status: MemoryStatus::Candidate,
                content: item.content,
                content_json: item.content_json,
                confidence: item.confidence.clamp(0.0, 1.0),
                importance: item.importance.clamp(0.0, 1.0),
                pinned: item.pinned,
                source_session_id: None,
                source_task_id: None,
                source_label: Some("import".into()),
                supersedes: None,
                valid_until: None,
                validity_hint: None,
                embedding: None,
                embedding_version: 0,
            },
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
        match outcome {
            AdmitOutcome::Inserted(_) => response.inserted += 1,
            AdmitOutcome::Duplicate { .. } => response.duplicates += 1,
            AdmitOutcome::Conflict { .. } => response.conflicts += 1,
            AdmitOutcome::Rejected { reason } => {
                response.rejected += 1;
                if response.errors.len() < 20 {
                    response.errors.push(format!("item {index}: {reason}"));
                }
            }
        }
    }
    Ok(Json(response))
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
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
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

    let cursor = query
        .cursor
        .as_deref()
        .map(decode_memory_cursor)
        .transpose()?;
    let limit = query.limit.unwrap_or(50).clamp(1, 150);
    let mut items = list_memory_items_overview_page_for_operator(
        &state.pool,
        identity.id,
        scope,
        query.scope_key.as_deref(),
        &statuses,
        query.q.as_deref(),
        cursor.as_ref().map(|value| MemoryOverviewCursor {
            pinned: value.pinned,
            importance: value.importance,
            updated_at: value.updated_at,
            id: value.id,
        }),
        limit + 1,
    )
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;

    let next_cursor = if items.len() > limit as usize {
        items.truncate(limit as usize);
        items.last().map(|item| {
            encode_memory_cursor(&MemoryCursor {
                pinned: item.pinned,
                importance: item.importance,
                updated_at: item.updated_at,
                id: item.id,
            })
        })
    } else {
        None
    };

    Ok(Json(MemoryListResponse {
        items,
        privacy: privacy_info(),
        next_cursor,
    }))
}

pub async fn create_memory(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
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
        operator_id: identity.id,
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
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Path(id): Path<Uuid>,
) -> Result<Json<MemoryItemRow>, ApiError> {
    get_memory_item_for_operator(&state.pool, identity.id, id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .map(Json)
        .ok_or_else(|| ApiError::BadRequest("memory item not found".into()))
}

pub async fn update_memory(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
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
        return evohime_memory::record_memory_rejected_for_operator(
            &state.pool,
            identity.id,
            id,
            None,
        )
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

    let updated = update_memory_item_fields_with_embedding_for_operator(
        &state.pool,
        identity.id,
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
        let _ = evohime_memory::record_memory_corrected_for_operator(
            &state.pool,
            identity.id,
            id,
            None,
        )
        .await;
        if let Some(row) = get_memory_item_for_operator(&state.pool, identity.id, id)
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
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Path(conflict_id): Path<Uuid>,
    Json(body): Json<ResolveMemoryConflictRequest>,
) -> Result<Json<ResolveMemoryConflictResponse>, ApiError> {
    let conflict = get_memory_item_for_operator(&state.pool, identity.id, conflict_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("memory conflict not found".into()))?;
    if conflict.status != MemoryStatus::Conflict.as_str() {
        return Err(ApiError::BadRequest("memory item is not a conflict".into()));
    }
    let related_id = conflict
        .supersedes
        .ok_or_else(|| ApiError::BadRequest("conflict has no related memory item".into()))?;
    let (winner, loser) = resolve_storage_conflict(
        &state.pool,
        identity.id,
        conflict_id,
        related_id,
        body.winner_id,
    )
    .await
    .map_err(|error| match error {
        evohime_storage::StorageError::InvalidMemory(message) => ApiError::BadRequest(message),
        other => ApiError::Internal(other.to_string()),
    })?
    .ok_or_else(|| ApiError::BadRequest("conflict pair is stale or incompatible".into()))?;
    Ok(Json(ResolveMemoryConflictResponse { winner, loser }))
}

pub async fn delete_memory(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = delete_memory_item_for_operator(&state.pool, identity.id, id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::BadRequest("memory item not found".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_memory_cursor, encode_memory_cursor, parse_memory_pack, MemoryCursor, MemoryPack,
        MemoryPackItem,
    };
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    #[test]
    fn memory_cursor_round_trips_sort_key() {
        let cursor = MemoryCursor {
            pinned: true,
            importance: 0.75,
            updated_at: Utc.with_ymd_and_hms(2026, 7, 18, 12, 30, 0).unwrap(),
            id: Uuid::nil(),
        };

        let encoded = encode_memory_cursor(&cursor);

        assert_eq!(decode_memory_cursor(&encoded).unwrap(), cursor);
    }

    #[test]
    fn memory_cursor_rejects_malformed_value() {
        assert!(decode_memory_cursor("not-a-memory-cursor").is_err());
    }

    #[test]
    fn memory_pack_round_trips_portable_items() {
        let pack = MemoryPack {
            format: "evohime-memory-pack".into(),
            version: 1,
            exported_at: "2026-07-18T12:00:00Z".into(),
            items: vec![MemoryPackItem {
                scope: "workspace".into(),
                scope_key: "demo".into(),
                kind: "fact".into(),
                content: "native PostgreSQL".into(),
                content_json: None,
                confidence: 0.9,
                importance: 0.8,
                pinned: true,
            }],
        };
        let json = serde_json::to_vec(&pack).unwrap();
        let parsed = parse_memory_pack(&json, false).unwrap();
        assert_eq!(parsed.items[0].content, "native PostgreSQL");
        assert!(parsed.items[0].pinned);
    }

    #[test]
    fn memory_pack_rejects_unsupported_version() {
        let json =
            br#"{"format":"evohime-memory-pack","version":2,"exported_at":"now","items":[]}"#;
        assert!(parse_memory_pack(json, false).is_err());
    }
}
