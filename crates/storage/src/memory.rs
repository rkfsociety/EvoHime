//! Structured agent memory items (roadmap 6.16 foundation).
//!
//! Legacy `session_memory` / `global_memory` tables remain for one-shot migration via
//! [`import_legacy_memory_notes`] (Stage 7.40–7.41). Runtime read/write uses `memory_items`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Session,
    Workspace,
    Project,
    Global,
    Experience,
}

impl MemoryScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Workspace => "workspace",
            Self::Project => "project",
            Self::Global => "global",
            Self::Experience => "experience",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "session" => Some(Self::Session),
            "workspace" => Some(Self::Workspace),
            "project" => Some(Self::Project),
            "global" | "user" => Some(Self::Global),
            "experience" => Some(Self::Experience),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Preference,
    Constraint,
    FailurePattern,
    SuccessPattern,
    VerificationRule,
    Playbook,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Preference => "preference",
            Self::Constraint => "constraint",
            Self::FailurePattern => "failure_pattern",
            Self::SuccessPattern => "success_pattern",
            Self::VerificationRule => "verification_rule",
            Self::Playbook => "playbook",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "fact" => Some(Self::Fact),
            "preference" => Some(Self::Preference),
            "constraint" => Some(Self::Constraint),
            "failure_pattern" => Some(Self::FailurePattern),
            "success_pattern" => Some(Self::SuccessPattern),
            "verification_rule" => Some(Self::VerificationRule),
            "playbook" => Some(Self::Playbook),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Candidate,
    Active,
    Conflict,
    Archived,
    Rejected,
}

impl MemoryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Active => "active",
            Self::Conflict => "conflict",
            Self::Archived => "archived",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "candidate" => Some(Self::Candidate),
            "active" => Some(Self::Active),
            "conflict" => Some(Self::Conflict),
            "archived" => Some(Self::Archived),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }

    pub fn is_retrievable(self) -> bool {
        matches!(self, Self::Candidate | Self::Active)
    }
}

/// Default scope_key for single-tenant global / experience memory.
pub const LOCAL_OPERATOR_SCOPE_KEY: &str = "local";

/// Scope keys created by integration tests — must not leak into the Memory UI.
pub fn is_synthetic_test_scope_key(scope_key: &str) -> bool {
    let key = scope_key.trim();
    key.starts_with("test-ws-") || key.starts_with("overview-") || key.starts_with("mem-svc-")
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MemoryItemRow {
    pub id: Uuid,
    pub operator_id: Uuid,
    pub scope: String,
    pub scope_key: String,
    pub kind: String,
    pub status: String,
    pub content: String,
    pub content_json: Option<Value>,
    pub confidence: f64,
    pub importance: f64,
    pub pinned: bool,
    pub source_session_id: Option<Uuid>,
    pub source_task_id: Option<Uuid>,
    pub source_label: Option<String>,
    pub supersedes: Option<Uuid>,
    pub valid_until: Option<DateTime<Utc>>,
    pub validity_hint: Option<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub use_count: i32,
    pub helpful_count: i32,
    pub harmful_count: i32,
    pub embedding: Option<Vec<f32>>,
    pub embedding_version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewMemoryItem {
    pub operator_id: Uuid,
    pub scope: MemoryScope,
    pub scope_key: String,
    pub kind: MemoryKind,
    pub status: MemoryStatus,
    pub content: String,
    pub content_json: Option<Value>,
    pub confidence: f64,
    pub importance: f64,
    pub pinned: bool,
    pub source_session_id: Option<Uuid>,
    pub source_task_id: Option<Uuid>,
    pub source_label: Option<String>,
    pub supersedes: Option<Uuid>,
    pub valid_until: Option<DateTime<Utc>>,
    pub validity_hint: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub embedding_version: i32,
}

impl NewMemoryItem {
    pub fn candidate_fact(
        scope: MemoryScope,
        scope_key: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            operator_id: crate::operators::BOOTSTRAP_OWNER_ID,
            scope,
            scope_key: scope_key.into(),
            kind: MemoryKind::Fact,
            status: MemoryStatus::Candidate,
            content: content.into(),
            content_json: None,
            confidence: 0.5,
            importance: 0.5,
            pinned: false,
            source_session_id: None,
            source_task_id: None,
            source_label: None,
            supersedes: None,
            valid_until: None,
            validity_hint: None,
            embedding: None,
            embedding_version: 0,
        }
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        if self.scope_key.trim().is_empty() {
            return Err(StorageError::InvalidMemory(
                "scope_key must not be empty".into(),
            ));
        }
        if self.content.trim().is_empty() {
            return Err(StorageError::InvalidMemory(
                "content must not be empty".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(StorageError::InvalidMemory(
                "confidence must be in 0..=1".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.importance) {
            return Err(StorageError::InvalidMemory(
                "importance must be in 0..=1".into(),
            ));
        }
        Ok(())
    }
}

const MEMORY_ITEM_COLUMNS: &str = r#"
    id, operator_id, scope, scope_key, kind, status, content, content_json,
    confidence, importance, pinned,
    source_session_id, source_task_id, source_label, supersedes,
    valid_until, validity_hint,
    last_used_at, use_count, helpful_count, harmful_count,
    embedding, embedding_version,
    created_at, updated_at
"#;

pub async fn insert_memory_item(
    pool: &PgPool,
    item: &NewMemoryItem,
) -> Result<MemoryItemRow, StorageError> {
    item.validate()?;
    let id = Uuid::new_v4();
    let row = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        INSERT INTO memory_items (
            id, operator_id, scope, scope_key, kind, status, content, content_json,
            confidence, importance, pinned,
            source_session_id, source_task_id, source_label, supersedes,
            valid_until, validity_hint,
            embedding, embedding_version
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8,
            $9, $10, $11,
            $12, $13, $14, $15,
            $16, $17,
            $18, $19
        )
        RETURNING {MEMORY_ITEM_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(item.operator_id)
    .bind(item.scope.as_str())
    .bind(item.scope_key.trim())
    .bind(item.kind.as_str())
    .bind(item.status.as_str())
    .bind(item.content.trim())
    .bind(&item.content_json)
    .bind(item.confidence)
    .bind(item.importance)
    .bind(item.pinned)
    .bind(item.source_session_id)
    .bind(item.source_task_id)
    .bind(&item.source_label)
    .bind(item.supersedes)
    .bind(item.valid_until)
    .bind(&item.validity_hint)
    .bind(&item.embedding)
    .bind(item.embedding_version)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_memory_item(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<MemoryItemRow>, StorageError> {
    let row = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        SELECT {MEMORY_ITEM_COLUMNS}
        FROM memory_items
        WHERE id = $1
        "#
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_memory_item_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    id: Uuid,
) -> Result<Option<MemoryItemRow>, StorageError> {
    Ok(sqlx::query_as::<_, MemoryItemRow>(&format!(
        "SELECT {MEMORY_ITEM_COLUMNS} FROM memory_items WHERE id = $1 AND operator_id = $2"
    ))
    .bind(id)
    .bind(operator_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_memory_items(
    pool: &PgPool,
    scope: MemoryScope,
    scope_key: &str,
    statuses: &[MemoryStatus],
    limit: i64,
) -> Result<Vec<MemoryItemRow>, StorageError> {
    let status_filters: Vec<&str> = if statuses.is_empty() {
        vec![
            MemoryStatus::Candidate.as_str(),
            MemoryStatus::Active.as_str(),
            MemoryStatus::Conflict.as_str(),
            MemoryStatus::Archived.as_str(),
            MemoryStatus::Rejected.as_str(),
        ]
    } else {
        statuses.iter().map(|s| s.as_str()).collect()
    };

    let rows = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        SELECT {MEMORY_ITEM_COLUMNS}
        FROM memory_items
        WHERE scope = $1
          AND scope_key = $2
          AND status = ANY($3)
        ORDER BY pinned DESC, importance DESC, updated_at DESC
        LIMIT $4
        "#
    ))
    .bind(scope.as_str())
    .bind(scope_key)
    .bind(&status_filters)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_memory_items_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    scope: MemoryScope,
    scope_key: &str,
    statuses: &[MemoryStatus],
    limit: i64,
) -> Result<Vec<MemoryItemRow>, StorageError> {
    let status_filters: Vec<&str> = if statuses.is_empty() {
        vec![
            MemoryStatus::Candidate.as_str(),
            MemoryStatus::Active.as_str(),
            MemoryStatus::Conflict.as_str(),
            MemoryStatus::Archived.as_str(),
            MemoryStatus::Rejected.as_str(),
        ]
    } else {
        statuses.iter().map(|s| s.as_str()).collect()
    };
    Ok(sqlx::query_as::<_, MemoryItemRow>(&format!(
        "SELECT {MEMORY_ITEM_COLUMNS} FROM memory_items WHERE operator_id = $1 AND scope = $2 AND scope_key = $3 AND status = ANY($4) ORDER BY pinned DESC, importance DESC, updated_at DESC LIMIT $5"
    ))
    .bind(operator_id)
    .bind(scope.as_str())
    .bind(scope_key)
    .bind(&status_filters)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

pub async fn list_all_memory_items(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<MemoryItemRow>, StorageError> {
    let rows = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        SELECT {MEMORY_ITEM_COLUMNS}
        FROM memory_items
        ORDER BY updated_at DESC, id DESC
        LIMIT $1
        "#
    ))
    .bind(limit.clamp(1, 50_000))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_all_memory_items_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    limit: i64,
) -> Result<Vec<MemoryItemRow>, StorageError> {
    Ok(sqlx::query_as::<_, MemoryItemRow>(&format!(
        "SELECT {MEMORY_ITEM_COLUMNS} FROM memory_items WHERE operator_id = $1 ORDER BY updated_at DESC, id DESC LIMIT $2"
    ))
    .bind(operator_id)
    .bind(limit.clamp(1, 50_000))
    .fetch_all(pool)
    .await?)
}

pub async fn update_memory_item_status(
    pool: &PgPool,
    id: Uuid,
    status: MemoryStatus,
) -> Result<Option<MemoryItemRow>, StorageError> {
    let row = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        UPDATE memory_items
        SET status = $2, updated_at = now()
        WHERE id = $1
        RETURNING {MEMORY_ITEM_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(status.as_str())
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn update_memory_item_status_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    id: Uuid,
    status: MemoryStatus,
) -> Result<Option<MemoryItemRow>, StorageError> {
    Ok(sqlx::query_as::<_, MemoryItemRow>(&format!(
        "UPDATE memory_items SET status = $3, updated_at = now() WHERE id = $1 AND operator_id = $2 RETURNING {MEMORY_ITEM_COLUMNS}"
    ))
    .bind(id)
    .bind(operator_id)
    .bind(status.as_str())
    .fetch_optional(pool)
    .await?)
}

/// Resolve two linked conflict records atomically.
pub async fn resolve_memory_conflict(
    pool: &PgPool,
    conflict_id: Uuid,
    related_id: Uuid,
    winner_id: Uuid,
) -> Result<Option<(MemoryItemRow, MemoryItemRow)>, StorageError> {
    if winner_id != conflict_id && winner_id != related_id {
        return Err(StorageError::InvalidMemory(
            "winner must be one of the conflict records".into(),
        ));
    }

    let mut transaction = pool.begin().await?;
    let ids = vec![conflict_id, related_id];
    let rows = sqlx::query_as::<_, MemoryItemRow>(&format!(
        "SELECT {MEMORY_ITEM_COLUMNS} FROM memory_items WHERE id = ANY($1) FOR UPDATE"
    ))
    .bind(&ids)
    .fetch_all(&mut *transaction)
    .await?;
    if rows.len() != 2
        || rows
            .iter()
            .any(|row| row.status != MemoryStatus::Conflict.as_str())
        || rows
            .iter()
            .map(|row| (&row.scope, &row.scope_key, &row.kind))
            .collect::<std::collections::HashSet<_>>()
            .len()
            != 1
    {
        return Ok(None);
    }

    let winner = sqlx::query_as::<_, MemoryItemRow>(&format!(
        "UPDATE memory_items SET status = $2, updated_at = now() WHERE id = $1 RETURNING {MEMORY_ITEM_COLUMNS}"
    ))
    .bind(winner_id)
    .bind(MemoryStatus::Active.as_str())
    .fetch_one(&mut *transaction)
    .await?;
    let loser_id = if winner_id == conflict_id {
        related_id
    } else {
        conflict_id
    };
    let loser = sqlx::query_as::<_, MemoryItemRow>(&format!(
        "UPDATE memory_items SET status = $2, updated_at = now() WHERE id = $1 RETURNING {MEMORY_ITEM_COLUMNS}"
    ))
    .bind(loser_id)
    .bind(MemoryStatus::Rejected.as_str())
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Some((winner, loser)))
}

pub async fn resolve_memory_conflict_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    conflict_id: Uuid,
    related_id: Uuid,
    winner_id: Uuid,
) -> Result<Option<(MemoryItemRow, MemoryItemRow)>, StorageError> {
    if winner_id != conflict_id && winner_id != related_id {
        return Err(StorageError::InvalidMemory(
            "winner must be one of the conflict records".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let ids = vec![conflict_id, related_id];
    let rows = sqlx::query_as::<_, MemoryItemRow>(&format!("SELECT {MEMORY_ITEM_COLUMNS} FROM memory_items WHERE id=ANY($1) AND operator_id=$2 FOR UPDATE")).bind(&ids).bind(operator_id).fetch_all(&mut *tx).await?;
    if rows.len() != 2
        || rows
            .iter()
            .any(|row| row.status != MemoryStatus::Conflict.as_str())
        || rows
            .iter()
            .map(|row| (&row.scope, &row.scope_key, &row.kind))
            .collect::<std::collections::HashSet<_>>()
            .len()
            != 1
    {
        return Ok(None);
    }
    let winner = sqlx::query_as::<_, MemoryItemRow>(&format!("UPDATE memory_items SET status=$2,updated_at=now() WHERE id=$1 AND operator_id=$3 RETURNING {MEMORY_ITEM_COLUMNS}")).bind(winner_id).bind(MemoryStatus::Active.as_str()).bind(operator_id).fetch_one(&mut *tx).await?;
    let loser_id = if winner_id == conflict_id {
        related_id
    } else {
        conflict_id
    };
    let loser = sqlx::query_as::<_, MemoryItemRow>(&format!("UPDATE memory_items SET status=$2,updated_at=now() WHERE id=$1 AND operator_id=$3 RETURNING {MEMORY_ITEM_COLUMNS}")).bind(loser_id).bind(MemoryStatus::Rejected.as_str()).bind(operator_id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some((winner, loser)))
}

/// List memory items across scopes for the Memory panel (6.22 / 6.24).
#[derive(Debug, Clone, Copy)]
pub struct MemoryOverviewCursor {
    pub pinned: bool,
    pub importance: f64,
    pub updated_at: DateTime<Utc>,
    pub id: Uuid,
}

pub async fn list_memory_items_overview(
    pool: &PgPool,
    scope: Option<MemoryScope>,
    scope_key: Option<&str>,
    statuses: &[MemoryStatus],
    query: Option<&str>,
    limit: i64,
) -> Result<Vec<MemoryItemRow>, StorageError> {
    list_memory_items_overview_page(pool, scope, scope_key, statuses, query, None, limit).await
}

pub async fn list_memory_items_overview_page(
    pool: &PgPool,
    scope: Option<MemoryScope>,
    scope_key: Option<&str>,
    statuses: &[MemoryStatus],
    query: Option<&str>,
    cursor: Option<MemoryOverviewCursor>,
    limit: i64,
) -> Result<Vec<MemoryItemRow>, StorageError> {
    let status_filters: Vec<&str> = if statuses.is_empty() {
        vec![
            MemoryStatus::Candidate.as_str(),
            MemoryStatus::Active.as_str(),
            MemoryStatus::Conflict.as_str(),
            MemoryStatus::Archived.as_str(),
            MemoryStatus::Rejected.as_str(),
        ]
    } else {
        statuses.iter().map(|s| s.as_str()).collect()
    };
    let scope_filter = scope.map(|s| s.as_str());
    let scope_key_filter = scope_key.map(str::trim).filter(|value| !value.is_empty());
    let query_filter = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{value}%"));

    let rows = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        SELECT {MEMORY_ITEM_COLUMNS}
        FROM memory_items
        WHERE ($1::text IS NULL OR scope = $1)
          AND ($2::text IS NULL OR scope_key = $2)
          AND status = ANY($3)
          AND ($4::text IS NULL OR content ILIKE $4)
          AND ($2::text IS NOT NULL OR scope_key !~ '^(test-ws-|overview-|mem-svc-)')
          AND ($6::boolean IS NULL OR pinned < $6
            OR (pinned = $6 AND (importance < $7
              OR (importance = $7 AND (updated_at < $8
                OR (updated_at = $8 AND id < $9))))))
        ORDER BY pinned DESC, importance DESC, updated_at DESC, id DESC
        LIMIT $5
        "#
    ))
    .bind(scope_filter)
    .bind(scope_key_filter)
    .bind(&status_filters)
    .bind(query_filter)
    .bind(limit.clamp(1, 500))
    .bind(cursor.as_ref().map(|value| value.pinned))
    .bind(cursor.as_ref().map(|value| value.importance))
    .bind(cursor.as_ref().map(|value| value.updated_at))
    .bind(cursor.as_ref().map(|value| value.id))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
pub async fn list_memory_items_overview_page_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    scope: Option<MemoryScope>,
    scope_key: Option<&str>,
    statuses: &[MemoryStatus],
    query: Option<&str>,
    cursor: Option<MemoryOverviewCursor>,
    limit: i64,
) -> Result<Vec<MemoryItemRow>, StorageError> {
    let status_filters: Vec<&str> = if statuses.is_empty() {
        vec!["candidate", "active", "conflict", "archived", "rejected"]
    } else {
        statuses.iter().map(|s| s.as_str()).collect()
    };
    let scope_filter = scope.map(|s| s.as_str());
    let scope_key_filter = scope_key.map(str::trim).filter(|v| !v.is_empty());
    let query_filter = query
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| format!("%{v}%"));
    Ok(sqlx::query_as::<_, MemoryItemRow>(&format!("SELECT {MEMORY_ITEM_COLUMNS} FROM memory_items WHERE operator_id=$1 AND ($2::text IS NULL OR scope=$2) AND ($3::text IS NULL OR scope_key=$3) AND status=ANY($4) AND ($5::text IS NULL OR content ILIKE $5) AND ($3::text IS NOT NULL OR scope_key !~ '^(test-ws-|overview-|mem-svc-)') AND ($7::boolean IS NULL OR pinned<$7 OR (pinned=$7 AND (importance<$8 OR (importance=$8 AND (updated_at<$9 OR (updated_at=$9 AND id<$10)))))) ORDER BY pinned DESC,importance DESC,updated_at DESC,id DESC LIMIT $6"))
        .bind(operator_id).bind(scope_filter).bind(scope_key_filter).bind(&status_filters).bind(query_filter).bind(limit.clamp(1,500)).bind(cursor.as_ref().map(|v| v.pinned)).bind(cursor.as_ref().map(|v| v.importance)).bind(cursor.as_ref().map(|v| v.updated_at)).bind(cursor.as_ref().map(|v| v.id)).fetch_all(pool).await?)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_memory_item_fields_with_embedding_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    id: Uuid,
    content: Option<&str>,
    status: Option<MemoryStatus>,
    pinned: Option<bool>,
    embedding: Option<&[f32]>,
    embedding_version: Option<i32>,
) -> Result<Option<MemoryItemRow>, StorageError> {
    if content.is_none()
        && status.is_none()
        && pinned.is_none()
        && embedding.is_none()
        && embedding_version.is_none()
    {
        return get_memory_item_for_operator(pool, operator_id, id).await;
    }
    if content.is_some_and(|text| text.trim().is_empty()) {
        return Err(StorageError::InvalidMemory(
            "content must not be empty".into(),
        ));
    }
    let embedding_owned = embedding.map(|v| v.to_vec());
    Ok(sqlx::query_as::<_, MemoryItemRow>(&format!("UPDATE memory_items SET content=COALESCE($3,content),status=COALESCE($4,status),pinned=COALESCE($5,pinned),embedding=COALESCE($6,embedding),embedding_version=COALESCE($7,embedding_version),updated_at=now() WHERE id=$1 AND operator_id=$2 RETURNING {MEMORY_ITEM_COLUMNS}"))
        .bind(id).bind(operator_id).bind(content.map(str::trim)).bind(status.map(|s| s.as_str())).bind(pinned).bind(&embedding_owned).bind(embedding_version).fetch_optional(pool).await?)
}

pub async fn update_memory_item_fields(
    pool: &PgPool,
    id: Uuid,
    content: Option<&str>,
    status: Option<MemoryStatus>,
    pinned: Option<bool>,
) -> Result<Option<MemoryItemRow>, StorageError> {
    update_memory_item_fields_with_embedding(pool, id, content, status, pinned, None, None).await
}

/// Update fields and optionally replace the stored embedding (when content changes).
pub async fn update_memory_item_fields_with_embedding(
    pool: &PgPool,
    id: Uuid,
    content: Option<&str>,
    status: Option<MemoryStatus>,
    pinned: Option<bool>,
    embedding: Option<&[f32]>,
    embedding_version: Option<i32>,
) -> Result<Option<MemoryItemRow>, StorageError> {
    if content.is_none()
        && status.is_none()
        && pinned.is_none()
        && embedding.is_none()
        && embedding_version.is_none()
    {
        return get_memory_item(pool, id).await;
    }
    if let Some(text) = content {
        if text.trim().is_empty() {
            return Err(StorageError::InvalidMemory(
                "content must not be empty".into(),
            ));
        }
    }

    let embedding_owned = embedding.map(|v| v.to_vec());
    let row = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        UPDATE memory_items
        SET
            content = COALESCE($2, content),
            status = COALESCE($3, status),
            pinned = COALESCE($4, pinned),
            embedding = COALESCE($5, embedding),
            embedding_version = COALESCE($6, embedding_version),
            updated_at = now()
        WHERE id = $1
        RETURNING {MEMORY_ITEM_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(content.map(str::trim))
    .bind(status.map(|s| s.as_str()))
    .bind(pinned)
    .bind(&embedding_owned)
    .bind(embedding_version)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn update_memory_item_embedding(
    pool: &PgPool,
    id: Uuid,
    embedding: &[f32],
    embedding_version: i32,
) -> Result<Option<MemoryItemRow>, StorageError> {
    let embedding = embedding.to_vec();
    let row = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        UPDATE memory_items
        SET
            embedding = $2,
            embedding_version = $3,
            updated_at = now()
        WHERE id = $1
        RETURNING {MEMORY_ITEM_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(&embedding)
    .bind(embedding_version)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn update_memory_item_embedding_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    id: Uuid,
    embedding: &[f32],
    embedding_version: i32,
) -> Result<Option<MemoryItemRow>, StorageError> {
    let embedding = embedding.to_vec();
    Ok(sqlx::query_as::<_, MemoryItemRow>(&format!(
        "UPDATE memory_items SET embedding = $3, embedding_version = $4, updated_at = now() WHERE id = $1 AND operator_id = $2 RETURNING {MEMORY_ITEM_COLUMNS}"
    ))
    .bind(id)
    .bind(operator_id)
    .bind(&embedding)
    .bind(embedding_version)
    .fetch_optional(pool)
    .await?)
}

pub async fn delete_memory_item(pool: &PgPool, id: Uuid) -> Result<bool, StorageError> {
    let result = sqlx::query("DELETE FROM memory_items WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_memory_item_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    id: Uuid,
) -> Result<bool, StorageError> {
    let result = sqlx::query("DELETE FROM memory_items WHERE id = $1 AND operator_id = $2")
        .bind(id)
        .bind(operator_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_memory_items_by_scope_key(
    pool: &PgPool,
    scope_key: &str,
) -> Result<u64, StorageError> {
    let result = sqlx::query("DELETE FROM memory_items WHERE scope_key = $1")
        .bind(scope_key.trim())
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Remove integration-test leftovers and ephemeral transcript dumps from the live DB.
pub async fn purge_memory_junk(pool: &PgPool) -> Result<u64, StorageError> {
    let items_junk = junk_note_sql("content");
    let items = sqlx::query(&format!(
        r#"
        DELETE FROM memory_items
        WHERE scope_key ~ '^(test-ws-|overview-|mem-svc-)'
           OR {items_junk}
        "#
    ))
    .execute(pool)
    .await?
    .rows_affected();

    let session_junk = junk_note_sql("note");
    let sessions = sqlx::query(&format!("DELETE FROM session_memory WHERE {session_junk}"))
        .execute(pool)
        .await?
        .rows_affected();

    let global_junk = junk_note_sql("note");
    let globals = sqlx::query(&format!("DELETE FROM global_memory WHERE {global_junk}"))
        .execute(pool)
        .await?
        .rows_affected();

    Ok(items + sessions + globals)
}

/// Apply a feedback adjustment to one memory item and append an audit event.
#[allow(clippy::too_many_arguments)]
pub async fn apply_memory_item_feedback(
    pool: &PgPool,
    id: Uuid,
    signal: &str,
    confidence: f64,
    importance: f64,
    status: Option<MemoryStatus>,
    task_id: Option<Uuid>,
    delta_confidence: f64,
    mark_used: bool,
    bump_helpful: bool,
    bump_harmful: bool,
) -> Result<Option<MemoryItemRow>, StorageError> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        UPDATE memory_items
        SET
            confidence = $2,
            importance = $3,
            status = COALESCE($4, status),
            last_used_at = CASE WHEN $5 THEN now() ELSE last_used_at END,
            use_count = use_count + CASE WHEN $5 THEN 1 ELSE 0 END,
            helpful_count = helpful_count + CASE WHEN $6 THEN 1 ELSE 0 END,
            harmful_count = harmful_count + CASE WHEN $7 THEN 1 ELSE 0 END,
            updated_at = now()
        WHERE id = $1
        RETURNING {MEMORY_ITEM_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(confidence.clamp(0.0, 1.0))
    .bind(importance.clamp(0.0, 1.0))
    .bind(status.map(|s| s.as_str()))
    .bind(mark_used)
    .bind(bump_helpful)
    .bind(bump_harmful)
    .fetch_optional(&mut *tx)
    .await?;

    if row.is_some() {
        sqlx::query(
            r#"
            INSERT INTO memory_feedback_events (id, memory_id, task_id, signal, delta_confidence)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(task_id)
        .bind(signal)
        .bind(delta_confidence)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn apply_memory_item_feedback_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    id: Uuid,
    signal: &str,
    confidence: f64,
    importance: f64,
    status: Option<MemoryStatus>,
    task_id: Option<Uuid>,
    delta_confidence: f64,
    mark_used: bool,
    bump_helpful: bool,
    bump_harmful: bool,
) -> Result<Option<MemoryItemRow>, StorageError> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query_as::<_, MemoryItemRow>(&format!("UPDATE memory_items SET confidence=$3,importance=$4,status=COALESCE($5,status),last_used_at=CASE WHEN $6 THEN now() ELSE last_used_at END,use_count=use_count+CASE WHEN $6 THEN 1 ELSE 0 END,helpful_count=helpful_count+CASE WHEN $7 THEN 1 ELSE 0 END,harmful_count=harmful_count+CASE WHEN $8 THEN 1 ELSE 0 END,updated_at=now() WHERE id=$1 AND operator_id=$2 RETURNING {MEMORY_ITEM_COLUMNS}"))
        .bind(id).bind(operator_id).bind(confidence.clamp(0.0,1.0)).bind(importance.clamp(0.0,1.0)).bind(status.map(|s|s.as_str())).bind(mark_used).bind(bump_helpful).bind(bump_harmful).fetch_optional(&mut *tx).await?;
    if row.is_some() {
        sqlx::query("INSERT INTO memory_feedback_events (id,memory_id,task_id,signal,delta_confidence) VALUES ($1,$2,$3,$4,$5)").bind(Uuid::new_v4()).bind(id).bind(task_id).bind(signal).bind(delta_confidence).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(row)
}

/// Candidates for idle decay: unused (or never used) for at least `idle_days`, not pinned.
pub async fn list_idle_memory_for_decay(
    pool: &PgPool,
    idle_days: i32,
    limit: i64,
) -> Result<Vec<MemoryItemRow>, StorageError> {
    let rows = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        SELECT {MEMORY_ITEM_COLUMNS}
        FROM memory_items
        WHERE pinned = false
          AND status IN ('active', 'candidate')
          AND confidence < 0.7
          AND (
                last_used_at IS NULL
                OR last_used_at < now() - make_interval(days => $1)
              )
        ORDER BY confidence ASC, updated_at ASC
        LIMIT $2
        "#
    ))
    .bind(idle_days.max(1))
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// SQL predicate: legacy/transcript/smoke dumps that must not become `memory_items`.
const MEMORY_JUNK_CONTENT_SQL: &str = r#"
(
     content ~* '^(user asked:|user: .*\| assistant:)'
  OR content ILIKE '%smoke test%'
  OR content ILIKE '%hello-smoke%'
  OR content ILIKE 'legacy-note-%'
  OR content ILIKE '%step-1%'
  OR content ILIKE '%step‑1%'
  OR content ILIKE '%model error%'
  OR content ILIKE '%разберись%'
  OR content ILIKE '%дорожную карту%'
  OR content = 'prefer typed API over raw fetch'
  OR content = 'prefer worktrees for parallel agents'
  OR content = 'use worktrees for parallel agents'
)
"#;

fn junk_note_sql(column: &str) -> String {
    MEMORY_JUNK_CONTENT_SQL.replace("content", column)
}

/// Import legacy free-text notes into `memory_items` as candidates.
/// Idempotent via `source_label` markers `legacy:session_memory:{id}` / `legacy:global_memory:{id}`.
/// Skips ephemeral transcript / test dumps so startup re-import cannot revive junk.
pub async fn import_legacy_memory_notes(pool: &PgPool) -> Result<u64, StorageError> {
    let session_junk = junk_note_sql("sm.note");
    let session_inserted = sqlx::query(&format!(
        r#"
        INSERT INTO memory_items (
            id, scope, scope_key, kind, status, content,
            confidence, importance, pinned,
            source_session_id, source_task_id, source_label
        )
        SELECT
            gen_random_uuid(),
            'session',
            sm.session_id::text,
            'fact',
            'candidate',
            sm.note,
            0.4,
            0.4,
            false,
            sm.session_id,
            sm.source_task_id,
            'legacy:session_memory:' || sm.id::text
        FROM session_memory sm
        WHERE NOT EXISTS (
            SELECT 1 FROM memory_items mi
            WHERE mi.source_label = 'legacy:session_memory:' || sm.id::text
        )
          AND NOT {session_junk}
        "#
    ))
    .execute(pool)
    .await?
    .rows_affected();

    let global_junk = junk_note_sql("gm.note");
    let global_inserted = sqlx::query(&format!(
        r#"
        INSERT INTO memory_items (
            id, scope, scope_key, kind, status, content,
            confidence, importance, pinned,
            source_task_id, source_label
        )
        SELECT
            gen_random_uuid(),
            'global',
            CASE
                WHEN gm.scope_key IS NULL OR btrim(gm.scope_key) = '' THEN 'local'
                ELSE gm.scope_key
            END,
            'fact',
            'candidate',
            gm.note,
            0.4,
            0.4,
            false,
            gm.source_task_id,
            'legacy:global_memory:' || gm.id::text
        FROM global_memory gm
        WHERE NOT EXISTS (
            SELECT 1 FROM memory_items mi
            WHERE mi.source_label = 'legacy:global_memory:' || gm.id::text
        )
          AND NOT {global_junk}
        "#
    ))
    .execute(pool)
    .await?
    .rows_affected();

    Ok(session_inserted + global_inserted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scopes_including_user_alias() {
        assert_eq!(MemoryScope::parse("global"), Some(MemoryScope::Global));
        assert_eq!(MemoryScope::parse("user"), Some(MemoryScope::Global));
        assert_eq!(
            MemoryScope::parse("workspace"),
            Some(MemoryScope::Workspace)
        );
        assert_eq!(MemoryScope::parse("nope"), None);
    }

    #[test]
    fn parses_kinds_and_statuses() {
        assert_eq!(
            MemoryKind::parse("failure_pattern"),
            Some(MemoryKind::FailurePattern)
        );
        assert_eq!(
            MemoryStatus::parse("candidate"),
            Some(MemoryStatus::Candidate)
        );
        assert!(MemoryStatus::Candidate.is_retrievable());
        assert!(!MemoryStatus::Conflict.is_retrievable());
        assert!(!MemoryStatus::Rejected.is_retrievable());
    }

    #[test]
    fn rejects_invalid_new_items() {
        let mut item =
            NewMemoryItem::candidate_fact(MemoryScope::Global, LOCAL_OPERATOR_SCOPE_KEY, "ok");
        assert!(item.validate().is_ok());

        item.content = "   ".into();
        assert!(item.validate().is_err());

        item.content = "ok".into();
        item.confidence = 1.5;
        assert!(item.validate().is_err());

        item.confidence = 0.5;
        item.scope_key = "".into();
        assert!(item.validate().is_err());
    }

    async fn connect_pool() -> Option<PgPool> {
        crate::connect_integration_pool().await
    }

    #[tokio::test]
    async fn inserts_lists_and_updates_memory_items() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping memory integration test: database unavailable");
            return;
        };

        let session = crate::create_session(&pool).await.expect("session");
        let scope_key = format!("test-ws-{}", Uuid::new_v4());

        let mut item = NewMemoryItem::candidate_fact(
            MemoryScope::Workspace,
            &scope_key,
            "prefer typed API over raw fetch",
        );
        item.source_session_id = Some(session.id);
        item.confidence = 0.8;
        item.importance = 0.7;

        let inserted = insert_memory_item(&pool, &item).await.expect("insert");
        assert_eq!(inserted.scope, "workspace");
        assert_eq!(inserted.status, "candidate");
        assert_eq!(inserted.source_session_id, Some(session.id));

        let listed = list_memory_items(
            &pool,
            MemoryScope::Workspace,
            &scope_key,
            &[MemoryStatus::Candidate],
            10,
        )
        .await
        .expect("list");
        assert!(listed.iter().any(|row| row.id == inserted.id));

        let promoted = update_memory_item_status(&pool, inserted.id, MemoryStatus::Active)
            .await
            .expect("promote")
            .expect("row");
        assert_eq!(promoted.status, "active");

        let loaded = get_memory_item(&pool, inserted.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(loaded.status, "active");

        let _ = delete_memory_items_by_scope_key(&pool, &scope_key)
            .await
            .expect("cleanup");
    }

    #[tokio::test]
    async fn imports_legacy_notes_idempotently() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping memory integration test: database unavailable");
            return;
        };

        let session = crate::create_session(&pool).await.expect("session");
        let note = format!("durable-pref-{}", Uuid::new_v4());
        crate::insert_session_memory(&pool, session.id, None, &note)
            .await
            .expect("legacy insert");

        let first = import_legacy_memory_notes(&pool).await.expect("import 1");
        assert!(first >= 1);
        let second = import_legacy_memory_notes(&pool).await.expect("import 2");
        assert_eq!(second, 0);

        let rows = list_memory_items(
            &pool,
            MemoryScope::Session,
            &session.id.to_string(),
            &[MemoryStatus::Candidate],
            50,
        )
        .await
        .expect("list");
        assert!(rows.iter().any(|row| row.content == note));

        let _ = delete_memory_items_by_scope_key(&pool, &session.id.to_string())
            .await
            .expect("cleanup");
        let _ = sqlx::query("DELETE FROM session_memory WHERE session_id = $1")
            .bind(session.id)
            .execute(&pool)
            .await
            .expect("session_memory cleanup");
    }

    #[tokio::test]
    async fn import_skips_ephemeral_legacy_notes() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping memory integration test: database unavailable");
            return;
        };

        let session = crate::create_session(&pool).await.expect("session");
        let junk = format!(
            "User asked: Разберись в коде; assistant replied: ok ({})",
            Uuid::new_v4()
        );
        crate::insert_session_memory(&pool, session.id, None, &junk)
            .await
            .expect("junk insert");

        let imported = import_legacy_memory_notes(&pool).await.expect("import");
        let rows = list_memory_items(
            &pool,
            MemoryScope::Session,
            &session.id.to_string(),
            &[MemoryStatus::Candidate],
            50,
        )
        .await
        .expect("list");
        assert!(
            !rows.iter().any(|row| row.content == junk),
            "ephemeral legacy note must not import; imported_count={imported}"
        );

        let _ = sqlx::query("DELETE FROM session_memory WHERE session_id = $1")
            .bind(session.id)
            .execute(&pool)
            .await
            .expect("cleanup");
    }

    #[tokio::test]
    async fn purge_memory_junk_clears_items_and_legacy() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping memory integration test: database unavailable");
            return;
        };

        let scope_key = format!("test-ws-{}", Uuid::new_v4());
        let item = NewMemoryItem::candidate_fact(
            MemoryScope::Workspace,
            &scope_key,
            "prefer typed API over raw fetch",
        );
        let _ = insert_memory_item(&pool, &item).await.expect("insert");
        let session = crate::create_session(&pool).await.expect("session");
        crate::insert_session_memory(
            &pool,
            session.id,
            None,
            "User asked: Разберись; assistant replied: nope",
        )
        .await
        .expect("legacy");

        let removed = purge_memory_junk(&pool).await.expect("purge");
        assert!(removed >= 2);

        let listed = list_memory_items_overview(
            &pool,
            Some(MemoryScope::Workspace),
            Some(&scope_key),
            &[MemoryStatus::Candidate, MemoryStatus::Active],
            None,
            10,
        )
        .await
        .expect("list");
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn overview_update_pin_and_delete() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping memory integration test: database unavailable");
            return;
        };

        let scope_key = format!("panel-{}", Uuid::new_v4());
        let item = NewMemoryItem::candidate_fact(
            MemoryScope::Workspace,
            &scope_key,
            "panel override item",
        );
        let inserted = insert_memory_item(&pool, &item).await.expect("insert");

        let listed = list_memory_items_overview(
            &pool,
            Some(MemoryScope::Workspace),
            Some(&scope_key),
            &[MemoryStatus::Candidate],
            Some("override"),
            20,
        )
        .await
        .expect("overview");
        assert!(listed.iter().any(|row| row.id == inserted.id));

        let updated = update_memory_item_fields(
            &pool,
            inserted.id,
            Some("panel override item edited"),
            Some(MemoryStatus::Active),
            Some(true),
        )
        .await
        .expect("update")
        .expect("row");
        assert_eq!(updated.content, "panel override item edited");
        assert_eq!(updated.status, "active");
        assert!(updated.pinned);

        assert!(delete_memory_item(&pool, inserted.id)
            .await
            .expect("delete"));
        assert!(get_memory_item(&pool, inserted.id)
            .await
            .expect("get")
            .is_none());
    }

    #[test]
    fn detects_synthetic_test_scope_keys() {
        assert!(is_synthetic_test_scope_key("test-ws-abc"));
        assert!(is_synthetic_test_scope_key("overview-1"));
        assert!(is_synthetic_test_scope_key("mem-svc-xyz"));
        assert!(!is_synthetic_test_scope_key("F:/github/EvoHime"));
        assert!(!is_synthetic_test_scope_key("local"));
    }

    #[tokio::test]
    async fn overview_hides_synthetic_test_scopes_by_default() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping memory integration test: database unavailable");
            return;
        };

        let scope_key = format!("test-ws-{}", Uuid::new_v4());
        let item = NewMemoryItem::candidate_fact(
            MemoryScope::Workspace,
            &scope_key,
            "prefer typed API over raw fetch",
        );
        let inserted = insert_memory_item(&pool, &item).await.expect("insert");

        let hidden = list_memory_items_overview(
            &pool,
            Some(MemoryScope::Workspace),
            None,
            &[MemoryStatus::Candidate],
            Some("prefer typed API"),
            50,
        )
        .await
        .expect("overview");
        assert!(!hidden.iter().any(|row| row.id == inserted.id));

        let explicit = list_memory_items_overview(
            &pool,
            Some(MemoryScope::Workspace),
            Some(&scope_key),
            &[MemoryStatus::Candidate],
            None,
            50,
        )
        .await
        .expect("explicit");
        assert!(explicit.iter().any(|row| row.id == inserted.id));

        let _ = delete_memory_items_by_scope_key(&pool, &scope_key)
            .await
            .expect("cleanup");
    }
}
