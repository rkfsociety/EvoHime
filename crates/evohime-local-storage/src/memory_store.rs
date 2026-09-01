//! Migration-neutral bounded persistence contract for Memory v1.
//!
//! The module intentionally does not register itself in `lib.rs` or create a
//! migration. The storage owner decides when the compatible table exists;
//! this file keeps the record bounds and parameterized SQL contract together.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const MAX_ID_BYTES: usize = 256;
pub const MAX_SCOPE_ID_BYTES: usize = 512;
pub const MAX_TITLE_BYTES: usize = 512;
pub const MAX_CONTENT_BYTES: usize = 32 * 1024;
pub const MAX_PROVENANCE_BYTES: usize = 2 * 1024;
pub const MAX_TIMESTAMP_BYTES: usize = 64;
pub const MAX_QUERY_BYTES: usize = 512;
pub const MAX_TTL_SECONDS: u64 = 366 * 24 * 60 * 60;
pub const MAX_EVIDENCE_REFS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Project,
    Task,
    Workspace,
    /// Session-scoped запись: живёт до конца сессии и ещё сутки, не участвует
    /// в long-term retrieval.
    Session,
}

impl MemoryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Task => "task",
            Self::Workspace => "workspace",
            Self::Session => "session",
        }
    }

    pub fn parse(value: &str) -> Result<Self, MemoryStoreError> {
        match value {
            "project" => Ok(Self::Project),
            "task" => Ok(Self::Task),
            "workspace" => Ok(Self::Workspace),
            "session" => Ok(Self::Session),
            _ => Err(MemoryStoreError::InvalidScope),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPrivacy {
    Public,
    Internal,
    Private,
}

impl MemoryPrivacy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Private => "private",
        }
    }

    fn parse(value: &str) -> Result<Self, MemoryStoreError> {
        match value {
            "public" => Ok(Self::Public),
            "internal" => Ok(Self::Internal),
            "private" => Ok(Self::Private),
            _ => Err(MemoryStoreError::InvalidPrivacy),
        }
    }
}

/// Поля контракта Memory Extraction. Отделены от Memory v1 полей, чтобы было
/// видно, что именно добавляет extraction и какие legacy-значения получает
/// мигрированная запись.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryExtractionFields {
    #[serde(default = "default_record_version")]
    pub record_version: u32,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub execution_event_refs: Vec<i64>,
    pub kind: String,
    /// `None` у legacy rows: точный нормализатор версионируется в Core и
    /// применяется к `title` при чтении.
    pub canonical_subject: Option<String>,
    pub confirmation_state: String,
    pub model_confidence: f64,
    pub verification_confidence: f64,
    pub privacy_class: String,
    pub source_trust: String,
    pub supersedes: Option<String>,
    pub superseded_by: Option<String>,
    pub supersession_reason: Option<String>,
    pub extractor_version: String,
    pub policy_version: String,
    pub validation_status: String,
    pub validated_at: Option<String>,
    pub provenance_source_id: Option<String>,
    /// Core-owned governance classification. Legacy rows default to the
    /// conservative user-confirmed durable profile.
    #[serde(default = "default_authority")]
    pub authority: String,
    #[serde(default = "default_durability")]
    pub durability: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

impl Default for MemoryExtractionFields {
    /// Значения, эквивалентные мигрированной Memory v1 записи: подтверждённый
    /// пользовательский факт, никогда не проходивший через model extraction.
    fn default() -> Self {
        Self {
            record_version: 1,
            evidence_refs: Vec::new(),
            execution_event_refs: Vec::new(),
            kind: "entity".to_owned(),
            canonical_subject: None,
            confirmation_state: "confirmed".to_owned(),
            model_confidence: 1.0,
            verification_confidence: 1.0,
            privacy_class: "normal".to_owned(),
            source_trust: "user".to_owned(),
            supersedes: None,
            superseded_by: None,
            supersession_reason: None,
            extractor_version: "v1_legacy".to_owned(),
            policy_version: "legacy-v1".to_owned(),
            validation_status: "not_required".to_owned(),
            validated_at: None,
            provenance_source_id: None,
            authority: default_authority(),
            durability: default_durability(),
            confidence: default_confidence(),
        }
    }
}

fn default_authority() -> String {
    "user_asserted".to_owned()
}
fn default_durability() -> String {
    "durable".to_owned()
}
fn default_confidence() -> f64 {
    1.0
}

fn default_record_version() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub scope: MemoryScope,
    pub scope_id: String,
    pub title: String,
    pub content: String,
    pub provenance: String,
    pub privacy: MemoryPrivacy,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub archived: bool,
    pub forgotten: bool,
    pub confirmations: i64,
    pub lesson_key: Option<String>,
    #[serde(flatten)]
    pub extraction: MemoryExtractionFields,
}

impl MemoryRecord {
    // Аргументы повторяют колонки записи памяти в SQLite.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        scope: MemoryScope,
        scope_id: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
        provenance: impl Into<String>,
        privacy: MemoryPrivacy,
        created_at: impl Into<String>,
        expires_at: Option<String>,
    ) -> Result<Self, MemoryStoreError> {
        let record = Self {
            id: id.into(),
            scope,
            scope_id: scope_id.into(),
            title: title.into(),
            content: redact_sensitive(&content.into()),
            provenance: provenance.into(),
            privacy,
            created_at: created_at.into(),
            expires_at,
            archived: false,
            forgotten: false,
            confirmations: 1,
            lesson_key: None,
            extraction: MemoryExtractionFields::default(),
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), MemoryStoreError> {
        validate_required("id", &self.id, MAX_ID_BYTES)?;
        validate_required("scope_id", &self.scope_id, MAX_SCOPE_ID_BYTES)?;
        validate_required("title", &self.title, MAX_TITLE_BYTES)?;
        validate_required("content", &self.content, MAX_CONTENT_BYTES)?;
        validate_required("provenance", &self.provenance, MAX_PROVENANCE_BYTES)?;
        validate_required("created_at", &self.created_at, MAX_TIMESTAMP_BYTES)?;
        if let Some(expires_at) = &self.expires_at {
            validate_required("expires_at", expires_at, MAX_TIMESTAMP_BYTES)?;
        }
        validate_required("kind", &self.extraction.kind, MAX_ID_BYTES)?;
        validate_required(
            "confirmation_state",
            &self.extraction.confirmation_state,
            MAX_ID_BYTES,
        )?;
        validate_required(
            "privacy_class",
            &self.extraction.privacy_class,
            MAX_ID_BYTES,
        )?;
        validate_required("source_trust", &self.extraction.source_trust, MAX_ID_BYTES)?;
        validate_required(
            "validation_status",
            &self.extraction.validation_status,
            MAX_ID_BYTES,
        )?;
        if !matches!(
            self.extraction.authority.as_str(),
            "user_asserted" | "system_defined" | "model_proposed" | "imported"
        ) {
            return Err(MemoryStoreError::InvalidField("authority"));
        }
        if !matches!(
            self.extraction.durability.as_str(),
            "ephemeral" | "session" | "durable"
        ) {
            return Err(MemoryStoreError::InvalidField("durability"));
        }
        if !self.extraction.confidence.is_finite()
            || !(0.0..=1.0).contains(&self.extraction.confidence)
        {
            return Err(MemoryStoreError::InvalidField("confidence"));
        }
        if self.extraction.record_version == 0
            || self.extraction.evidence_refs.len() > MAX_EVIDENCE_REFS
            || self.extraction.execution_event_refs.len() > MAX_EVIDENCE_REFS
            || self
                .extraction
                .evidence_refs
                .iter()
                .any(|value| value.trim().is_empty())
            || self
                .extraction
                .execution_event_refs
                .iter()
                .any(|value| *value < 0)
        {
            return Err(MemoryStoreError::InvalidEvidenceRefs);
        }
        // `secret` не имеет представления в persistent store: такие записи
        // отвергаются до persistence, а не маскируются после.
        if self.extraction.privacy_class == "secret" {
            return Err(MemoryStoreError::SecretNotStorable);
        }
        if let Some(subject) = &self.extraction.canonical_subject {
            validate_required("canonical_subject", subject, MAX_SCOPE_ID_BYTES)?;
        }
        for confidence in [
            self.extraction.model_confidence,
            self.extraction.verification_confidence,
        ] {
            if !(0.0..=1.0).contains(&confidence) {
                return Err(MemoryStoreError::InvalidConfidence);
            }
        }
        Ok(())
    }

    /// Ключ конфликта в терминах хранилища: `kind + canonical_subject + scope`.
    /// У legacy rows canonical subject берётся из заголовка — точную
    /// нормализацию выполняет Core.
    pub fn subject_for_conflict(&self) -> &str {
        self.extraction
            .canonical_subject
            .as_deref()
            .unwrap_or(&self.title)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryStoreError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: usize },
    #[error("invalid memory scope")]
    InvalidScope,
    #[error("invalid privacy label")]
    InvalidPrivacy,
    #[error("invalid TTL")]
    InvalidTtl,
    #[error("invalid memory governance field: {0}")]
    InvalidField(&'static str),
    #[error("secret memory is never persisted")]
    SecretNotStorable,
    #[error("confidence must be within 0.0..=1.0")]
    InvalidConfidence,
    #[error("memory evidence references are invalid or unbounded")]
    InvalidEvidenceRefs,
    #[error("memory record was not found")]
    NotFound,
    #[error("state transition from {from} to {to} is not allowed")]
    InvalidTransition { from: String, to: String },
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

fn validate_required(field: &'static str, value: &str, max: usize) -> Result<(), MemoryStoreError> {
    if value.trim().is_empty() {
        return Err(MemoryStoreError::Empty { field });
    }
    if value.len() > max {
        return Err(MemoryStoreError::Limit { field, max });
    }
    Ok(())
}

fn redact_sensitive(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if token.contains('@')
                || lower.starts_with("bearer")
                || lower.starts_with("sk-")
                || lower.starts_with("ghp_")
                || lower.starts_with("github_pat_")
                || lower.starts_with("api_key=")
                || lower.starts_with("token=")
            {
                "[REDACTED]".to_owned()
            } else {
                token.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parameterized SQL only; schema creation and migrations remain external.
pub struct MemoryStoreSql;

/// Installs the v31 typed-memory columns on every database open. It is
/// intentionally idempotent and independent of the legacy migration ladder.
pub fn install_schema(connection: &Connection) -> Result<(), MemoryStoreError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'memory_entries')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let mut statement = connection.prepare("PRAGMA table_info(memory_entries)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|name| name == "record_version") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN record_version INTEGER NOT NULL DEFAULT 1",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "evidence_refs") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN evidence_refs TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "execution_event_refs") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN execution_event_refs TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "authority") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN authority TEXT NOT NULL DEFAULT 'user_asserted'",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "durability") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN durability TEXT NOT NULL DEFAULT 'durable'",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "confidence") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0",
            [],
        )?;
    }
    Ok(())
}

/// Полный список колонок в порядке, которого придерживается `map_record`.
/// Держится в одном месте, чтобы SELECT'ы не расходились между собой.
const COLUMNS: &str = "id, scope_kind, scope_id, title, content, provenance, privacy,
        created_at, expires_at, archived, forgotten, confirmations, lesson_key,
        kind, canonical_subject, confirmation_state, model_confidence,
        verification_confidence, privacy_class, source_trust, supersedes,
        superseded_by, supersession_reason, extractor_version, policy_version,
        validation_status, validated_at, provenance_source_id, record_version,
        evidence_refs, execution_event_refs, authority, durability, confidence";

/// Только те состояния, в которых запись считается активной памятью.
const RETRIEVABLE_PREDICATE: &str = "forgotten = 0 AND archived = 0
          AND confirmation_state = 'confirmed'
          AND validation_status IN ('not_required', 'valid')
          AND superseded_by IS NULL";

impl MemoryStoreSql {
    pub const INSERT: &'static str = "INSERT INTO memory_entries
        (id, scope_kind, scope_id, title, content, provenance, privacy,
         created_at, expires_at, archived, forgotten, confirmations, lesson_key,
         kind, canonical_subject, confirmation_state, model_confidence,
         verification_confidence, privacy_class, source_trust, supersedes,
         superseded_by, supersession_reason, extractor_version, policy_version,
         validation_status, validated_at, provenance_source_id, record_version,
         evidence_refs, execution_event_refs, authority, durability, confidence)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
                 ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34)";
    pub const ARCHIVE: &'static str =
        "UPDATE memory_entries SET archived = 1 WHERE id = ?1 AND forgotten = 0";
    /// Forget — logical deletion: statement, заголовок, provenance, canonical
    /// subject и evidence очищаются, строка остаётся только как носитель
    /// metadata и state.
    pub const FORGET: &'static str = "UPDATE memory_entries
        SET title = '', content = '', provenance = '', canonical_subject = NULL,
            provenance_source_id = NULL, lesson_key = NULL,
            evidence_refs = '[]', execution_event_refs = '[]',
            forgotten = 1, confirmation_state = 'forgotten'
        WHERE id = ?1";

    fn select_by_id() -> String {
        format!("SELECT {COLUMNS} FROM memory_entries WHERE id = ?1")
    }

    fn search_sql() -> String {
        format!(
            "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2
          AND {RETRIEVABLE_PREDICATE}
          AND (expires_at IS NULL OR expires_at > ?3)
          AND (lower(title) LIKE lower(?4) OR lower(content) LIKE lower(?4))
        ORDER BY id ASC LIMIT ?5"
        )
    }

    fn search_lessons_sql() -> String {
        format!(
            "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2
          AND {RETRIEVABLE_PREDICATE} AND lesson_key IS NOT NULL
          AND (expires_at IS NULL OR expires_at > ?3)
          AND (lower(title) LIKE lower(?4) OR lower(content) LIKE lower(?4))
        ORDER BY confirmations DESC, created_at DESC, id ASC LIMIT ?5"
        )
    }

    fn list_sql() -> String {
        format!(
            "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2
          AND forgotten = 0
          AND (?3 = 1 OR archived = 0)
        ORDER BY created_at DESC, id ASC LIMIT ?4"
        )
    }

    fn list_by_state_sql() -> String {
        format!(
            "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2
          AND confirmation_state = ?3 AND forgotten = 0
        ORDER BY created_at DESC, id ASC LIMIT ?4"
        )
    }

    fn conflict_candidates_sql() -> String {
        format!(
            "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2 AND kind = ?3
          AND {RETRIEVABLE_PREDICATE}
        ORDER BY created_at DESC, id ASC LIMIT ?4"
        )
    }

    pub fn insert(connection: &Connection, record: &MemoryRecord) -> Result<(), MemoryStoreError> {
        record.validate()?;
        connection.execute(
            Self::INSERT,
            params![
                record.id,
                record.scope.as_str(),
                record.scope_id,
                record.title,
                record.content,
                record.provenance,
                record.privacy.as_str(),
                record.created_at,
                record.expires_at,
                record.archived as i64,
                record.forgotten as i64,
                record.confirmations,
                record.lesson_key,
                record.extraction.kind,
                record.extraction.canonical_subject,
                record.extraction.confirmation_state,
                record.extraction.model_confidence,
                record.extraction.verification_confidence,
                record.extraction.privacy_class,
                record.extraction.source_trust,
                record.extraction.supersedes,
                record.extraction.superseded_by,
                record.extraction.supersession_reason,
                record.extraction.extractor_version,
                record.extraction.policy_version,
                record.extraction.validation_status,
                record.extraction.validated_at,
                record.extraction.provenance_source_id,
                record.extraction.record_version,
                serde_json::to_string(&record.extraction.evidence_refs)
                    .unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&record.extraction.execution_event_refs)
                    .unwrap_or_else(|_| "[]".into()),
                record.extraction.authority,
                record.extraction.durability,
                record.extraction.confidence,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_lesson(
        connection: &Connection,
        record: &MemoryRecord,
    ) -> Result<MemoryRecord, MemoryStoreError> {
        record.validate()?;
        let Some(lesson_key) = record.lesson_key.as_deref() else {
            Self::insert(connection, record)?;
            return Ok(record.clone());
        };
        let existing_id: Option<String> = connection
            .query_row(
                "SELECT id FROM memory_entries WHERE scope_kind = ?1 AND scope_id = ?2 AND lesson_key = ?3 AND forgotten = 0 LIMIT 1",
                params![record.scope.as_str(), record.scope_id, lesson_key],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing_id {
            connection.execute(
                "UPDATE memory_entries SET confirmations = confirmations + 1, created_at = ?2, expires_at = ?3 WHERE id = ?1",
                params![id, record.created_at, record.expires_at],
            )?;
            return Self::get_by_id(connection, &id)?
                .ok_or_else(|| MemoryStoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        Self::insert(connection, record)?;
        connection.execute(
            "DELETE FROM memory_entries WHERE id IN (
                SELECT id FROM memory_entries
                WHERE scope_kind = ?1 AND scope_id = ?2 AND lesson_key IS NOT NULL
                ORDER BY confirmations DESC, created_at DESC, id ASC LIMIT -1 OFFSET 128
            )",
            params![record.scope.as_str(), record.scope_id],
        )?;
        Ok(record.clone())
    }

    pub fn get_by_id(
        connection: &Connection,
        id: &str,
    ) -> Result<Option<MemoryRecord>, MemoryStoreError> {
        Ok(connection
            .query_row(&Self::select_by_id(), params![id], map_record)
            .optional()?)
    }

    pub fn search(
        connection: &Connection,
        scope: MemoryScope,
        scope_id: &str,
        query: &str,
        now: &str,
        limit: u32,
    ) -> Result<Vec<MemoryRecord>, MemoryStoreError> {
        if query.len() > MAX_QUERY_BYTES {
            return Err(MemoryStoreError::Limit {
                field: "query",
                max: MAX_QUERY_BYTES,
            });
        }
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        validate_required("now", now, MAX_TIMESTAMP_BYTES)?;
        let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let mut statement = connection.prepare(&Self::search_sql())?;
        let records = statement
            .query_map(
                params![
                    scope.as_str(),
                    scope_id,
                    now,
                    pattern,
                    i64::from(limit.min(100))
                ],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    /// Lists non-forgotten records for one exact scope, newest first.
    /// Unlike `search`, this is not lexically filtered and does not exclude
    /// expired records: bounded listing/cleanup of expired entries is a
    /// separate concern left to the caller.
    pub fn list(
        connection: &Connection,
        scope: MemoryScope,
        scope_id: &str,
        include_archived: bool,
        limit: u32,
    ) -> Result<Vec<MemoryRecord>, MemoryStoreError> {
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        let mut statement = connection.prepare(&Self::list_sql())?;
        let records = statement
            .query_map(
                params![
                    scope.as_str(),
                    scope_id,
                    include_archived as i64,
                    i64::from(limit.clamp(1, 100))
                ],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    pub fn search_lessons(
        connection: &Connection,
        scope: MemoryScope,
        scope_id: &str,
        query: &str,
        now: &str,
        limit: u32,
    ) -> Result<Vec<MemoryRecord>, MemoryStoreError> {
        if query.len() > MAX_QUERY_BYTES {
            return Err(MemoryStoreError::Limit {
                field: "query",
                max: MAX_QUERY_BYTES,
            });
        }
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        validate_required("now", now, MAX_TIMESTAMP_BYTES)?;
        let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let mut statement = connection.prepare(&Self::search_lessons_sql())?;
        let records = statement
            .query_map(
                params![
                    scope.as_str(),
                    scope_id,
                    now,
                    pattern,
                    i64::from(limit.min(100))
                ],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    pub fn archive(connection: &Connection, id: &str) -> Result<bool, MemoryStoreError> {
        Ok(connection.execute(Self::ARCHIVE, params![id])? == 1)
    }

    pub fn forget(connection: &Connection, id: &str) -> Result<bool, MemoryStoreError> {
        Ok(connection.execute(Self::FORGET, params![id])? == 1)
    }

    /// Записи в одном state (например, весь pending queue) для одного scope.
    pub fn list_by_state(
        connection: &Connection,
        scope: MemoryScope,
        scope_id: &str,
        state: &str,
        limit: u32,
    ) -> Result<Vec<MemoryRecord>, MemoryStoreError> {
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        let mut statement = connection.prepare(&Self::list_by_state_sql())?;
        let records = statement
            .query_map(
                params![
                    scope.as_str(),
                    scope_id,
                    state,
                    i64::from(limit.clamp(1, 100))
                ],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    /// Количество записей по состояниям — для pending/conflict/expired
    /// счётчиков в OperationsPanel без раскрытия body.
    pub fn count_by_state(
        connection: &Connection,
        scope: MemoryScope,
        scope_id: &str,
    ) -> Result<Vec<(String, i64)>, MemoryStoreError> {
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        let mut statement = connection.prepare(
            "SELECT confirmation_state, COUNT(*) FROM memory_entries
             WHERE scope_kind = ?1 AND scope_id = ?2
             GROUP BY confirmation_state ORDER BY confirmation_state ASC",
        )?;
        let counts = statement
            .query_map(params![scope.as_str(), scope_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(counts)
    }

    /// Активные записи того же kind в том же scope — вход для детектора
    /// конфликтов. Сравнение statement'ов детерминированно выполняет Core.
    pub fn conflict_candidates(
        connection: &Connection,
        scope: MemoryScope,
        scope_id: &str,
        kind: &str,
        limit: u32,
    ) -> Result<Vec<MemoryRecord>, MemoryStoreError> {
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        let mut statement = connection.prepare(&Self::conflict_candidates_sql())?;
        let records = statement
            .query_map(
                params![
                    scope.as_str(),
                    scope_id,
                    kind,
                    i64::from(limit.clamp(1, 100))
                ],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    /// Идемпотентный переход состояния. Повторный confirm/reject безопасен и
    /// возвращает фактическое текущее state. Терминальные состояния
    /// (`rejected`, `forgotten`, `superseded`) не переоткрываются.
    pub fn transition_state(
        connection: &Connection,
        id: &str,
        target: &str,
    ) -> Result<String, MemoryStoreError> {
        let transaction = connection.unchecked_transaction()?;
        let current: Option<String> = transaction
            .query_row(
                "SELECT confirmation_state FROM memory_entries WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let current = current.ok_or(MemoryStoreError::NotFound)?;
        if current == target {
            transaction.commit()?;
            return Ok(current);
        }
        if matches!(
            current.as_str(),
            "rejected" | "forgotten" | "superseded" | "expired"
        ) {
            // Повторное действие не меняет запись, но и не притворяется
            // успешным переходом: caller видит фактическое состояние.
            transaction.commit()?;
            return Ok(current);
        }
        if !matches!(
            (current.as_str(), target),
            ("candidate", "pending_confirmation")
                | ("candidate", "confirmed")
                | ("candidate", "rejected")
                | ("pending_confirmation", "confirmed")
                | ("pending_confirmation", "rejected")
                | ("confirmed", "superseded")
                | ("confirmed", "expired")
                | ("confirmed", "forgotten")
                | ("pending_confirmation", "expired")
        ) {
            return Err(MemoryStoreError::InvalidTransition {
                from: current,
                to: target.to_owned(),
            });
        }
        transaction.execute(
            "UPDATE memory_entries SET confirmation_state = ?2 WHERE id = ?1",
            params![id, target],
        )?;
        transaction.commit()?;
        Ok(target.to_owned())
    }

    /// Правка statement'а у записи, ожидающей подтверждения.
    ///
    /// После правки запись перестаёт быть model-generated: её текст написал
    /// пользователь, поэтому source trust становится `user`, версия
    /// извлекателя — `user_edited`, а прошлая проверка сбрасывается, ведь
    /// evidence относилась к прежней формулировке. Правка ничего не
    /// подтверждает: запись остаётся pending до явного confirm.
    pub fn revise_pending_statement(
        connection: &Connection,
        id: &str,
        statement: &str,
    ) -> Result<(), MemoryStoreError> {
        validate_required("statement", statement, MAX_CONTENT_BYTES)?;
        let transaction = connection.unchecked_transaction()?;
        let state: Option<String> = transaction
            .query_row(
                "SELECT confirmation_state FROM memory_entries WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let state = state.ok_or(MemoryStoreError::NotFound)?;
        if !matches!(state.as_str(), "pending_confirmation" | "candidate") {
            return Err(MemoryStoreError::InvalidTransition {
                from: state,
                to: "revised".to_owned(),
            });
        }
        transaction.execute(
            "UPDATE memory_entries
             SET content = ?2, source_trust = 'user', extractor_version = 'user_edited',
                 model_confidence = 1.0, verification_confidence = 0.0,
                 validation_status = 'not_required', validated_at = NULL
             WHERE id = ?1",
            params![id, redact_sensitive(statement)],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Явный выбор пользователя: `old_id` уступает место `new_id`. Цепочка
    /// `A -> B -> C` хранится через supersedes/superseded_by и обязательную
    /// причину. Операция транзакционная: параллельные confirm сериализуются.
    pub fn supersede(
        connection: &Connection,
        old_id: &str,
        new_id: &str,
        reason: &str,
    ) -> Result<(), MemoryStoreError> {
        validate_required("supersession_reason", reason, MAX_ID_BYTES)?;
        let transaction = connection.unchecked_transaction()?;
        let old_state: Option<String> = transaction
            .query_row(
                "SELECT confirmation_state FROM memory_entries WHERE id = ?1",
                params![old_id],
                |row| row.get(0),
            )
            .optional()?;
        let old_state = old_state.ok_or(MemoryStoreError::NotFound)?;
        if transaction
            .query_row(
                "SELECT 1 FROM memory_entries WHERE id = ?1",
                params![new_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_none()
        {
            return Err(MemoryStoreError::NotFound);
        }
        if old_state != "confirmed" {
            return Err(MemoryStoreError::InvalidTransition {
                from: old_state,
                to: "superseded".to_owned(),
            });
        }
        transaction.execute(
            "UPDATE memory_entries
             SET confirmation_state = 'superseded', superseded_by = ?2,
                 supersession_reason = ?3
             WHERE id = ?1",
            params![old_id, new_id, reason],
        )?;
        transaction.execute(
            "UPDATE memory_entries
             SET confirmation_state = 'confirmed', supersedes = ?2,
                 supersession_reason = ?3
             WHERE id = ?1",
            params![new_id, old_id, reason],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Цепочка supersede от записи вверх по `supersedes`, не длиннее `limit`.
    pub fn supersession_chain(
        connection: &Connection,
        id: &str,
        limit: usize,
    ) -> Result<Vec<String>, MemoryStoreError> {
        let mut chain = vec![id.to_owned()];
        let mut current = id.to_owned();
        while chain.len() < limit {
            let previous: Option<Option<String>> = connection
                .query_row(
                    "SELECT supersedes FROM memory_entries WHERE id = ?1",
                    params![current],
                    |row| row.get(0),
                )
                .optional()?;
            match previous.flatten() {
                // Циклы невозможны при корректном supersede, но защищаемся:
                // повтор id прекращает обход.
                Some(previous) if !chain.contains(&previous) => {
                    chain.push(previous.clone());
                    current = previous;
                }
                _ => break,
            }
        }
        chain.reverse();
        Ok(chain)
    }

    /// Помечает истёкшие записи. Истёкшая запись исключается из retrieval и
    /// может быть продлена только явным действием или новой проверкой.
    pub fn expire_due(connection: &Connection, now: &str) -> Result<usize, MemoryStoreError> {
        Ok(connection.execute(
            "UPDATE memory_entries SET confirmation_state = 'expired'
             WHERE expires_at IS NOT NULL AND expires_at <= ?1
               AND confirmation_state IN ('confirmed', 'pending_confirmation', 'candidate')",
            params![now],
        )?)
    }

    /// Forget с tombstone: body стирается, а в audit остаётся только
    /// случайный id, kind, scope, timestamps, класс причины и digest — без
    /// исходного текста.
    pub fn forget_with_tombstone(
        connection: &Connection,
        id: &str,
        tombstone_id: &str,
        reason_class: &str,
        forgotten_at: &str,
    ) -> Result<bool, MemoryStoreError> {
        validate_required("tombstone_id", tombstone_id, MAX_ID_BYTES)?;
        validate_required("reason_class", reason_class, MAX_ID_BYTES)?;
        validate_required("forgotten_at", forgotten_at, MAX_TIMESTAMP_BYTES)?;
        let transaction = connection.unchecked_transaction()?;
        let existing: Option<(String, String, String, String, String)> = transaction
            .query_row(
                "SELECT kind, scope_kind, scope_id, created_at, content
                 FROM memory_entries WHERE id = ?1 AND forgotten = 0",
                params![id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((kind, scope_kind, scope_id, created_at, content)) = existing else {
            transaction.commit()?;
            return Ok(false);
        };
        let digest = digest_hex(&content);
        transaction.execute(Self::FORGET, params![id])?;
        transaction.execute(
            "INSERT OR REPLACE INTO memory_tombstones
             (tombstone_id, kind, scope_kind, scope_id, created_at, forgotten_at,
              reason_class, digest)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                tombstone_id,
                kind,
                scope_kind,
                scope_id,
                created_at,
                forgotten_at,
                reason_class,
                digest
            ],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    /// Регистрирует alias -> entity id. Источник обязателен: model inference
    /// не может единолично создать alias.
    pub fn register_alias(
        connection: &Connection,
        scope: MemoryScope,
        scope_id: &str,
        alias: &str,
        entity_id: &str,
        registered_by: &str,
        created_at: &str,
    ) -> Result<(), MemoryStoreError> {
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        validate_required("alias", alias, MAX_SCOPE_ID_BYTES)?;
        validate_required("entity_id", entity_id, MAX_SCOPE_ID_BYTES)?;
        validate_required("registered_by", registered_by, MAX_ID_BYTES)?;
        validate_required("created_at", created_at, MAX_TIMESTAMP_BYTES)?;
        if registered_by == "model_inference" {
            return Err(MemoryStoreError::Empty {
                field: "registered_by",
            });
        }
        connection.execute(
            "INSERT OR REPLACE INTO memory_aliases
             (scope_kind, scope_id, alias, entity_id, registered_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                scope.as_str(),
                scope_id,
                alias,
                entity_id,
                registered_by,
                created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_aliases(
        connection: &Connection,
        scope: MemoryScope,
        scope_id: &str,
    ) -> Result<Vec<(String, String)>, MemoryStoreError> {
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        let mut statement = connection.prepare(
            "SELECT alias, entity_id FROM memory_aliases
             WHERE scope_kind = ?1 AND scope_id = ?2 ORDER BY alias ASC",
        )?;
        let aliases = statement
            .query_map(params![scope.as_str(), scope_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(aliases)
    }

    /// «Только на эту сессию»: отдельный session-scoped state с
    /// автоматическим expiry. Persistent row не создаётся.
    // Аргументы повторяют колонки заметки сессии в SQLite.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_session_note(
        connection: &Connection,
        id: &str,
        session_id: &str,
        scope: MemoryScope,
        scope_id: &str,
        kind: &str,
        statement: &str,
        created_at: &str,
        expires_at: &str,
    ) -> Result<(), MemoryStoreError> {
        validate_required("id", id, MAX_ID_BYTES)?;
        validate_required("session_id", session_id, MAX_ID_BYTES)?;
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        validate_required("kind", kind, MAX_ID_BYTES)?;
        validate_required("statement", statement, MAX_CONTENT_BYTES)?;
        validate_required("created_at", created_at, MAX_TIMESTAMP_BYTES)?;
        validate_required("expires_at", expires_at, MAX_TIMESTAMP_BYTES)?;
        connection.execute(
            "INSERT OR REPLACE INTO memory_session_notes
             (id, session_id, scope_kind, scope_id, kind, statement, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                session_id,
                scope.as_str(),
                scope_id,
                kind,
                redact_sensitive(statement),
                created_at,
                expires_at
            ],
        )?;
        Ok(())
    }

    pub fn list_session_notes(
        connection: &Connection,
        session_id: &str,
        now: &str,
    ) -> Result<Vec<(String, String)>, MemoryStoreError> {
        validate_required("session_id", session_id, MAX_ID_BYTES)?;
        validate_required("now", now, MAX_TIMESTAMP_BYTES)?;
        let mut statement = connection.prepare(
            "SELECT id, statement FROM memory_session_notes
             WHERE session_id = ?1 AND expires_at > ?2 ORDER BY created_at ASC, id ASC",
        )?;
        let notes = statement
            .query_map(params![session_id, now], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(notes)
    }

    pub fn purge_expired_session_notes(
        connection: &Connection,
        now: &str,
    ) -> Result<usize, MemoryStoreError> {
        validate_required("now", now, MAX_TIMESTAMP_BYTES)?;
        Ok(connection.execute(
            "DELETE FROM memory_session_notes WHERE expires_at <= ?1",
            params![now],
        )?)
    }
}

/// SHA-256 hex: tombstone хранит digest, а не исходный текст.
fn digest_hex(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let archived: i64 = row.get(9)?;
    let forgotten: i64 = row.get(10)?;
    Ok(MemoryRecord {
        id: row.get(0)?,
        scope: MemoryScope::parse(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
        scope_id: row.get(2)?,
        title: row.get(3)?,
        content: row.get(4)?,
        provenance: row.get(5)?,
        privacy: MemoryPrivacy::parse(&row.get::<_, String>(6)?).map_err(to_sql_error)?,
        created_at: row.get(7)?,
        expires_at: row.get(8)?,
        archived: archived != 0,
        forgotten: forgotten != 0,
        confirmations: row.get(11).unwrap_or(1),
        lesson_key: row.get(12).unwrap_or(None),
        extraction: MemoryExtractionFields {
            kind: row.get(13)?,
            canonical_subject: row.get(14)?,
            confirmation_state: row.get(15)?,
            model_confidence: row.get(16)?,
            verification_confidence: row.get(17)?,
            privacy_class: row.get(18)?,
            source_trust: row.get(19)?,
            supersedes: row.get(20)?,
            superseded_by: row.get(21)?,
            supersession_reason: row.get(22)?,
            extractor_version: row.get(23)?,
            policy_version: row.get(24)?,
            validation_status: row.get(25)?,
            validated_at: row.get(26)?,
            provenance_source_id: row.get(27)?,
            record_version: row.get(28).unwrap_or(1),
            evidence_refs: row
                .get::<_, Option<String>>(29)
                .unwrap_or(None)
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
            execution_event_refs: row
                .get::<_, Option<String>>(30)
                .unwrap_or(None)
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
            authority: row.get(31).unwrap_or_else(|_| default_authority()),
            durability: row.get(32).unwrap_or_else(|_| default_durability()),
            confidence: row.get(33).unwrap_or(1.0),
        },
    })
}

fn to_sql_error(error: MemoryStoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE memory_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    content TEXT NOT NULL,
                    provenance TEXT NOT NULL,
                    privacy TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    expires_at TEXT,
                    archived INTEGER NOT NULL,
                    forgotten INTEGER NOT NULL,
                    confirmations INTEGER NOT NULL DEFAULT 1,
                    lesson_key TEXT,
                    kind TEXT NOT NULL DEFAULT 'entity',
                    canonical_subject TEXT,
                    confirmation_state TEXT NOT NULL DEFAULT 'confirmed',
                    model_confidence REAL NOT NULL DEFAULT 1.0,
                    verification_confidence REAL NOT NULL DEFAULT 1.0,
                    privacy_class TEXT NOT NULL DEFAULT 'normal',
                    source_trust TEXT NOT NULL DEFAULT 'user',
                    supersedes TEXT,
                    superseded_by TEXT,
                    supersession_reason TEXT,
                    extractor_version TEXT NOT NULL DEFAULT 'v1_legacy',
                    policy_version TEXT NOT NULL DEFAULT 'legacy-v1',
                    validation_status TEXT NOT NULL DEFAULT 'not_required',
                    validated_at TEXT,
                    provenance_source_id TEXT,
                     record_version INTEGER NOT NULL DEFAULT 1,
                     evidence_refs TEXT NOT NULL DEFAULT '[]',
                     execution_event_refs TEXT NOT NULL DEFAULT '[]',
                     authority TEXT NOT NULL DEFAULT 'user_asserted',
                     durability TEXT NOT NULL DEFAULT 'durable',
                     confidence REAL NOT NULL DEFAULT 1.0
                );
                CREATE TABLE memory_aliases (
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    alias TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    registered_by TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (scope_kind, scope_id, alias)
                );
                CREATE TABLE memory_tombstones (
                    tombstone_id TEXT PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    forgotten_at TEXT NOT NULL,
                    reason_class TEXT NOT NULL,
                    digest TEXT NOT NULL
                );
                CREATE TABLE memory_session_notes (
                    id TEXT PRIMARY KEY NOT NULL,
                    session_id TEXT NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    statement TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL
                );",
            )
            .expect("memory schema creates");
    }

    fn pending(id: &str, subject: &str, statement: &str) -> MemoryRecord {
        let mut memory = record(id, statement);
        memory.extraction = MemoryExtractionFields {
            kind: "preference".to_owned(),
            canonical_subject: Some(subject.to_owned()),
            confirmation_state: "pending_confirmation".to_owned(),
            model_confidence: 0.9,
            verification_confidence: 0.0,
            extractor_version: "extractor-v1".to_owned(),
            policy_version: "extraction-policy-v1".to_owned(),
            ..MemoryExtractionFields::default()
        };
        memory
    }

    fn record(id: &str, content: &str) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryScope::Project,
            "project-1",
            "Decision",
            content,
            "run:1",
            MemoryPrivacy::Internal,
            "2026-08-12T10:00:00Z",
            Some("2027-01-01T00:00:00Z".into()),
        )
        .expect("memory record builds")
    }

    #[test]
    fn constructor_redacts_sensitive_content_and_bounds_fields() {
        let memory = record("m-1", "contact roman@example.test with token=secret");
        assert!(!memory.content.contains("roman@example.test"));
        assert!(!memory.content.contains("token=secret"));

        let too_large = MemoryRecord::new(
            "m-1",
            MemoryScope::Project,
            "project-1",
            "title",
            "x".repeat(MAX_CONTENT_BYTES + 1),
            "run:1",
            MemoryPrivacy::Internal,
            "2026-08-12T10:00:00Z",
            None,
        );
        assert!(matches!(
            too_large,
            Err(MemoryStoreError::Limit {
                field: "content",
                ..
            })
        ));
    }

    #[test]
    fn round_trip_search_is_scoped_bounded_and_deterministic() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert(&connection, &record("b", "Rust decision")).expect("insert b");
        MemoryStoreSql::insert(&connection, &record("a", "Rust decision")).expect("insert a");
        let other = MemoryRecord {
            scope_id: "other-project".into(),
            ..record("c", "Rust decision")
        };
        MemoryStoreSql::insert(&connection, &other).expect("insert other");

        let found = MemoryStoreSql::search(
            &connection,
            MemoryScope::Project,
            "project-1",
            "rust",
            "2026-09-01T00:00:00Z",
            10,
        )
        .expect("search memories");
        assert_eq!(
            found
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(
            MemoryStoreSql::get_by_id(&connection, "a").unwrap(),
            Some(record("a", "Rust decision"))
        );
    }

    #[test]
    fn list_is_scoped_and_hides_archived_unless_requested() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert(&connection, &record("b", "Rust decision")).expect("insert b");
        MemoryStoreSql::insert(&connection, &record("a", "Rust decision")).expect("insert a");
        let other = MemoryRecord {
            scope_id: "other-project".into(),
            ..record("c", "Rust decision")
        };
        MemoryStoreSql::insert(&connection, &other).expect("insert other");
        assert!(MemoryStoreSql::archive(&connection, "a").expect("archive a"));

        let active =
            MemoryStoreSql::list(&connection, MemoryScope::Project, "project-1", false, 10)
                .expect("list active");
        assert_eq!(
            active
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["b"]
        );

        let all = MemoryStoreSql::list(&connection, MemoryScope::Project, "project-1", true, 10)
            .expect("list including archived");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn archive_and_forget_remove_memory_from_search_without_deleting_row() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert(&connection, &record("m-1", "keep this fact")).expect("insert");
        assert!(MemoryStoreSql::archive(&connection, "m-1").expect("archive"));
        assert!(MemoryStoreSql::search(
            &connection,
            MemoryScope::Project,
            "project-1",
            "fact",
            "2026-09-01T00:00:00Z",
            10
        )
        .unwrap()
        .is_empty());

        assert!(MemoryStoreSql::forget(&connection, "m-1").expect("forget"));
        let forgotten = MemoryStoreSql::get_by_id(&connection, "m-1")
            .unwrap()
            .unwrap();
        assert!(forgotten.forgotten);
        assert!(forgotten.content.is_empty());
        assert!(forgotten.provenance.is_empty());
        assert_eq!(forgotten.extraction.confirmation_state, "forgotten");
    }

    #[test]
    fn pending_records_are_listable_but_never_retrievable() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert(&connection, &record("confirmed-1", "Rust decision"))
            .expect("insert confirmed");
        MemoryStoreSql::insert(
            &connection,
            &pending("pending-1", "язык интерфейса", "Rust decision"),
        )
        .expect("insert pending");

        // Search отдаёт только подтверждённую активную запись.
        let found = MemoryStoreSql::search(
            &connection,
            MemoryScope::Project,
            "project-1",
            "rust",
            "2026-09-01T00:00:00Z",
            10,
        )
        .expect("search");
        assert_eq!(
            found
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["confirmed-1"]
        );

        let queue = MemoryStoreSql::list_by_state(
            &connection,
            MemoryScope::Project,
            "project-1",
            "pending_confirmation",
            10,
        )
        .expect("pending queue");
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id, "pending-1");

        let counts = MemoryStoreSql::count_by_state(&connection, MemoryScope::Project, "project-1")
            .expect("counts");
        assert_eq!(
            counts,
            vec![
                ("confirmed".to_owned(), 1),
                ("pending_confirmation".to_owned(), 1)
            ]
        );
    }

    #[test]
    fn state_transitions_are_idempotent_and_terminal_states_stick() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert(&connection, &pending("p-1", "тема", "утверждение"))
            .expect("insert pending");

        assert_eq!(
            MemoryStoreSql::transition_state(&connection, "p-1", "confirmed").unwrap(),
            "confirmed"
        );
        // Повторный confirm безопасен и возвращает фактическое состояние.
        assert_eq!(
            MemoryStoreSql::transition_state(&connection, "p-1", "confirmed").unwrap(),
            "confirmed"
        );

        MemoryStoreSql::insert(&connection, &pending("p-2", "тема-2", "утверждение-2"))
            .expect("insert second");
        assert_eq!(
            MemoryStoreSql::transition_state(&connection, "p-2", "rejected").unwrap(),
            "rejected"
        );
        // Отклонённая запись не переоткрывается повторным confirm.
        assert_eq!(
            MemoryStoreSql::transition_state(&connection, "p-2", "confirmed").unwrap(),
            "rejected"
        );

        assert!(matches!(
            MemoryStoreSql::transition_state(&connection, "missing", "confirmed"),
            Err(MemoryStoreError::NotFound)
        ));
    }

    #[test]
    fn supersede_builds_a_chain_and_keeps_the_old_record_inactive() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        for id in ["a", "b", "c"] {
            let mut memory = pending(id, "тема", &format!("вариант {id}"));
            memory.extraction.confirmation_state = "confirmed".to_owned();
            MemoryStoreSql::insert(&connection, &memory).expect("insert");
        }
        MemoryStoreSql::supersede(&connection, "a", "b", "user_choice").expect("a -> b");
        MemoryStoreSql::supersede(&connection, "b", "c", "user_choice").expect("b -> c");

        let chain = MemoryStoreSql::supersession_chain(&connection, "c", 16).expect("chain");
        assert_eq!(chain, ["a", "b", "c"]);

        let old = MemoryStoreSql::get_by_id(&connection, "a")
            .unwrap()
            .unwrap();
        assert_eq!(old.extraction.confirmation_state, "superseded");
        assert_eq!(old.extraction.superseded_by.as_deref(), Some("b"));
        assert_eq!(
            old.extraction.supersession_reason.as_deref(),
            Some("user_choice")
        );

        // Только последняя запись цепочки участвует в retrieval.
        let active = MemoryStoreSql::conflict_candidates(
            &connection,
            MemoryScope::Project,
            "project-1",
            "preference",
            10,
        )
        .expect("active");
        assert_eq!(
            active
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["c"]
        );

        assert!(matches!(
            MemoryStoreSql::supersede(&connection, "a", "c", "user_choice"),
            Err(MemoryStoreError::InvalidTransition { .. })
        ));
        assert!(matches!(
            MemoryStoreSql::supersede(&connection, "c", "missing", "user_choice"),
            Err(MemoryStoreError::NotFound)
        ));
    }

    #[test]
    fn expire_due_removes_records_from_retrieval() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert(&connection, &record("m-1", "Rust decision")).expect("insert");
        assert_eq!(
            MemoryStoreSql::expire_due(&connection, "2026-08-12T10:00:00Z").expect("nothing due"),
            0
        );
        assert_eq!(
            MemoryStoreSql::expire_due(&connection, "2027-06-01T00:00:00Z").expect("expire"),
            1
        );
        assert!(MemoryStoreSql::search(
            &connection,
            MemoryScope::Project,
            "project-1",
            "rust",
            "2026-09-01T00:00:00Z",
            10
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn forget_leaves_only_metadata_and_a_digest_tombstone() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        let mut source = record("m-1", "конфиденциальная деталь");
        source.extraction.evidence_refs = vec!["evidence-1".into()];
        source.extraction.execution_event_refs = vec![42];
        MemoryStoreSql::insert(&connection, &source).expect("insert");
        assert!(MemoryStoreSql::forget_with_tombstone(
            &connection,
            "m-1",
            "tomb-random-1",
            "user_request",
            "2026-08-14T00:00:00Z"
        )
        .expect("forget"));

        let forgotten = MemoryStoreSql::get_by_id(&connection, "m-1")
            .unwrap()
            .unwrap();
        assert!(forgotten.content.is_empty());
        assert!(forgotten.title.is_empty());
        assert!(forgotten.provenance.is_empty());
        assert!(forgotten.extraction.canonical_subject.is_none());
        assert!(forgotten.extraction.evidence_refs.is_empty());
        assert!(forgotten.extraction.execution_event_refs.is_empty());
        assert_eq!(forgotten.extraction.confirmation_state, "forgotten");

        let (digest, reason): (String, String) = connection
            .query_row(
                "SELECT digest, reason_class FROM memory_tombstones WHERE tombstone_id = ?1",
                params!["tomb-random-1"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("tombstone exists");
        assert_eq!(digest.len(), 64);
        assert!(!digest.contains("конфиденциальная"));
        assert_eq!(reason, "user_request");

        // Повторный forget не создаёт второй tombstone.
        assert!(!MemoryStoreSql::forget_with_tombstone(
            &connection,
            "m-1",
            "tomb-random-2",
            "user_request",
            "2026-08-14T00:00:00Z"
        )
        .expect("second forget is a no-op"));
    }

    #[test]
    fn aliases_cannot_be_registered_by_model_inference() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::register_alias(
            &connection,
            MemoryScope::Project,
            "project-1",
            "ui язык",
            "entity:ui-language",
            "user",
            "2026-08-14T00:00:00Z",
        )
        .expect("user alias registers");
        assert!(matches!(
            MemoryStoreSql::register_alias(
                &connection,
                MemoryScope::Project,
                "project-1",
                "другой",
                "entity:other",
                "model_inference",
                "2026-08-14T00:00:00Z",
            ),
            Err(MemoryStoreError::Empty {
                field: "registered_by"
            })
        ));
        assert_eq!(
            MemoryStoreSql::list_aliases(&connection, MemoryScope::Project, "project-1").unwrap(),
            vec![("ui язык".to_owned(), "entity:ui-language".to_owned())]
        );
    }

    #[test]
    fn session_notes_expire_and_never_touch_persistent_memory() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert_session_note(
            &connection,
            "note-1",
            "session-1",
            MemoryScope::Session,
            "project-1",
            "preference",
            "только на эту сессию: краткие ответы",
            "2026-08-14T00:00:00Z",
            "2026-08-15T00:00:00Z",
        )
        .expect("session note");

        assert_eq!(
            MemoryStoreSql::list_session_notes(&connection, "session-1", "2026-08-14T12:00:00Z")
                .unwrap()
                .len(),
            1
        );
        assert!(MemoryStoreSql::list_session_notes(
            &connection,
            "session-1",
            "2026-08-16T00:00:00Z"
        )
        .unwrap()
        .is_empty());
        assert_eq!(
            MemoryStoreSql::purge_expired_session_notes(&connection, "2026-08-16T00:00:00Z")
                .unwrap(),
            1
        );
        // Persistent память при этом не создавалась.
        let persistent: i64 = connection
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(persistent, 0);
    }

    #[test]
    fn revising_a_pending_statement_makes_it_a_user_assertion_without_confirming_it() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        let mut record = pending("p-1", "тема", "модель предложила так");
        record.extraction.source_trust = "model_inference".to_owned();
        record.extraction.validation_status = "valid".to_owned();
        record.extraction.verification_confidence = 0.9;
        MemoryStoreSql::insert(&connection, &record).expect("insert pending");

        MemoryStoreSql::revise_pending_statement(&connection, "p-1", "пользователь написал так")
            .expect("revision applies");
        let revised = MemoryStoreSql::get_by_id(&connection, "p-1")
            .unwrap()
            .unwrap();
        assert_eq!(revised.content, "пользователь написал так");
        assert_eq!(revised.extraction.source_trust, "user");
        assert_eq!(revised.extraction.extractor_version, "user_edited");
        // Прошлая проверка относилась к прежней формулировке.
        assert_eq!(revised.extraction.verification_confidence, 0.0);
        assert_eq!(revised.extraction.validation_status, "not_required");
        // Правка не подтверждает запись.
        assert_eq!(
            revised.extraction.confirmation_state,
            "pending_confirmation"
        );

        // Секреты не проникают в память через поле правки.
        MemoryStoreSql::revise_pending_statement(&connection, "p-1", "ключ sk-live-42")
            .expect("revision applies");
        let redacted = MemoryStoreSql::get_by_id(&connection, "p-1")
            .unwrap()
            .unwrap();
        assert!(!redacted.content.contains("sk-live-42"));

        // Уже решённую запись править нельзя.
        MemoryStoreSql::transition_state(&connection, "p-1", "confirmed").unwrap();
        assert!(matches!(
            MemoryStoreSql::revise_pending_statement(&connection, "p-1", "поздно"),
            Err(MemoryStoreError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn candidate_state_never_reaches_retrieval_after_a_crash() {
        // A crash between the model call and the confirmation leaves a row in
        // `candidate` state. It must behave like nothing was ever learned:
        // invisible to search, still resolvable by the user afterwards.
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        let mut record = pending("c-1", "тема", "Rust decision");
        record.extraction.confirmation_state = "candidate".to_owned();
        MemoryStoreSql::insert(&connection, &record).expect("insert candidate");

        assert!(MemoryStoreSql::search(
            &connection,
            MemoryScope::Project,
            "project-1",
            "rust",
            "2026-09-01T00:00:00Z",
            10
        )
        .unwrap()
        .is_empty());
        assert!(MemoryStoreSql::conflict_candidates(
            &connection,
            MemoryScope::Project,
            "project-1",
            "preference",
            10
        )
        .unwrap()
        .is_empty());

        // Recovery can still route it through the normal approval path.
        assert_eq!(
            MemoryStoreSql::transition_state(&connection, "c-1", "pending_confirmation").unwrap(),
            "pending_confirmation"
        );
    }

    #[test]
    fn concurrent_decisions_on_one_record_converge_to_a_single_state() {
        // Two reviewers acting at once must not produce two transitions: the
        // second call reports the state the store actually holds.
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert(&connection, &pending("p-1", "тема", "утверждение"))
            .expect("insert pending");

        assert_eq!(
            MemoryStoreSql::transition_state(&connection, "p-1", "confirmed").unwrap(),
            "confirmed"
        );
        // The losing decision is refused outright rather than silently
        // overwriting the winner.
        assert!(matches!(
            MemoryStoreSql::transition_state(&connection, "p-1", "rejected"),
            Err(MemoryStoreError::InvalidTransition {
                ref from,
                ref to
            }) if from == "confirmed" && to == "rejected"
        ));
        let stored = MemoryStoreSql::get_by_id(&connection, "p-1")
            .unwrap()
            .unwrap();
        assert_eq!(stored.extraction.confirmation_state, "confirmed");

        // The mirror case: once rejected, a later confirm is a safe no-op
        // that reports the real state instead of reopening the record.
        MemoryStoreSql::insert(&connection, &pending("p-2", "тема-2", "утверждение-2"))
            .expect("insert second");
        assert_eq!(
            MemoryStoreSql::transition_state(&connection, "p-2", "rejected").unwrap(),
            "rejected"
        );
        assert_eq!(
            MemoryStoreSql::transition_state(&connection, "p-2", "confirmed").unwrap(),
            "rejected"
        );
    }

    #[test]
    fn a_large_pending_queue_stays_bounded_per_read() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        for index in 0..250 {
            MemoryStoreSql::insert(
                &connection,
                &pending(
                    &format!("p-{index:03}"),
                    "тема",
                    &format!("вариант {index}"),
                ),
            )
            .expect("insert");
        }
        let page = MemoryStoreSql::list_by_state(
            &connection,
            MemoryScope::Project,
            "project-1",
            "pending_confirmation",
            10_000,
        )
        .expect("bounded read");
        assert_eq!(
            page.len(),
            100,
            "read must stay bounded regardless of limit"
        );
        let counts = MemoryStoreSql::count_by_state(&connection, MemoryScope::Project, "project-1")
            .expect("counts");
        assert_eq!(counts, vec![("pending_confirmation".to_owned(), 250)]);
    }

    #[test]
    fn secret_privacy_class_is_never_persisted() {
        let mut memory = record("m-1", "содержимое");
        memory.extraction.privacy_class = "secret".to_owned();
        assert!(matches!(
            memory.validate(),
            Err(MemoryStoreError::SecretNotStorable)
        ));
        memory.extraction.privacy_class = "normal".to_owned();
        memory.extraction.model_confidence = 1.5;
        assert!(matches!(
            memory.validate(),
            Err(MemoryStoreError::InvalidConfidence)
        ));
    }

    #[test]
    fn v31_columns_install_idempotently_and_round_trip_refs() {
        let connection = Connection::open_in_memory().expect("connection");
        schema(&connection);
        install_schema(&connection).expect("first v31 install");
        install_schema(&connection).expect("second v31 install");
        let mut memory = record("refs", "with evidence");
        memory.extraction.evidence_refs = vec!["evidence-1".into()];
        memory.extraction.execution_event_refs = vec![42];
        memory.extraction.authority = "model_proposed".into();
        memory.extraction.durability = "durable".into();
        memory.extraction.confidence = 0.75;
        MemoryStoreSql::insert(&connection, &memory).expect("insert");
        let loaded = MemoryStoreSql::get_by_id(&connection, "refs")
            .expect("read")
            .expect("row");
        assert_eq!(loaded.extraction.record_version, 1);
        assert_eq!(loaded.extraction.evidence_refs, vec!["evidence-1"]);
        assert_eq!(loaded.extraction.execution_event_refs, vec![42]);
        assert_eq!(loaded.extraction.authority, "model_proposed");
        assert_eq!(loaded.extraction.durability, "durable");
        assert_eq!(loaded.extraction.confidence, 0.75);
    }
}
