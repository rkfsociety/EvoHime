//! Bounded Memory v1 API and policy contract.
//!
//! This file is intentionally standalone until the Core/storage wiring task.
//! It provides deterministic in-memory semantics for the public API, including
//! approval gates for destructive operations and export/delete.

#[allow(dead_code)]
#[path = "memory_domain.rs"]
mod domain;

use domain::{
    CreateMemory, MemoryDomain, MemoryError, MemoryRecord, MemoryScope, MemorySearchHit,
    MemoryStatus, PrivacyLabel, ProvenanceRef, MAX_QUERY_CHARS, MAX_RESULTS,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

pub const MAX_EXPORT_BYTES: usize = 256 * 1024;
pub const MAX_PROVENANCE_RESULTS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryApiError {
    Domain(MemoryError),
    ApprovalRequired { operation: MemoryOperation },
    InvalidApproval,
    NotFound,
    AlreadyForgotten,
    ExportTooLarge,
    TooManyProvenanceResults,
}

impl fmt::Display for MemoryApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(f, "memory domain error: {error}"),
            Self::ApprovalRequired { operation } => {
                write!(f, "approval required for {operation:?}")
            }
            Self::InvalidApproval => write!(f, "approval is missing or invalid"),
            Self::NotFound => write!(f, "memory was not found"),
            Self::AlreadyForgotten => write!(f, "memory was already forgotten"),
            Self::ExportTooLarge => write!(f, "memory export exceeds the bounded size"),
            Self::TooManyProvenanceResults => write!(f, "too many provenance results requested"),
        }
    }
}

impl std::error::Error for MemoryApiError {}

impl From<MemoryError> for MemoryApiError {
    fn from(value: MemoryError) -> Self {
        Self::Domain(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOperation {
    Create,
    Update,
    Archive,
    Forget,
    Export,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub approval_id: String,
    pub operation: MemoryOperation,
}

impl Approval {
    pub fn new(
        approval_id: impl Into<String>,
        operation: MemoryOperation,
    ) -> Result<Self, MemoryApiError> {
        let approval_id = approval_id.into();
        if approval_id.trim().is_empty() || approval_id.chars().count() > 128 {
            return Err(MemoryApiError::InvalidApproval);
        }
        Ok(Self {
            approval_id,
            operation,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryAuthorization {
    pub approval: Option<Approval>,
}

impl MemoryAuthorization {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn approved(
        approval_id: impl Into<String>,
        operation: MemoryOperation,
    ) -> Result<Self, MemoryApiError> {
        Ok(Self {
            approval: Some(Approval::new(approval_id, operation)?),
        })
    }

    fn check(&self, operation: MemoryOperation) -> Result<(), MemoryApiError> {
        match &self.approval {
            Some(approval) if approval.operation == operation => Ok(()),
            Some(_) => Err(MemoryApiError::InvalidApproval),
            None => Err(MemoryApiError::ApprovalRequired { operation }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryCreateRequest {
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
pub struct MemoryUpdateRequest {
    pub id: String,
    pub title: String,
    pub content: String,
    pub provenance: ProvenanceRef,
    pub privacy: PrivacyLabel,
    pub ttl_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryListRequest {
    pub scope: Option<MemoryScope>,
    pub include_archived: bool,
    pub include_expired: bool,
    pub now_ms: u64,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySearchRequest {
    pub query: String,
    pub scope: Option<MemoryScope>,
    pub now_ms: u64,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceInspectionRequest {
    pub scope: Option<MemoryScope>,
    pub kind: Option<String>,
    pub id: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceInspection {
    pub memory_id: String,
    pub scope: MemoryScope,
    pub provenance: ProvenanceRef,
    pub status: MemoryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryExport {
    pub records: Vec<MemoryRecord>,
}

#[derive(Debug, Default)]
pub struct MemoryApi {
    records: BTreeMap<String, MemoryRecord>,
}

impl MemoryApi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, request: MemoryCreateRequest) -> Result<MemoryRecord, MemoryApiError> {
        if self.records.contains_key(&request.id) {
            return Err(MemoryError::DuplicateId.into());
        }
        let record = MemoryDomain::new().create(CreateMemory {
            id: request.id,
            scope: request.scope,
            title: request.title,
            content: request.content,
            provenance: request.provenance,
            privacy: request.privacy,
            created_at_ms: request.created_at_ms,
            ttl_ms: request.ttl_ms,
        })?;
        self.records.insert(record.id.clone(), record.clone());
        Ok(record)
    }

    pub fn list(&self, request: MemoryListRequest) -> Result<Vec<MemoryRecord>, MemoryApiError> {
        if !(1..=MAX_RESULTS).contains(&request.limit) {
            return Err(MemoryError::InvalidLimit.into());
        }
        let mut records: Vec<_> = self
            .records
            .values()
            .filter(|record| record.status != MemoryStatus::Forgotten)
            .filter(|record| request.include_archived || record.status == MemoryStatus::Active)
            .filter(|record| request.include_expired || !record.is_expired_at(request.now_ms))
            .filter(|record| scope_matches(&record.scope, request.scope.as_ref()))
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

    pub fn search(
        &self,
        request: MemorySearchRequest,
    ) -> Result<Vec<MemorySearchHit>, MemoryApiError> {
        if !(1..=MAX_RESULTS).contains(&request.limit) {
            return Err(MemoryError::InvalidLimit.into());
        }
        let query = tokens(&request.query)?;
        let mut hits = self
            .records
            .values()
            .filter(|record| record.status == MemoryStatus::Active)
            .filter(|record| !record.is_expired_at(request.now_ms))
            .filter(|record| scope_matches(&record.scope, request.scope.as_ref()))
            .filter_map(|record| {
                let haystack = tokens(&format!("{} {}", record.title, record.content)).ok()?;
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
            .collect::<Vec<_>>();
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

    pub fn update(&mut self, request: MemoryUpdateRequest) -> Result<MemoryRecord, MemoryApiError> {
        let current = self
            .records
            .get(&request.id)
            .ok_or(MemoryApiError::NotFound)?
            .clone();
        if current.status == MemoryStatus::Forgotten {
            return Err(MemoryApiError::AlreadyForgotten);
        }
        let updated = MemoryDomain::new().create(CreateMemory {
            id: request.id,
            scope: current.scope,
            title: request.title,
            content: request.content,
            provenance: request.provenance,
            privacy: request.privacy,
            created_at_ms: current.created_at_ms,
            ttl_ms: request.ttl_ms,
        })?;
        let updated = MemoryRecord {
            status: current.status,
            ..updated
        };
        self.records.insert(updated.id.clone(), updated.clone());
        Ok(updated)
    }

    pub fn archive(
        &mut self,
        id: &str,
        authorization: &MemoryAuthorization,
    ) -> Result<(), MemoryApiError> {
        authorization.check(MemoryOperation::Archive)?;
        let record = self.records.get_mut(id).ok_or(MemoryApiError::NotFound)?;
        if record.status == MemoryStatus::Forgotten {
            return Err(MemoryApiError::AlreadyForgotten);
        }
        record.status = MemoryStatus::Archived;
        Ok(())
    }

    pub fn forget(
        &mut self,
        id: &str,
        authorization: &MemoryAuthorization,
    ) -> Result<(), MemoryApiError> {
        authorization.check(MemoryOperation::Forget)?;
        let record = self.records.get_mut(id).ok_or(MemoryApiError::NotFound)?;
        if record.status == MemoryStatus::Forgotten {
            return Err(MemoryApiError::AlreadyForgotten);
        }
        record.status = MemoryStatus::Forgotten;
        record.title.clear();
        record.content.clear();
        Ok(())
    }

    pub fn delete(
        &mut self,
        id: &str,
        authorization: &MemoryAuthorization,
    ) -> Result<MemoryRecord, MemoryApiError> {
        authorization.check(MemoryOperation::Delete)?;
        self.records.remove(id).ok_or(MemoryApiError::NotFound)
    }

    pub fn inspect_provenance(
        &self,
        request: ProvenanceInspectionRequest,
    ) -> Result<Vec<ProvenanceInspection>, MemoryApiError> {
        if !(1..=MAX_PROVENANCE_RESULTS).contains(&request.limit) {
            return Err(MemoryApiError::TooManyProvenanceResults);
        }
        let mut result = self
            .records
            .values()
            .filter(|record| scope_matches(&record.scope, request.scope.as_ref()))
            .filter(|record| {
                request
                    .kind
                    .as_deref()
                    .is_none_or(|kind| record.provenance.kind == kind)
            })
            .filter(|record| {
                request
                    .id
                    .as_deref()
                    .is_none_or(|id| record.provenance.id == id)
            })
            .map(|record| ProvenanceInspection {
                memory_id: record.id.clone(),
                scope: record.scope.clone(),
                provenance: record.provenance.clone(),
                status: record.status,
            })
            .collect::<Vec<_>>();
        result.sort_by(|left, right| left.memory_id.cmp(&right.memory_id));
        result.truncate(request.limit);
        Ok(result)
    }

    pub fn export(&self, authorization: &MemoryAuthorization) -> Result<String, MemoryApiError> {
        authorization.check(MemoryOperation::Export)?;
        let export = MemoryExport {
            records: self.records.values().cloned().collect(),
        };
        let json = serde_json::to_string(&export).expect("MemoryExport is serializable");
        if json.len() > MAX_EXPORT_BYTES {
            return Err(MemoryApiError::ExportTooLarge);
        }
        Ok(json)
    }
}

fn scope_matches(candidate: &MemoryScope, filter: Option<&MemoryScope>) -> bool {
    match filter {
        None => true,
        Some(MemoryScope::Project { project_id }) => candidate_project(candidate) == project_id,
        Some(MemoryScope::Task {
            project_id,
            task_id,
        }) => {
            candidate_project(candidate) == project_id
                && matches!(candidate, MemoryScope::Task { task_id: candidate, .. } if candidate == task_id)
        }
        Some(MemoryScope::Workspace {
            project_id,
            workspace_id,
        }) => {
            candidate_project(candidate) == project_id
                && matches!(candidate, MemoryScope::Workspace { workspace_id: candidate, .. } if candidate == workspace_id)
        }
    }
}

fn candidate_project(scope: &MemoryScope) -> &str {
    match scope {
        MemoryScope::Project { project_id }
        | MemoryScope::Task { project_id, .. }
        | MemoryScope::Workspace { project_id, .. } => project_id,
    }
}

fn tokens(value: &str) -> Result<Vec<String>, MemoryApiError> {
    if value.chars().count() > MAX_QUERY_CHARS {
        return Err(MemoryError::QueryTooLong.into());
    }
    let result = value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    if result.is_empty() {
        Err(MemoryError::EmptyQuery.into())
    } else {
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create(
        id: &str,
        scope: MemoryScope,
        content: &str,
        created_at_ms: u64,
    ) -> MemoryCreateRequest {
        MemoryCreateRequest {
            id: id.to_owned(),
            scope,
            title: "Memory title".to_owned(),
            content: content.to_owned(),
            provenance: ProvenanceRef::new("event", format!("event-{id}"), None).unwrap(),
            privacy: PrivacyLabel::Internal,
            created_at_ms,
            ttl_ms: 10_000,
        }
    }

    fn auth(operation: MemoryOperation) -> MemoryAuthorization {
        MemoryAuthorization::approved("approval-1", operation).unwrap()
    }

    #[test]
    fn create_list_search_are_scoped_redacted_and_deterministic() {
        let mut api = MemoryApi::new();
        api.create(create(
            "m-2",
            MemoryScope::task("p-1", "t-1").unwrap(),
            "Rust build sk-secret",
            2_000,
        ))
        .unwrap();
        api.create(create(
            "m-1",
            MemoryScope::task("p-1", "t-1").unwrap(),
            "Rust build notes",
            2_000,
        ))
        .unwrap();
        api.create(create(
            "m-3",
            MemoryScope::task("p-2", "t-1").unwrap(),
            "Rust other",
            3_000,
        ))
        .unwrap();
        let records = api
            .list(MemoryListRequest {
                scope: Some(MemoryScope::project("p-1").unwrap()),
                include_archived: false,
                include_expired: false,
                now_ms: 2_001,
                limit: 10,
            })
            .unwrap();
        assert_eq!(
            records
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            ["m-1", "m-2"]
        );
        assert_eq!(records[1].content, "Rust build [REDACTED]");
        let hits = api
            .search(MemorySearchRequest {
                query: "RUST build".to_owned(),
                scope: Some(MemoryScope::task("p-1", "t-1").unwrap()),
                now_ms: 2_001,
                limit: 10,
            })
            .unwrap();
        assert_eq!(hits[0].record.id, "m-1");
    }

    #[test]
    fn update_preserves_identity_and_archive_forget_require_approval() {
        let mut api = MemoryApi::new();
        api.create(create(
            "m-1",
            MemoryScope::project("p-1").unwrap(),
            "old",
            1_000,
        ))
        .unwrap();
        let updated = api
            .update(MemoryUpdateRequest {
                id: "m-1".to_owned(),
                title: "new title".to_owned(),
                content: "new content".to_owned(),
                provenance: ProvenanceRef::new("run", "run-1", None).unwrap(),
                privacy: PrivacyLabel::Private,
                ttl_ms: 20_000,
            })
            .unwrap();
        assert_eq!(updated.id, "m-1");
        assert_eq!(
            api.archive("m-1", &MemoryAuthorization::none()),
            Err(MemoryApiError::ApprovalRequired {
                operation: MemoryOperation::Archive
            })
        );
        api.archive("m-1", &auth(MemoryOperation::Archive)).unwrap();
        assert!(api
            .list(MemoryListRequest {
                scope: None,
                include_archived: false,
                include_expired: true,
                now_ms: 1_001,
                limit: 10
            })
            .unwrap()
            .is_empty());
        api.forget("m-1", &auth(MemoryOperation::Forget)).unwrap();
    }

    #[test]
    fn provenance_export_and_delete_are_bounded_and_approved() {
        let mut api = MemoryApi::new();
        api.create(create(
            "m-1",
            MemoryScope::project("p-1").unwrap(),
            "content",
            1_000,
        ))
        .unwrap();
        let provenance = api
            .inspect_provenance(ProvenanceInspectionRequest {
                scope: None,
                kind: Some("event".to_owned()),
                id: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(provenance[0].memory_id, "m-1");
        assert_eq!(
            api.export(&MemoryAuthorization::none()),
            Err(MemoryApiError::ApprovalRequired {
                operation: MemoryOperation::Export
            })
        );
        let json = api.export(&auth(MemoryOperation::Export)).unwrap();
        assert!(json.contains("m-1"));
        assert_eq!(
            api.delete("m-1", &MemoryAuthorization::none()),
            Err(MemoryApiError::ApprovalRequired {
                operation: MemoryOperation::Delete
            })
        );
        assert!(api.delete("m-1", &auth(MemoryOperation::Delete)).is_ok());
    }
}
