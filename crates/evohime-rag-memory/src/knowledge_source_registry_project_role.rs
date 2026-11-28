//! Core-owned registry contract for reference knowledge, separate from Memory.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Serialized schema version accepted by knowledge registry records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum sources admitted by one knowledge policy.
pub const MAX_SOURCES: usize = 128;
/// Maximum target bindings associated with one source.
pub const MAX_BINDINGS_PER_SOURCE: usize = 32;
/// Maximum indexed chunks retained for one source.
pub const MAX_CHUNKS_PER_SOURCE: usize = 1024;
/// Maximum retrieval hits returned to a caller.
pub const MAX_HITS: usize = 128;
/// Maximum identifier length accepted by the registry.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum serialized source or collection size in bytes.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// Maximum content bytes accepted in one knowledge chunk.
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;
/// Maximum serialized source manifest size in bytes.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// Maximum evidence excerpt size in bytes.
pub const MAX_EVIDENCE_BYTES: usize = 256 * 1024;
/// Maximum token budget assigned to one knowledge view.
pub const MAX_VIEW_TOKENS: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Source media or provider category for a knowledge source.
pub enum SourceKind {
    /// Files discovered under an authorized workspace root.
    WorkspaceFiles,
    /// Plain text document.
    TextDocument,
    /// Markdown document with executable content excluded.
    MarkdownDocument,
    /// PDF document processed by a bounded text extractor.
    PdfDocument,
    /// JSON document processed as structured text.
    JsonDocument,
    /// CSV document processed as tabular text.
    CsvDocument,
    /// Previously captured web content; fetching is not implied.
    WebSnapshot,
    /// References to existing workspace artifacts.
    ArtifactCollection,
    /// Owner-registered provider reference.
    CustomProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Ingestion and availability lifecycle of a source.
pub enum SourceStatus {
    /// Collection is registered but not yet ready.
    Registered,
    /// Source is queued for ingestion.
    PendingIngestion,
    /// Source content is being indexed.
    Indexing,
    /// All selected source references are available.
    Ready,
    /// One or more source revisions require refresh.
    Stale,
    /// A replacement index is being built.
    Reindexing,
    /// Ingestion failed.
    Failed,
    /// Target cannot retrieve from the source.
    Disabled,
    /// Source was removed from active use.
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Data sensitivity level used to prevent unsafe disclosure.
pub enum Sensitivity {
    /// May be exposed to any authorized target.
    Public,
    /// May be exposed only within the owning project or organization.
    Internal,
    /// May be exposed only to a view with explicit secret sensitivity.
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Owner type that may receive a source binding.
pub enum TargetKind {
    /// Project-level target.
    Project,
    /// Agent role target.
    AgentRole,
    /// Workflow definition target.
    Workflow,
    /// Team protocol target.
    TeamProtocol,
    /// Single session target.
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Whether a target may retrieve from a bound source.
pub enum AccessMode {
    /// Target may retrieve authorized source content.
    ReadOnly,
    /// Target cannot retrieve from the source.
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Versioned metadata and provenance for a reference knowledge source.
pub struct KnowledgeSource {
    /// Serialized schema version supported by this record.
    pub schema_version: u32,
    /// Stable view or collection identifier.
    pub id: String,
    /// Monotonic source or collection revision.
    pub version: u64,
    /// Source format or provider category.
    pub kind: SourceKind,
    /// Human-readable source name.
    pub display_name: String,
    /// Owner-controlled reference to the original source.
    pub origin_ref: String,
    /// Optional project that owns the source.
    pub project_id: Option<String>,
    /// Stable fingerprint used to detect source changes.
    pub source_fingerprint: String,
    /// Maximum data sensitivity assigned to this content.
    pub sensitivity: Sensitivity,
    /// Trust provenance category for the source.
    pub trust_class: String,
    /// Versioned parser and ingestion profile reference.
    pub ingestion_profile_id: String,
    /// Current source ingestion and availability state.
    pub status: SourceStatus,
    /// Identity that registered the source.
    pub created_by: String,
    /// Source registration time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Optional indexing time as Unix epoch milliseconds.
    pub last_indexed_at_ms: Option<i64>,
    /// Integrity hash for canonical source, view, or chunk content.
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Authorization link from a source to an owner target.
pub struct KnowledgeBinding {
    /// Identifier of the source being bound or cited.
    pub source_id: String,
    /// Owner category receiving the source binding.
    pub target_kind: TargetKind,
    /// Identifier of the target whose bindings are considered.
    pub target_id: String,
    /// Read policy granted to the target.
    pub access_mode: AccessMode,
    /// Optional profile controlling retrieval behavior.
    pub retrieval_profile_id: Option<String>,
    /// Relative retrieval preference for this binding.
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Run-scoped, sensitivity-bounded set of authorized source identifiers.
pub struct KnowledgeView {
    /// Serialized schema version supported by this record.
    pub schema_version: u32,
    /// Stable view or collection identifier.
    pub id: String,
    /// Execution run that owns this view.
    pub run_id: String,
    /// Authorized source identifiers included in the view or collection.
    pub source_ids: Vec<String>,
    /// Highest sensitivity level permitted in the view.
    pub max_sensitivity: Sensitivity,
    /// Retrieval configuration applied to the source set.
    pub retrieval_profile: String,
    /// Optional view expiration time as Unix epoch milliseconds.
    pub expires_at_ms: Option<i64>,
    /// Integrity hash for canonical source, view, or chunk content.
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Bounded indexed excerpt with source revision and locator provenance.
pub struct KnowledgeChunk {
    /// Stable view or collection identifier.
    pub id: String,
    /// Identifier of the source being bound or cited.
    pub source_id: String,
    /// Exact source revision from which the chunk was indexed.
    pub source_revision: u64,
    /// Chunk order within the source revision.
    pub ordinal: u32,
    /// Stable source-relative location of the content.
    pub locator: String,
    /// Bounded, sanitized content excerpt stored for retrieval.
    pub content_projection: String,
    /// Integrity hash for canonical source, view, or chunk content.
    pub content_hash: String,
    /// Maximum data sensitivity assigned to this content.
    pub sensitivity: Sensitivity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Scored excerpt candidate returned within an authorized view.
pub struct KnowledgeHit {
    /// Identifier of the source being bound or cited.
    pub source_id: String,
    /// Exact source revision from which the chunk was indexed.
    pub source_revision: u64,
    /// Identifier of the matching chunk.
    pub chunk_id: String,
    /// Stable source-relative location of the content.
    pub locator: String,
    /// Bounded evidence excerpt presented to the caller.
    pub excerpt: String,
    /// Deterministic retrieval score.
    pub score: u32,
    /// Reasons the chunk matched the query.
    pub match_reasons: Vec<String>,
    /// Freshness state of the indexed source content.
    pub freshness: String,
    /// Trust provenance category for the source.
    pub trust_class: String,
}

/// A bounded, versioned set of references to existing KnowledgeSource records.
/// It never owns or duplicates source/chunk content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Versioned set of references to existing source records.
pub struct KnowledgeCollection {
    /// Serialized schema version supported by this record.
    pub schema_version: u32,
    /// Stable view or collection identifier.
    pub id: String,
    /// Monotonic source or collection revision.
    pub version: u64,
    /// Authorized source identifiers included in the view or collection.
    pub source_ids: Vec<String>,
    /// Retrieval configuration applied to the source set.
    pub retrieval_profile: String,
    /// Owner scope for this collection.
    pub scope: String,
    /// Current source ingestion and availability state.
    pub status: CollectionStatus,
    /// Integrity hash for canonical source, view, or chunk content.
    pub content_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Registration and freshness state of a knowledge collection.
pub enum CollectionStatus {
    /// Collection is registered but not yet ready.
    Registered,
    /// All selected source references are available.
    Ready,
    /// One or more source revisions require refresh.
    Stale,
    /// Target cannot retrieve from the source.
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Hard bounds controlling source, chunk, evidence, and view sizes.
pub struct KnowledgePolicy {
    /// Serialized schema version supported by this record.
    pub schema_version: u32,
    /// Maximum sources in a view or collection.
    pub max_sources: usize,
    /// Maximum owner bindings per source.
    pub max_bindings_per_source: usize,
    /// Maximum indexed chunks per source.
    pub max_chunks_per_source: usize,
    /// Maximum search hits returned.
    pub max_hits: usize,
    /// Maximum evidence excerpt size in bytes.
    pub max_evidence_bytes: usize,
    /// Maximum context tokens allocated to the view.
    pub max_view_tokens: usize,
}

/// Returns the standard hard limits for knowledge registry operations.
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
/// Validation, authorization, sensitivity, identity, or content-bound failure.
pub enum KnowledgeError {
    /// Serialized knowledge schema is unsupported.
    #[error("unsupported knowledge schema version {0}")]
    UnsupportedVersion(u32),
    /// Identifier, locator, or reference is invalid.
    #[error("invalid knowledge identifier or locator")]
    InvalidIdentifier,
    /// Source policy limit was exceeded.
    #[error("knowledge source limit exceeded")]
    SourceLimit,
    /// Per-source binding limit was exceeded.
    #[error("knowledge binding limit exceeded")]
    BindingLimit,
    /// Chunk, manifest, evidence, or view size limit was exceeded.
    #[error("knowledge chunk or evidence limit exceeded")]
    ContentLimit,
    /// Duplicate source or collection identity was found.
    #[error("duplicate knowledge identity")]
    DuplicateIdentity,
    /// Source is not ready for retrieval.
    #[error("knowledge source is not retrieval-ready")]
    NotReady,
    /// The target is not authorized to retrieve this source or hit.
    #[error("knowledge source is not authorized in the view")]
    Unauthorized,
    /// A source exceeds the view sensitivity ceiling.
    #[error("secret knowledge cannot enter a lower-sensitivity view")]
    SensitivityViolation,
    /// A file reference escapes its allowlisted root.
    #[error("knowledge path escapes its allowlisted root")]
    PathEscape,
    /// Active content, macros, scripts, or remote fetch are not allowed.
    #[error("document scripts/macros and embedded fetch are not allowed")]
    ExecutableContent,
    /// A knowledge record could not be serialized.
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

/// Checks configured limits against supported hard maxima.
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

/// Validates source identity, trust references, and serialized size.
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

/// Validates target and source identifiers for an authorization binding.
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

/// Checks collection revision, unique source IDs, size, and content hash.
pub fn validate_collection(
    collection: &KnowledgeCollection,
    policy: &KnowledgePolicy,
) -> Result<(), KnowledgeError> {
    validate_policy(policy)?;
    if collection.schema_version != SCHEMA_VERSION {
        return Err(KnowledgeError::UnsupportedVersion(
            collection.schema_version,
        ));
    }
    if !valid_id(&collection.id)
        || !valid_id(&collection.retrieval_profile)
        || !valid_id(&collection.scope)
        || collection.content_hash.is_empty()
        || collection.source_ids.is_empty()
        || collection.source_ids.len() > policy.max_sources
    {
        return Err(KnowledgeError::InvalidIdentifier);
    }
    let mut unique = BTreeSet::new();
    if collection
        .source_ids
        .iter()
        .any(|source_id| !valid_id(source_id) || !unique.insert(source_id))
    {
        return Err(KnowledgeError::DuplicateIdentity);
    }
    let bytes = serde_json::to_vec(collection).map_err(|_| KnowledgeError::Serialization)?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(KnowledgeError::ContentLimit);
    }
    Ok(())
}

/// Sources, bindings, target, sensitivity, and policy used to build a view.
pub struct BuildViewInput<'a> {
    /// Stable view or collection identifier.
    pub id: String,
    /// Execution run that owns this view.
    pub run_id: String,
    /// Validated source records eligible for selection.
    pub sources: &'a [KnowledgeSource],
    /// Owner-scoped source authorization records.
    pub bindings: &'a [KnowledgeBinding],
    /// Owner category receiving the source binding.
    pub target_kind: TargetKind,
    /// Identifier of the target whose bindings are considered.
    pub target_id: &'a str,
    /// Highest sensitivity level permitted in the view.
    pub max_sensitivity: Sensitivity,
    /// Retrieval configuration applied to the source set.
    pub retrieval_profile: String,
    /// Optional view expiration time as Unix epoch milliseconds.
    pub expires_at_ms: Option<i64>,
    /// Hard limits applied while validating and building the view.
    pub policy: &'a KnowledgePolicy,
}

/// Builds a sensitivity-bounded view from ready sources authorized for one target.
pub fn build_view(input: BuildViewInput<'_>) -> Result<KnowledgeView, KnowledgeError> {
    validate_policy(input.policy)?;
    let mut ids = BTreeSet::new();
    for binding in input.bindings.iter().filter(|b| {
        b.target_kind == input.target_kind
            && b.target_id == input.target_id
            && b.access_mode == AccessMode::ReadOnly
    }) {
        let source = input
            .sources
            .iter()
            .find(|s| s.id == binding.source_id)
            .ok_or(KnowledgeError::Unauthorized)?;
        validate_source(source, input.policy)?;
        if source.status != SourceStatus::Ready {
            continue;
        }
        if source.sensitivity == Sensitivity::Secret && input.max_sensitivity != Sensitivity::Secret
        {
            return Err(KnowledgeError::SensitivityViolation);
        }
        ids.insert(source.id.clone());
    }
    let source_ids: Vec<_> = ids.into_iter().take(input.policy.max_sources).collect();
    let mut view = KnowledgeView {
        schema_version: SCHEMA_VERSION,
        id: input.id,
        run_id: input.run_id,
        source_ids,
        max_sensitivity: input.max_sensitivity,
        retrieval_profile: input.retrieval_profile,
        expires_at_ms: input.expires_at_ms,
        content_hash: String::new(),
    };
    let bytes = serde_json::to_vec(&view).map_err(|_| KnowledgeError::Serialization)?;
    view.content_hash = hex::encode(Sha256::digest(bytes));
    Ok(view)
}

/// Collection and authorization context used to build a view from its sources.
pub struct BuildCollectionViewInput<'a> {
    /// Collection whose source identifiers define the candidate source set.
    pub collection: &'a KnowledgeCollection,
    /// Validated source records eligible for selection.
    pub sources: &'a [KnowledgeSource],
    /// Owner-scoped source authorization records.
    pub bindings: &'a [KnowledgeBinding],
    /// Owner category receiving the source binding.
    pub target_kind: TargetKind,
    /// Identifier of the target whose bindings are considered.
    pub target_id: &'a str,
    /// Highest sensitivity level permitted in the view.
    pub max_sensitivity: Sensitivity,
    /// Optional view expiration time as Unix epoch milliseconds.
    pub expires_at_ms: Option<i64>,
    /// Hard limits applied while validating and building the view.
    pub policy: &'a KnowledgePolicy,
}

/// Resolves a collection through current owner bindings into a run-scoped view.
pub fn build_collection_view(
    input: BuildCollectionViewInput<'_>,
) -> Result<KnowledgeView, KnowledgeError> {
    validate_collection(input.collection, input.policy)?;
    let selected = input
        .sources
        .iter()
        .filter(|source| {
            input
                .collection
                .source_ids
                .iter()
                .any(|id| id == &source.id)
        })
        .cloned()
        .collect::<Vec<_>>();
    build_view(BuildViewInput {
        id: format!("view-{}-{}", input.collection.id, input.collection.version),
        run_id: format!("collection:{}", input.collection.id),
        sources: &selected,
        bindings: input.bindings,
        target_kind: input.target_kind,
        target_id: input.target_id,
        max_sensitivity: input.max_sensitivity,
        retrieval_profile: input.collection.retrieval_profile.clone(),
        expires_at_ms: input.expires_at_ms,
        policy: input.policy,
    })
}

/// Ensures a retrieval hit belongs to the view and satisfies evidence bounds.
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
        let v = build_view(BuildViewInput {
            id: "view".into(),
            run_id: "run".into(),
            sources: &[s],
            bindings: &[b],
            target_kind: TargetKind::Project,
            target_id: "project-1",
            max_sensitivity: Sensitivity::Internal,
            retrieval_profile: "keyword".into(),
            expires_at_ms: None,
            policy: &p,
        })
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
        let v = build_view(BuildViewInput {
            id: "view".into(),
            run_id: "run".into(),
            sources: &[s],
            bindings: &[b],
            target_kind: TargetKind::Project,
            target_id: "project-1",
            max_sensitivity: Sensitivity::Internal,
            retrieval_profile: "keyword".into(),
            expires_at_ms: None,
            policy: &p,
        })
        .unwrap();
        assert!(v.source_ids.is_empty());
    }

    #[test]
    fn collection_is_bounded_unique_and_versioned() {
        let collection = KnowledgeCollection {
            schema_version: SCHEMA_VERSION,
            id: "collection-1".into(),
            version: 1,
            source_ids: vec!["source-1".into(), "source-2".into()],
            retrieval_profile: "keyword".into(),
            scope: "project:project-1".into(),
            status: CollectionStatus::Ready,
            content_hash: "hash".into(),
        };
        assert!(validate_collection(&collection, &default_policy()).is_ok());
        let mut duplicate = collection;
        duplicate.source_ids.push("source-1".into());
        assert_eq!(
            validate_collection(&duplicate, &default_policy()),
            Err(KnowledgeError::DuplicateIdentity)
        );
        let source = source(SourceStatus::Ready, Sensitivity::Internal);
        let binding = KnowledgeBinding {
            source_id: source.id.clone(),
            target_kind: TargetKind::Project,
            target_id: "project-1".into(),
            access_mode: AccessMode::ReadOnly,
            retrieval_profile_id: None,
            priority: 1,
        };
        assert_eq!(
            build_collection_view(BuildCollectionViewInput {
                collection: &KnowledgeCollection {
                    source_ids: vec![source.id.clone()],
                    ..duplicate
                },
                sources: &[source],
                bindings: &[binding],
                target_kind: TargetKind::Project,
                target_id: "project-1",
                max_sensitivity: Sensitivity::Internal,
                expires_at_ms: None,
                policy: &default_policy()
            })
            .unwrap()
            .source_ids,
            vec!["source-1"]
        );
    }
}
