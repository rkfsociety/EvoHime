//! Core-owned Continual Refinement contract.
//!
//! This module admits bounded evidence and creates proposals only after
//! independent task observations. It has no tool, grant, credential, or
//! policy mutation authority.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_TITLE_CHARS: usize = 256;
pub const MAX_CONTENT_CHARS: usize = 8_192;
pub const MAX_EVIDENCE_REFS: usize = 64;
pub const MAX_TASK_IDS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Memory,
    Skill,
    PromptRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerScope {
    Session,
    Workspace,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    Draft,
    Evaluating,
    Proposed,
    Approved,
    Active,
    Superseded,
    RolledBack,
    Rejected,
    FailedEvaluation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRefV1 {
    pub source_id: String,
    pub source_kind: String,
    pub owner_scope: OwnerScope,
    pub content_hash: String,
    pub observed_at_ms: i64,
    pub redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefinementCandidateV1 {
    pub schema_version: u32,
    pub id: String,
    pub revision: i64,
    pub kind: CandidateKind,
    pub target: String,
    pub scope: OwnerScope,
    pub pattern_key: String,
    pub title: String,
    pub rationale: String,
    pub proposed_content: String,
    pub source_task_ids: Vec<String>,
    pub evidence: Vec<EvidenceRefV1>,
    pub conflicts: Vec<EvidenceRefV1>,
    pub confidence: u32,
    pub content_hash: String,
    pub policy_snapshot_hash: String,
    pub idempotency_key: String,
    pub status: CandidateStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionPolicy {
    pub min_evidence: usize,
    pub min_independent_tasks: usize,
    pub max_candidates: usize,
}

impl Default for AdmissionPolicy {
    fn default() -> Self {
        Self {
            min_evidence: 2,
            min_independent_tasks: 2,
            max_candidates: 128,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RefinementError {
    #[error("unsupported refinement contract version {0}")]
    UnsupportedVersion(u32),
    #[error("field {0} is empty or invalid")]
    InvalidField(&'static str),
    #[error("field {field} exceeds {max} characters")]
    TooLong { field: &'static str, max: usize },
    #[error("too many bounded {0}")]
    TooMany(&'static str),
    #[error("insufficient independent evidence")]
    InsufficientEvidence,
    #[error("candidate target is unavailable")]
    Unavailable,
    #[error("candidate cannot change authority-bearing policy")]
    ForbiddenAuthorityChange,
}

impl RefinementCandidateV1 {
    pub fn new(input: RefinementCandidateInput) -> Result<Self, RefinementError> {
        let proposed_content = input.proposed_content;
        let candidate = Self {
            schema_version: CONTRACT_VERSION,
            id: input.id,
            revision: 1,
            kind: input.kind,
            target: input.target,
            scope: input.scope,
            pattern_key: input.pattern_key,
            title: input.title,
            rationale: input.rationale,
            proposed_content,
            source_task_ids: input.source_task_ids,
            evidence: input.evidence,
            conflicts: Vec::new(),
            confidence: 0,
            content_hash: String::new(),
            policy_snapshot_hash: input.policy_snapshot_hash,
            idempotency_key: input.idempotency_key,
            status: CandidateStatus::Proposed,
        };
        candidate.validate()
    }

    pub fn validate(mut self) -> Result<Self, RefinementError> {
        if self.schema_version != CONTRACT_VERSION {
            return Err(RefinementError::UnsupportedVersion(self.schema_version));
        }
        for (name, value) in [
            ("id", &self.id),
            ("target", &self.target),
            ("pattern_key", &self.pattern_key),
            ("title", &self.title),
            ("rationale", &self.rationale),
            ("policy_snapshot_hash", &self.policy_snapshot_hash),
            ("idempotency_key", &self.idempotency_key),
        ] {
            if value.trim().is_empty() {
                return Err(RefinementError::InvalidField(name));
            }
        }
        if self.title.chars().count() > MAX_TITLE_CHARS {
            return Err(RefinementError::TooLong {
                field: "title",
                max: MAX_TITLE_CHARS,
            });
        }
        if self.proposed_content.chars().count() > MAX_CONTENT_CHARS {
            return Err(RefinementError::TooLong {
                field: "proposed_content",
                max: MAX_CONTENT_CHARS,
            });
        }
        let lower = self.proposed_content.to_ascii_lowercase();
        if [
            "approval policy",
            "security policy",
            "credential",
            "grant",
            "unrestricted shell",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
        {
            return Err(RefinementError::ForbiddenAuthorityChange);
        }
        if self.evidence.len() > MAX_EVIDENCE_REFS {
            return Err(RefinementError::TooMany("evidence"));
        }
        if self.source_task_ids.len() > MAX_TASK_IDS {
            return Err(RefinementError::TooMany("task ids"));
        }
        if self.source_task_ids.iter().any(|id| id.trim().is_empty())
            || self
                .evidence
                .iter()
                .any(|e| e.source_id.trim().is_empty() || e.content_hash.trim().is_empty())
        {
            return Err(RefinementError::InvalidField("provenance"));
        }
        self.content_hash = content_hash(&self.proposed_content);
        Ok(self)
    }
}

/// Типизированный вход кандидата улучшения, сохраняющий все поля контракта
/// вместе и исключающий ошибки порядка аргументов.
pub struct RefinementCandidateInput {
    pub id: String,
    pub kind: CandidateKind,
    pub target: String,
    pub scope: OwnerScope,
    pub pattern_key: String,
    pub title: String,
    pub rationale: String,
    pub proposed_content: String,
    pub source_task_ids: Vec<String>,
    pub evidence: Vec<EvidenceRefV1>,
    pub policy_snapshot_hash: String,
    pub idempotency_key: String,
}

pub fn content_hash(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

pub fn admit(
    evidence: &[EvidenceRefV1],
    task_ids: &[String],
    policy: AdmissionPolicy,
) -> Result<(), RefinementError> {
    if evidence.len() < policy.min_evidence {
        return Err(RefinementError::InsufficientEvidence);
    }
    let unique = task_ids
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if unique < policy.min_independent_tasks {
        return Err(RefinementError::InsufficientEvidence);
    }
    Ok(())
}

pub fn target_available(kind: CandidateKind) -> bool {
    matches!(kind, CandidateKind::Memory)
}

pub struct RefinementService<'a> {
    store: evohime_local_storage::refinement_store::RefinementStore<'a>,
    policy: AdmissionPolicy,
}

impl<'a> RefinementService<'a> {
    pub fn new(connection: &'a rusqlite::Connection, policy: AdmissionPolicy) -> Self {
        Self {
            store: evohime_local_storage::refinement_store::RefinementStore::new(connection),
            policy,
        }
    }

    pub fn propose_memory(
        &self,
        candidate: RefinementCandidateV1,
        now_ms: i64,
    ) -> Result<evohime_local_storage::refinement_store::CandidateRow, String> {
        admit(&candidate.evidence, &candidate.source_task_ids, self.policy)
            .map_err(|error| error.to_string())?;
        if !target_available(candidate.kind) {
            return Err(RefinementError::Unavailable.to_string());
        }
        let content_json = serde_json::to_string(&candidate.proposed_content)
            .map_err(|error| error.to_string())?;
        let source_task_ids_json =
            serde_json::to_string(&candidate.source_task_ids).map_err(|error| error.to_string())?;
        let evidence_json =
            serde_json::to_string(&candidate.evidence).map_err(|error| error.to_string())?;
        let conflicts_json =
            serde_json::to_string(&candidate.conflicts).map_err(|error| error.to_string())?;
        let row = evohime_local_storage::refinement_store::CandidateRow {
            id: candidate.id,
            revision: candidate.revision,
            owner_scope: serde_json::to_string(&candidate.scope)
                .unwrap_or_default()
                .trim_matches('"')
                .to_owned(),
            kind: serde_json::to_string(&candidate.kind)
                .unwrap_or_default()
                .trim_matches('"')
                .to_owned(),
            target: candidate.target,
            status: "proposed".into(),
            pattern_key: candidate.pattern_key,
            title: candidate.title,
            rationale: candidate.rationale,
            content_hash: candidate.content_hash,
            confidence: candidate.confidence,
            evidence_count: candidate.evidence.len() as u32,
            conflict_count: candidate.conflicts.len() as u32,
            policy_snapshot_hash: candidate.policy_snapshot_hash,
            version: 0,
            idempotency_key: candidate.idempotency_key,
            error_code: None,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        self.store
            .insert_candidate(
                &row,
                &content_json,
                &source_task_ids_json,
                &evidence_json,
                &conflicts_json,
            )
            .map_err(|error| error.to_string())?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn evidence(id: &str) -> EvidenceRefV1 {
        EvidenceRefV1 {
            source_id: id.into(),
            source_kind: "task".into(),
            owner_scope: OwnerScope::Workspace,
            content_hash: "hash".into(),
            observed_at_ms: 1,
            redacted: true,
        }
    }
    #[test]
    fn one_observation_does_not_admit_global_candidate() {
        assert_eq!(
            admit(
                &[evidence("e1")],
                &["task-1".into()],
                AdmissionPolicy::default()
            ),
            Err(RefinementError::InsufficientEvidence)
        );
    }
    #[test]
    fn independent_evidence_admits_memory_candidate() {
        let c = RefinementCandidateV1::new(RefinementCandidateInput {
            id: "c1".into(),
            kind: CandidateKind::Memory,
            target: "memory".into(),
            scope: OwnerScope::Workspace,
            pattern_key: "p".into(),
            title: "title".into(),
            rationale: "why".into(),
            proposed_content: "content".into(),
            source_task_ids: vec!["t1".into(), "t2".into()],
            evidence: vec![evidence("e1"), evidence("e2")],
            policy_snapshot_hash: "policy".into(),
            idempotency_key: "idem".into(),
        })
        .unwrap();
        assert_eq!(c.content_hash, content_hash("content"));
    }
    #[test]
    fn unsupported_targets_fail_closed() {
        assert!(!target_available(CandidateKind::Skill));
        assert!(!target_available(CandidateKind::PromptRule));
    }
    #[test]
    fn authority_text_is_rejected() {
        assert_eq!(
            RefinementCandidateV1::new(RefinementCandidateInput {
                id: "c1".into(),
                kind: CandidateKind::Memory,
                target: "memory".into(),
                scope: OwnerScope::Global,
                pattern_key: "p".into(),
                title: "title".into(),
                rationale: "why".into(),
                proposed_content: "change approval policy".into(),
                source_task_ids: vec!["t1".into(), "t2".into()],
                evidence: vec![evidence("e1"), evidence("e2")],
                policy_snapshot_hash: "policy".into(),
                idempotency_key: "idem".into(),
            })
            .unwrap_err(),
            RefinementError::ForbiddenAuthorityChange
        );
    }

    #[test]
    fn service_persists_only_repeated_independent_observations() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        evohime_local_storage::refinement_store::install_schema(&connection).unwrap();
        let candidate = RefinementCandidateV1::new(RefinementCandidateInput {
            id: "c1".into(),
            kind: CandidateKind::Memory,
            target: "memory".into(),
            scope: OwnerScope::Workspace,
            pattern_key: "p".into(),
            title: "title".into(),
            rationale: "why".into(),
            proposed_content: "content".into(),
            source_task_ids: vec!["t1".into(), "t2".into()],
            evidence: vec![evidence("e1"), evidence("e2")],
            policy_snapshot_hash: "policy".into(),
            idempotency_key: "idem".into(),
        })
        .unwrap();
        let row = RefinementService::new(&connection, AdmissionPolicy::default())
            .propose_memory(candidate, 2)
            .unwrap();
        assert_eq!(row.status, "proposed");
        assert_eq!(
            evohime_local_storage::refinement_store::RefinementStore::new(&connection)
                .list("workspace", 10)
                .unwrap()
                .len(),
            1
        );
    }
}
