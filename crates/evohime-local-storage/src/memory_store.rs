//! Migration-neutral bounded persistence contract for Memory v1.
//!
//! The module intentionally does not register itself in `lib.rs` or create a
//! migration. The storage owner decides when the compatible table exists;
//! this file keeps the record bounds and parameterized SQL contract together.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub use crate::memory_inputs::{InsertSessionNoteInput, MemoryRecordInput};
use crate::memory_mapping::map_record;
pub use crate::memory_schema::install_schema;

pub const MAX_ID_BYTES: usize = 256;
pub const MAX_SCOPE_ID_BYTES: usize = 512;
pub const MAX_TITLE_BYTES: usize = 512;
pub const MAX_CONTENT_BYTES: usize = 32 * 1024;
pub const MAX_PROVENANCE_BYTES: usize = 2 * 1024;
pub const MAX_TIMESTAMP_BYTES: usize = 64;
pub const MAX_QUERY_BYTES: usize = 512;
pub const MAX_TTL_SECONDS: u64 = 366 * 24 * 60 * 60;
pub const MAX_EVIDENCE_REFS: usize = 64;
pub const MAX_METADATA_ROWS: usize = 500;

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

    pub(crate) fn parse(value: &str) -> Result<Self, MemoryStoreError> {
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

pub(crate) fn default_authority() -> String {
    "user_asserted".to_owned()
}
pub(crate) fn default_durability() -> String {
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
    pub fn new(input: MemoryRecordInput) -> Result<Self, MemoryStoreError> {
        let record = Self {
            id: input.id,
            scope: input.scope,
            scope_id: input.scope_id,
            title: input.title,
            content: redact_sensitive(&input.content),
            provenance: input.provenance,
            privacy: input.privacy,
            created_at: input.created_at,
            expires_at: input.expires_at,
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
    #[error("memory extraction idempotency key was reused for another source basis")]
    ExtractionIdempotencyConflict,
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
            .query_row(
                &crate::memory_queries::select_by_id(),
                params![id],
                map_record,
            )
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
        let pattern = format!(
            "%{}%",
            query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let mut statement = connection.prepare(&crate::memory_queries::search())?;
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
        let mut statement = connection.prepare(&crate::memory_queries::list())?;
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
        let mut statement = connection.prepare(&crate::memory_queries::search_lessons())?;
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
        let mut statement = connection.prepare(&crate::memory_queries::list_by_state())?;
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
        let mut statement = connection.prepare(&crate::memory_queries::conflict_candidates())?;
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
        let tombstone_exists: Option<i64> = transaction
            .query_row(
                "SELECT 1 FROM memory_tombstones WHERE tombstone_id = ?1",
                params![tombstone_id],
                |row| row.get(0),
            )
            .optional()?;
        if tombstone_exists.is_some() {
            transaction.commit()?;
            return Ok(false);
        }
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
            "INSERT INTO memory_tombstones
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
             WHERE scope_kind = ?1 AND scope_id = ?2 ORDER BY alias ASC LIMIT ?3",
        )?;
        let aliases = statement
            .query_map(
                params![scope.as_str(), scope_id, MAX_METADATA_ROWS as i64],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(aliases)
    }

    /// «Только на эту сессию»: отдельный session-scoped state с
    /// автоматическим expiry. Persistent row не создаётся.
    pub fn insert_session_note(
        connection: &Connection,
        input: InsertSessionNoteInput<'_>,
    ) -> Result<(), MemoryStoreError> {
        validate_required("id", input.id, MAX_ID_BYTES)?;
        validate_required("session_id", input.session_id, MAX_ID_BYTES)?;
        validate_required("scope_id", input.scope_id, MAX_SCOPE_ID_BYTES)?;
        validate_required("kind", input.kind, MAX_ID_BYTES)?;
        validate_required("statement", input.statement, MAX_CONTENT_BYTES)?;
        validate_required("created_at", input.created_at, MAX_TIMESTAMP_BYTES)?;
        validate_required("expires_at", input.expires_at, MAX_TIMESTAMP_BYTES)?;
        connection.execute(
            "INSERT OR REPLACE INTO memory_session_notes
             (id, session_id, scope_kind, scope_id, kind, statement, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                input.id,
                input.session_id,
                input.scope.as_str(),
                input.scope_id,
                input.kind,
                redact_sensitive(input.statement),
                input.created_at,
                input.expires_at
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
             WHERE session_id = ?1 AND expires_at > ?2 ORDER BY created_at ASC, id ASC LIMIT ?3",
        )?;
        let notes = statement
            .query_map(params![session_id, now, MAX_METADATA_ROWS as i64], |row| {
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

#[cfg(test)]
#[path = "memory_store_tests.rs"]
mod tests;
