//! Автономный bounded Memory v1 domain contract.
//!
//! Модуль намеренно не подключён к `lib.rs`: он описывает безопасную границу
//! домена до появления persistence и IPC. Поиск здесь только lexical и
//! детерминированный; vector/RAG и сетевое обогащение не входят в контракт.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_MEMORY_ID_CHARS: usize = 128;
pub const MAX_SCOPE_ID_CHARS: usize = 128;
pub const MAX_TITLE_CHARS: usize = 256;
pub const MAX_CONTENT_CHARS: usize = 8_192;
pub const MAX_PROVENANCE_KIND_CHARS: usize = 64;
pub const MAX_PROVENANCE_ID_CHARS: usize = 256;
pub const MAX_LOCATOR_CHARS: usize = 512;
pub const MAX_QUERY_CHARS: usize = 512;
pub const MAX_RESULTS: usize = 128;
pub const MAX_TTL_MS: u64 = 31 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    EmptyField(&'static str),
    DuplicateId,
    FieldTooLong { field: &'static str, max: usize },
    InvalidTimestamp,
    InvalidTtl,
    InvalidLimit,
    EmptyQuery,
    QueryTooLong,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::DuplicateId => write!(f, "memory id must be unique"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::InvalidTimestamp => write!(f, "timestamp must be a positive Unix timestamp"),
            Self::InvalidTtl => write!(f, "ttl must be between 1 ms and {MAX_TTL_MS} ms"),
            Self::InvalidLimit => write!(f, "limit must be between 1 and {MAX_RESULTS}"),
            Self::EmptyQuery => write!(f, "query must contain at least one lexical token"),
            Self::QueryTooLong => write!(f, "query exceeds {MAX_QUERY_CHARS} characters"),
        }
    }
}

impl std::error::Error for MemoryError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryScope {
    Project {
        project_id: String,
    },
    Task {
        project_id: String,
        task_id: String,
    },
    Workspace {
        project_id: String,
        workspace_id: String,
    },
}

impl MemoryScope {
    pub fn project(project_id: impl Into<String>) -> Result<Self, MemoryError> {
        let scope = Self::Project {
            project_id: project_id.into(),
        };
        scope.validate()?;
        Ok(scope)
    }

    pub fn task(
        project_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let scope = Self::Task {
            project_id: project_id.into(),
            task_id: task_id.into(),
        };
        scope.validate()?;
        Ok(scope)
    }

    pub fn workspace(
        project_id: impl Into<String>,
        workspace_id: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let scope = Self::Workspace {
            project_id: project_id.into(),
            workspace_id: workspace_id.into(),
        };
        scope.validate()?;
        Ok(scope)
    }

    fn validate(&self) -> Result<(), MemoryError> {
        match self {
            Self::Project { project_id } => {
                validate_text("project_id", project_id, MAX_SCOPE_ID_CHARS)
            }
            Self::Task {
                project_id,
                task_id,
            } => {
                validate_text("project_id", project_id, MAX_SCOPE_ID_CHARS)?;
                validate_text("task_id", task_id, MAX_SCOPE_ID_CHARS)
            }
            Self::Workspace {
                project_id,
                workspace_id,
            } => {
                validate_text("project_id", project_id, MAX_SCOPE_ID_CHARS)?;
                validate_text("workspace_id", workspace_id, MAX_SCOPE_ID_CHARS)
            }
        }
    }

    fn matches_filter(&self, filter: Option<&MemoryScope>) -> bool {
        match filter {
            None => true,
            Some(filter) if self.project_id() != filter.project_id() => false,
            Some(Self::Project { .. }) => true,
            Some(Self::Task { task_id, .. }) => {
                matches!(self, Self::Project { .. })
                    || matches!(self, Self::Task { task_id: candidate, .. } if candidate == task_id)
            }
            Some(Self::Workspace { workspace_id, .. }) => {
                matches!(self, Self::Project { .. })
                    || matches!(self, Self::Workspace { workspace_id: candidate, .. } if candidate == workspace_id)
            }
        }
    }

    fn project_id(&self) -> &str {
        match self {
            Self::Project { project_id }
            | Self::Task { project_id, .. }
            | Self::Workspace { project_id, .. } => project_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceRef {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub locator: Option<String>,
}

impl ProvenanceRef {
    pub fn new(
        kind: impl Into<String>,
        id: impl Into<String>,
        locator: Option<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            kind: kind.into(),
            id: id.into(),
            locator,
        };
        validate_text("provenance.kind", &value.kind, MAX_PROVENANCE_KIND_CHARS)?;
        validate_text("provenance.id", &value.id, MAX_PROVENANCE_ID_CHARS)?;
        if let Some(locator) = &value.locator {
            validate_text("provenance.locator", locator, MAX_LOCATOR_CHARS)?;
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLabel {
    Public,
    Internal,
    Private,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Archived,
    Forgotten,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub scope: MemoryScope,
    pub title: String,
    pub content: String,
    pub provenance: ProvenanceRef,
    pub privacy: PrivacyLabel,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub status: MemoryStatus,
}

impl MemoryRecord {
    pub fn is_expired_at(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateMemory {
    pub id: String,
    pub scope: MemoryScope,
    pub title: String,
    pub content: String,
    pub provenance: ProvenanceRef,
    pub privacy: PrivacyLabel,
    pub created_at_ms: u64,
    pub ttl_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMemory {
    pub scope: Option<MemoryScope>,
    pub include_archived: bool,
    pub include_expired: bool,
    pub now_ms: u64,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMemory {
    pub query: String,
    pub scope: Option<MemoryScope>,
    pub now_ms: u64,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySearchHit {
    pub record: MemoryRecord,
    pub score: u32,
}

/// Bounded reference implementation of the Memory domain.
///
/// It intentionally owns no persistence and exposes no embedding/vector API.
#[derive(Debug, Default)]
pub struct MemoryDomain {
    records: Vec<MemoryRecord>,
}

impl MemoryDomain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, input: CreateMemory) -> Result<MemoryRecord, MemoryError> {
        validate_text("id", &input.id, MAX_MEMORY_ID_CHARS)?;
        validate_text("title", &input.title, MAX_TITLE_CHARS)?;
        validate_text("content", &input.content, MAX_CONTENT_CHARS)?;
        input.scope.validate()?;
        if input.created_at_ms == 0 {
            return Err(MemoryError::InvalidTimestamp);
        }
        if !(1..=MAX_TTL_MS).contains(&input.ttl_ms) {
            return Err(MemoryError::InvalidTtl);
        }
        if self.records.iter().any(|record| record.id == input.id) {
            return Err(MemoryError::DuplicateId);
        }
        let record = MemoryRecord {
            id: input.id,
            scope: input.scope,
            title: input.title,
            content: redact_text(&input.content)?,
            provenance: input.provenance,
            privacy: input.privacy,
            created_at_ms: input.created_at_ms,
            expires_at_ms: input
                .created_at_ms
                .checked_add(input.ttl_ms)
                .ok_or(MemoryError::InvalidTtl)?,
            status: MemoryStatus::Active,
        };
        self.records.push(record.clone());
        Ok(record)
    }

    pub fn list(&self, request: ListMemory) -> Result<Vec<MemoryRecord>, MemoryError> {
        validate_limit(request.limit)?;
        let mut records: Vec<_> = self
            .records
            .iter()
            .filter(|record| record.status != MemoryStatus::Forgotten)
            .filter(|record| request.include_archived || record.status == MemoryStatus::Active)
            .filter(|record| request.include_expired || !record.is_expired_at(request.now_ms))
            .filter(|record| record.scope.matches_filter(request.scope.as_ref()))
            .cloned()
            .collect();
        records.sort_by(|left, right| {
            right
                .created_at_ms
                .cmp(&left.created_at_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        records.truncate(request.limit);
        Ok(records)
    }

    pub fn search(&self, request: SearchMemory) -> Result<Vec<MemorySearchHit>, MemoryError> {
        validate_limit(request.limit)?;
        let query = lexical_tokens(&request.query)?;
        let mut hits: Vec<_> = self
            .records
            .iter()
            .filter(|record| record.status == MemoryStatus::Active)
            .filter(|record| !record.is_expired_at(request.now_ms))
            .filter(|record| record.scope.matches_filter(request.scope.as_ref()))
            .filter_map(|record| {
                let haystack =
                    lexical_tokens(&format!("{} {}", record.title, record.content)).ok()?;
                let score = query
                    .iter()
                    .map(|term| {
                        haystack
                            .iter()
                            .filter(|candidate| *candidate == term)
                            .count() as u32
                    })
                    .sum::<u32>();
                (score > 0).then(|| MemorySearchHit {
                    record: record.clone(),
                    score,
                })
            })
            .collect();
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| right.record.created_at_ms.cmp(&left.record.created_at_ms))
                .then_with(|| left.record.id.cmp(&right.record.id))
        });
        hits.truncate(request.limit);
        Ok(hits)
    }

    pub fn archive(&mut self, id: &str) -> bool {
        self.records
            .iter_mut()
            .find(|record| record.id == id && record.status == MemoryStatus::Active)
            .map(|record| record.status = MemoryStatus::Archived)
            .is_some()
    }

    pub fn forget(&mut self, id: &str) -> bool {
        self.records
            .iter_mut()
            .find(|record| record.id == id && record.status != MemoryStatus::Forgotten)
            .map(|record| {
                record.status = MemoryStatus::Forgotten;
                record.content.clear();
                record.title.clear();
            })
            .is_some()
    }
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), MemoryError> {
    if value.trim().is_empty() {
        return Err(MemoryError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(MemoryError::FieldTooLong { field, max });
    }
    Ok(())
}

fn validate_limit(limit: usize) -> Result<(), MemoryError> {
    if (1..=MAX_RESULTS).contains(&limit) {
        Ok(())
    } else {
        Err(MemoryError::InvalidLimit)
    }
}

fn lexical_tokens(value: &str) -> Result<Vec<String>, MemoryError> {
    if value.chars().count() > MAX_QUERY_CHARS {
        return Err(MemoryError::QueryTooLong);
    }
    let tokens: Vec<_> = value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect();
    if tokens.is_empty() {
        Err(MemoryError::EmptyQuery)
    } else {
        Ok(tokens)
    }
}

fn redact_text(input: &str) -> Result<String, MemoryError> {
    if input.chars().count() > MAX_CONTENT_CHARS {
        return Err(MemoryError::FieldTooLong {
            field: "content",
            max: MAX_CONTENT_CHARS,
        });
    }
    let mut redacted = String::with_capacity(input.len());
    let mut redact_next = false;
    for token in input.split_inclusive(char::is_whitespace) {
        let trimmed = token.trim_end_matches(char::is_whitespace);
        let suffix = &token[trimmed.len()..];
        let is_bearer_marker = trimmed.eq_ignore_ascii_case("bearer");
        let should_redact = redact_next || is_sensitive_token(trimmed);
        redacted.push_str(if should_redact { "[REDACTED]" } else { trimmed });
        redacted.push_str(suffix);
        redact_next = is_bearer_marker;
    }
    Ok(redacted)
}

fn is_sensitive_token(token: &str) -> bool {
    let normalized = token.trim_matches(|character: char| {
        matches!(character, ',' | '.' | ';' | ':' | ')' | ']' | '}')
    });
    let lower = normalized.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("aiza")
        || (normalized.contains('@')
            && normalized
                .split('@')
                .next()
                .is_some_and(|part| !part.is_empty()))
        || lower.starts_with("api_key=")
        || lower.starts_with("token=")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> MemoryScope {
        MemoryScope::task("project-1", "task-1").unwrap()
    }

    fn provenance() -> ProvenanceRef {
        ProvenanceRef::new("event", "evt-1", Some("run-1".to_owned())).unwrap()
    }

    fn create(id: &str, title: &str, content: &str, created_at_ms: u64) -> CreateMemory {
        CreateMemory {
            id: id.to_owned(),
            scope: scope(),
            title: title.to_owned(),
            content: content.to_owned(),
            provenance: provenance(),
            privacy: PrivacyLabel::Internal,
            created_at_ms,
            ttl_ms: 10_000,
        }
    }

    #[test]
    fn create_redacts_sensitive_content_and_sets_ttl() {
        let mut domain = MemoryDomain::new();
        let record = domain
            .create(create(
                "m-1",
                "Deployment note",
                "Use sk-secret and alice@example.test",
                1_000,
            ))
            .unwrap();
        assert_eq!(record.content, "Use [REDACTED] and [REDACTED]");
        assert_eq!(record.expires_at_ms, 11_000);
        assert_eq!(record.status, MemoryStatus::Active);
    }

    #[test]
    fn lexical_search_is_case_insensitive_and_deterministic() {
        let mut domain = MemoryDomain::new();
        domain
            .create(create("m-2", "Rust build", "Build cache", 2_000))
            .unwrap();
        domain
            .create(create("m-1", "Rust build", "Build notes", 2_000))
            .unwrap();
        let request = SearchMemory {
            query: "RUST build".to_owned(),
            scope: Some(scope()),
            now_ms: 2_001,
            limit: 10,
        };
        let hits = domain.search(request).unwrap();
        assert_eq!(
            hits.iter()
                .map(|hit| hit.record.id.as_str())
                .collect::<Vec<_>>(),
            ["m-1", "m-2"]
        );
        assert_eq!(hits[0].score, 3);
    }

    #[test]
    fn scope_filter_accepts_project_and_excludes_other_project() {
        let mut domain = MemoryDomain::new();
        domain.create(create("m-1", "Task", "one", 1_000)).unwrap();
        let mut sibling = create("m-3", "Sibling", "three", 3_000);
        sibling.scope = MemoryScope::task("project-1", "task-2").unwrap();
        domain.create(sibling).unwrap();
        let mut other = create("m-2", "Other", "two", 2_000);
        other.scope = MemoryScope::project("project-2").unwrap();
        domain.create(other).unwrap();
        let records = domain
            .list(ListMemory {
                scope: Some(scope()),
                include_archived: false,
                include_expired: false,
                now_ms: 2_000,
                limit: 10,
            })
            .unwrap();
        assert_eq!(
            records
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            ["m-1"]
        );

        let project_records = domain
            .list(ListMemory {
                scope: Some(MemoryScope::project("project-1").unwrap()),
                include_archived: false,
                include_expired: false,
                now_ms: 3_001,
                limit: 10,
            })
            .unwrap();
        assert_eq!(project_records.len(), 2);
    }

    #[test]
    fn archive_is_hidden_by_default_and_forget_erases_content() {
        let mut domain = MemoryDomain::new();
        domain
            .create(create("m-1", "Archive", "secret", 1_000))
            .unwrap();
        assert!(domain.archive("m-1"));
        assert!(domain
            .list(ListMemory {
                scope: None,
                include_archived: false,
                include_expired: false,
                now_ms: 1_001,
                limit: 10,
            })
            .unwrap()
            .is_empty());
        assert!(domain.forget("m-1"));
        assert!(domain
            .list(ListMemory {
                scope: None,
                include_archived: true,
                include_expired: true,
                now_ms: 1_001,
                limit: 10,
            })
            .unwrap()
            .is_empty());
    }

    #[test]
    fn ttl_and_query_bounds_are_enforced() {
        let mut domain = MemoryDomain::new();
        let mut input = create("m-1", "Title", "content", 1);
        input.ttl_ms = 0;
        assert_eq!(domain.create(input), Err(MemoryError::InvalidTtl));
        assert_eq!(
            domain.create(create("m-1", "Title", "content", 1)),
            Ok(MemoryRecord {
                id: "m-1".to_owned(),
                scope: scope(),
                title: "Title".to_owned(),
                content: "content".to_owned(),
                provenance: provenance(),
                privacy: PrivacyLabel::Internal,
                created_at_ms: 1,
                expires_at_ms: 10_001,
                status: MemoryStatus::Active,
            })
        );
        assert_eq!(
            domain.create(create("m-1", "Duplicate", "content", 2)),
            Err(MemoryError::DuplicateId)
        );
        assert_eq!(
            domain.search(SearchMemory {
                query: " ".to_owned(),
                scope: None,
                now_ms: 1,
                limit: 1,
            }),
            Err(MemoryError::EmptyQuery)
        );
        assert_eq!(
            domain.list(ListMemory {
                scope: None,
                include_archived: false,
                include_expired: false,
                now_ms: 1,
                limit: 0,
            }),
            Err(MemoryError::InvalidLimit)
        );
    }

    #[test]
    fn expired_memories_are_excluded_unless_explicitly_requested() {
        let mut domain = MemoryDomain::new();
        let mut input = create("m-1", "Old", "content", 1_000);
        input.ttl_ms = 1;
        domain.create(input).unwrap();
        let request = ListMemory {
            scope: None,
            include_archived: false,
            include_expired: false,
            now_ms: 1_001,
            limit: 10,
        };
        assert!(domain.list(request).unwrap().is_empty());
        assert_eq!(
            domain
                .list(ListMemory {
                    include_expired: true,
                    scope: None,
                    include_archived: false,
                    now_ms: 1_001,
                    limit: 10,
                })
                .unwrap()
                .len(),
            1
        );
    }
}
