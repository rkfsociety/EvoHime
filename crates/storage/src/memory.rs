//! Structured agent memory items (roadmap 6.16 foundation).
//!
//! Legacy `session_memory` / `global_memory` tables remain for compatibility;
//! new code should use `memory_items` via this module.

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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MemoryItemRow {
    pub id: Uuid,
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewMemoryItem {
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
}

impl NewMemoryItem {
    pub fn candidate_fact(scope: MemoryScope, scope_key: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
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
        }
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        if self.scope_key.trim().is_empty() {
            return Err(StorageError::InvalidMemory("scope_key must not be empty".into()));
        }
        if self.content.trim().is_empty() {
            return Err(StorageError::InvalidMemory("content must not be empty".into()));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(StorageError::InvalidMemory("confidence must be in 0..=1".into()));
        }
        if !(0.0..=1.0).contains(&self.importance) {
            return Err(StorageError::InvalidMemory("importance must be in 0..=1".into()));
        }
        Ok(())
    }
}

const MEMORY_ITEM_COLUMNS: &str = r#"
    id, scope, scope_key, kind, status, content, content_json,
    confidence, importance, pinned,
    source_session_id, source_task_id, source_label, supersedes,
    valid_until, validity_hint, created_at, updated_at
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
            id, scope, scope_key, kind, status, content, content_json,
            confidence, importance, pinned,
            source_session_id, source_task_id, source_label, supersedes,
            valid_until, validity_hint
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7,
            $8, $9, $10,
            $11, $12, $13, $14,
            $15, $16
        )
        RETURNING {MEMORY_ITEM_COLUMNS}
        "#
    ))
    .bind(id)
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
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_memory_item(pool: &PgPool, id: Uuid) -> Result<Option<MemoryItemRow>, StorageError> {
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

/// List memory items across scopes for the Memory panel (6.22 / 6.24).
pub async fn list_memory_items_overview(
    pool: &PgPool,
    scope: Option<MemoryScope>,
    scope_key: Option<&str>,
    statuses: &[MemoryStatus],
    query: Option<&str>,
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
    let scope_key_filter = scope_key
        .map(str::trim)
        .filter(|value| !value.is_empty());
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
        ORDER BY pinned DESC, importance DESC, updated_at DESC
        LIMIT $5
        "#
    ))
    .bind(scope_filter)
    .bind(scope_key_filter)
    .bind(&status_filters)
    .bind(query_filter)
    .bind(limit.clamp(1, 500))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_memory_item_fields(
    pool: &PgPool,
    id: Uuid,
    content: Option<&str>,
    status: Option<MemoryStatus>,
    pinned: Option<bool>,
) -> Result<Option<MemoryItemRow>, StorageError> {
    if content.is_none() && status.is_none() && pinned.is_none() {
        return get_memory_item(pool, id).await;
    }
    if let Some(text) = content {
        if text.trim().is_empty() {
            return Err(StorageError::InvalidMemory("content must not be empty".into()));
        }
    }

    let row = sqlx::query_as::<_, MemoryItemRow>(&format!(
        r#"
        UPDATE memory_items
        SET
            content = COALESCE($2, content),
            status = COALESCE($3, status),
            pinned = COALESCE($4, pinned),
            updated_at = now()
        WHERE id = $1
        RETURNING {MEMORY_ITEM_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(content.map(str::trim))
    .bind(status.map(|s| s.as_str()))
    .bind(pinned)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn delete_memory_item(pool: &PgPool, id: Uuid) -> Result<bool, StorageError> {
    let result = sqlx::query("DELETE FROM memory_items WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Import legacy free-text notes into `memory_items` as candidates.
/// Idempotent via `source_label` markers `legacy:session_memory:{id}` / `legacy:global_memory:{id}`.
pub async fn import_legacy_memory_notes(pool: &PgPool) -> Result<u64, StorageError> {
    let session_inserted = sqlx::query(
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
        "#,
    )
    .execute(pool)
    .await?
    .rows_affected();

    let global_inserted = sqlx::query(
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
        "#,
    )
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
        assert_eq!(MemoryScope::parse("workspace"), Some(MemoryScope::Workspace));
        assert_eq!(MemoryScope::parse("nope"), None);
    }

    #[test]
    fn parses_kinds_and_statuses() {
        assert_eq!(MemoryKind::parse("failure_pattern"), Some(MemoryKind::FailurePattern));
        assert_eq!(MemoryStatus::parse("candidate"), Some(MemoryStatus::Candidate));
        assert!(MemoryStatus::Candidate.is_retrievable());
        assert!(!MemoryStatus::Conflict.is_retrievable());
        assert!(!MemoryStatus::Rejected.is_retrievable());
    }

    #[test]
    fn rejects_invalid_new_items() {
        let mut item = NewMemoryItem::candidate_fact(MemoryScope::Global, LOCAL_OPERATOR_SCOPE_KEY, "ok");
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
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://evohime:evohime@localhost:5432/evohime".into());
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()?;
        crate::run_migrations(&pool).await.ok()?;
        Some(pool)
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
    }

    #[tokio::test]
    async fn imports_legacy_notes_idempotently() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping memory integration test: database unavailable");
            return;
        };

        let session = crate::create_session(&pool).await.expect("session");
        let note = format!("legacy-note-{}", Uuid::new_v4());
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
    }

    #[tokio::test]
    async fn overview_update_pin_and_delete() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping memory integration test: database unavailable");
            return;
        };

        let scope_key = format!("overview-{}", Uuid::new_v4());
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

        assert!(delete_memory_item(&pool, inserted.id).await.expect("delete"));
        assert!(get_memory_item(&pool, inserted.id)
            .await
            .expect("get")
            .is_none());
    }
}
