//! Core-owned registry contract for reference knowledge, separate from Memory.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SOURCES: usize = 128;
pub const MAX_BINDINGS_PER_SOURCE: usize = 32;
pub const MAX_CHUNKS_PER_SOURCE: usize = 1024;
pub const MAX_HITS: usize = 128;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_EVIDENCE_BYTES: usize = 256 * 1024;
pub const MAX_VIEW_TOKENS: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    WorkspaceFiles,
    TextDocument,
    MarkdownDocument,
    PdfDocument,
    JsonDocument,
    CsvDocument,
    WebSnapshot,
    ArtifactCollection,
    CustomProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Registered,
    PendingIngestion,
    Indexing,
    Ready,
    Stale,
    Reindexing,
    Failed,
    Disabled,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Internal,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Project,
    AgentRole,
    Workflow,
    TeamProtocol,
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    ReadOnly,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSource {
    pub schema_version: u32,
    pub id: String,
    pub version: u64,
    pub kind: SourceKind,
    pub display_name: String,
    pub origin_ref: String,
    pub project_id: Option<String>,
    pub source_fingerprint: String,
    pub sensitivity: Sensitivity,
    pub trust_class: String,
    pub ingestion_profile_id: String,
    pub status: SourceStatus,
    pub created_by: String,
    pub created_at_ms: i64,
    pub last_indexed_at_ms: Option<i64>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeBinding {
    pub source_id: String,
    pub target_kind: TargetKind,
    pub target_id: String,
    pub access_mode: AccessMode,
    pub retrieval_profile_id: Option<String>,
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeView {
    pub schema_version: u32,
    pub id: String,
    pub run_id: String,
    pub source_ids: Vec<String>,
    pub max_sensitivity: Sensitivity,
    pub retrieval_profile: String,
    pub expires_at_ms: Option<i64>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeChunk {
    pub id: String,
    pub source_id: String,
    pub source_revision: u64,
    pub ordinal: u32,
    pub locator: String,
    pub content_projection: String,
    pub content_hash: String,
    pub sensitivity: Sensitivity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeHit {
    pub source_id: String,
    pub source_revision: u64,
    pub chunk_id: String,
    pub locator: String,
    pub excerpt: String,
    pub score: u32,
    pub match_reasons: Vec<String>,
    pub freshness: String,
    pub trust_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgePolicy {
    pub schema_version: u32,
    pub max_sources: usize,
    pub max_bindings_per_source: usize,
    pub max_chunks_per_source: usize,
    pub max_hits: usize,
    pub max_evidence_bytes: usize,
    pub max_view_tokens: usize,
}

pub fn default_policy() -> KnowledgePolicy {
    KnowledgePolicy {
        schema_version: SCHEMA_VERSION,
        max_sources: MAX_SOURCES,
        max_bindings_per_source: MAX_BINDINGS_PER_SOURCE,
        max_chunks_per_source: MAX_CHUNKS_PER_SOURCE,
        max_hits: MAX_HITS,
        max_evidence_bytes: MAX_EVIDENCE_BYTES,
        max_view_tokens: MAX_VIEW_TOKENS,
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum KnowledgeError {
    #[error("unsupported knowledge schema version {0}")]
    UnsupportedVersion(u32),
    #[error("invalid knowledge identifier or locator")]
    InvalidIdentifier,
    #[error("knowledge source limit exceeded")]
    SourceLimit,
    #[error("knowledge binding limit exceeded")]
    BindingLimit,
    #[error("knowledge chunk or evidence limit exceeded")]
    ContentLimit,
    #[error("duplicate knowledge identity")]
    DuplicateIdentity,
    #[error("knowledge source is not retrieval-ready")]
    NotReady,
    #[error("knowledge source is not authorized in the view")]
    Unauthorized,
    #[error("secret knowledge cannot enter a lower-sensitivity view")]
    SensitivityViolation,
    #[error("knowledge path escapes its allowlisted root")]
    PathEscape,
    #[error("document scripts/macros and embedded fetch are not allowed")]
    ExecutableContent,
    #[error("knowledge serialization failed")]
    Serialization,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:/".contains(&b))
}

pub fn validate_policy(policy: &KnowledgePolicy) -> Result<(), KnowledgeError> {
    if policy.schema_version != SCHEMA_VERSION
        || policy.max_sources == 0
        || policy.max_sources > MAX_SOURCES
        || policy.max_bindings_per_source == 0
        || policy.max_bindings_per_source > MAX_BINDINGS_PER_SOURCE
        || policy.max_chunks_per_source == 0
        || policy.max_chunks_per_source > MAX_CHUNKS_PER_SOURCE
        || policy.max_hits == 0
        || policy.max_hits > MAX_HITS
        || policy.max_evidence_bytes == 0
        || policy.max_evidence_bytes > MAX_EVIDENCE_BYTES
        || policy.max_view_tokens == 0
        || policy.max_view_tokens > MAX_VIEW_TOKENS
    {
        return Err(KnowledgeError::ContentLimit);
    }
    Ok(())
}

pub fn validate_source(
    source: &KnowledgeSource,
    policy: &KnowledgePolicy,
) -> Result<(), KnowledgeError> {
    validate_policy(policy)?;
    if source.schema_version != SCHEMA_VERSION {
        return Err(KnowledgeError::UnsupportedVersion(source.schema_version));
    }
    if !valid_id(&source.id)
        || !valid_id(&source.display_name)
        || !valid_id(&source.origin_ref)
        || !valid_id(&source.ingestion_profile_id)
        || !valid_id(&source.created_by)
        || source.source_fingerprint.is_empty()
        || source.content_hash.is_empty()
    {
        return Err(KnowledgeError::InvalidIdentifier);
    }
    if matches!(source.kind, SourceKind::CustomProvider) && source.origin_ref.starts_with("http") {
        return Err(KnowledgeError::ExecutableContent);
    }
    let bytes = serde_json::to_vec(source).map_err(|_| KnowledgeError::Serialization)?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(KnowledgeError::ContentLimit);
    }
    Ok(())
}

pub fn validate_binding(
    binding: &KnowledgeBinding,
    policy: &KnowledgePolicy,
) -> Result<(), KnowledgeError> {
    validate_policy(policy)?;
    if !valid_id(&binding.source_id)
        || !valid_id(&binding.target_id)
        || binding
            .retrieval_profile_id
            .as_deref()
            .is_some_and(|v| !valid_id(v))
    {
        return Err(KnowledgeError::InvalidIdentifier);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn build_view(
    id: String,
    run_id: String,
    sources: &[KnowledgeSource],
    bindings: &[KnowledgeBinding],
    target_kind: TargetKind,
    target_id: &str,
    max_sensitivity: Sensitivity,
    retrieval_profile: String,
    expires_at_ms: Option<i64>,
    policy: &KnowledgePolicy,
) -> Result<KnowledgeView, KnowledgeError> {
    validate_policy(policy)?;
    let mut ids = BTreeSet::new();
    for binding in bindings.iter().filter(|b| {
        b.target_kind == target_kind
            && b.target_id == target_id
            && b.access_mode == AccessMode::ReadOnly
    }) {
        let source = sources
            .iter()
            .find(|s| s.id == binding.source_id)
            .ok_or(KnowledgeError::Unauthorized)?;
        validate_source(source, policy)?;
        if source.status != SourceStatus::Ready {
            continue;
        }
        if source.sensitivity == Sensitivity::Secret && max_sensitivity != Sensitivity::Secret {
            return Err(KnowledgeError::SensitivityViolation);
        }
        ids.insert(source.id.clone());
    }
    let source_ids: Vec<_> = ids.into_iter().take(policy.max_sources).collect();
    let mut view = KnowledgeView {
        schema_version: SCHEMA_VERSION,
        id,
        run_id,
        source_ids,
        max_sensitivity,
        retrieval_profile,
        expires_at_ms,
        content_hash: String::new(),
    };
    let bytes = serde_json::to_vec(&view).map_err(|_| KnowledgeError::Serialization)?;
    view.content_hash = hex::encode(Sha256::digest(bytes));
    Ok(view)
}

pub fn validate_hit(
    hit: &KnowledgeHit,
    view: &KnowledgeView,
    policy: &KnowledgePolicy,
) -> Result<(), KnowledgeError> {
    validate_policy(policy)?;
    if !view.source_ids.iter().any(|id| id == &hit.source_id)
        || !valid_id(&hit.chunk_id)
        || !valid_id(&hit.locator)
        || hit.excerpt.len() > MAX_CHUNK_BYTES
        || hit.match_reasons.len() > 16
    {
        return Err(KnowledgeError::Unauthorized);
    }
    if hit.excerpt.len() > policy.max_evidence_bytes {
        return Err(KnowledgeError::ContentLimit);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(status: SourceStatus, sensitivity: Sensitivity) -> KnowledgeSource {
        KnowledgeSource {
            schema_version: SCHEMA_VERSION,
            id: "source-1".into(),
            version: 2,
            kind: SourceKind::MarkdownDocument,
            display_name: "docs".into(),
            origin_ref: "workspace/docs.md".into(),
            project_id: Some("project-1".into()),
            source_fingerprint: "fp".into(),
            sensitivity,
            trust_class: "repository_reference".into(),
            ingestion_profile_id: "plain-text-v1".into(),
            status,
            created_by: "user".into(),
            created_at_ms: 1,
            last_indexed_at_ms: Some(2),
            content_hash: "hash".into(),
        }
    }
    #[test]
    fn knowledge_is_separate_and_ready_view_is_authorized() {
        let p = default_policy();
        let s = source(SourceStatus::Ready, Sensitivity::Internal);
        let b = KnowledgeBinding {
            source_id: s.id.clone(),
            target_kind: TargetKind::Project,
            target_id: "project-1".into(),
            access_mode: AccessMode::ReadOnly,
            retrieval_profile_id: None,
            priority: 1,
        };
        let v = build_view(
            "view".into(),
            "run".into(),
            &[s],
            &[b],
            TargetKind::Project,
            "project-1",
            Sensitivity::Internal,
            "keyword".into(),
            None,
            &p,
        )
        .unwrap();
        assert_eq!(v.source_ids, vec!["source-1"]);
    }
    #[test]
    fn stale_and_secret_sources_do_not_enter_view() {
        let p = default_policy();
        let s = source(SourceStatus::Stale, Sensitivity::Secret);
        let b = KnowledgeBinding {
            source_id: s.id.clone(),
            target_kind: TargetKind::Project,
            target_id: "project-1".into(),
            access_mode: AccessMode::ReadOnly,
            retrieval_profile_id: None,
            priority: 1,
        };
        let v = build_view(
            "view".into(),
            "run".into(),
            &[s],
            &[b],
            TargetKind::Project,
            "project-1",
            Sensitivity::Internal,
            "keyword".into(),
            None,
            &p,
        )
        .unwrap();
        assert!(v.source_ids.is_empty());
    }
}
