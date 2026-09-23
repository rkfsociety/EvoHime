//! Core-owned, metadata-only context discovery and deterministic retrieval.
//!
//! This module deliberately does not own source content or permissions.  A
//! node is only a bounded description of an object owned by another subsystem;
//! every read is authorized against an immutable ContextViewSnapshot.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Serialized schema version for the context namespace contract.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum catalog nodes in one namespace snapshot.
pub const MAX_NODES: usize = 4096;
/// Maximum identifier size in bytes.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum stable reference size in bytes.
pub const MAX_REF_BYTES: usize = 512;
/// Maximum display-name size in bytes.
pub const MAX_DISPLAY_BYTES: usize = 256;
/// Maximum retrieval query size in bytes.
pub const MAX_QUERY_BYTES: usize = 1024;
/// Maximum projection levels advertised by one node.
pub const MAX_PROJECTIONS_PER_NODE: usize = 3;
/// Maximum content size permitted for a projection, in bytes.
pub const MAX_PROJECTION_BYTES: usize = 16 * 1024;
/// Maximum root nodes in one context view.
pub const MAX_VIEW_ROOTS: usize = 128;
/// Maximum allowed node kinds in one context view.
pub const MAX_VIEW_KINDS: usize = 16;
/// Maximum references pinned into one context view.
pub const MAX_VIEW_REFS: usize = 128;
/// Maximum explicit exclusions in one context view.
pub const MAX_EXCLUSIONS: usize = 256;
/// Maximum nodes visited during one retrieval.
pub const MAX_VISITED: usize = 4096;
/// Maximum traversal depth for one retrieval.
pub const MAX_DEPTH: u8 = 32;
/// Maximum entries in one retrieval trace collection.
pub const MAX_TRACE_ITEMS: usize = 4096;
/// Maximum token budget representable for one retrieval.
pub const MAX_TOKEN_BUDGET: u32 = 8 * 1024 * 1024;

/// Category of a metadata-only context namespace node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Namespace grouping other context nodes.
    Namespace,
    /// Scope grouping related memory records.
    MemoryScope,
    /// Individual memory record.
    MemoryRecord,
    /// Knowledge collection grouping sources and chunks.
    KnowledgeCollection,
    /// Acquired knowledge source.
    KnowledgeSource,
    /// Extracted knowledge unit from a source.
    KnowledgeChunk,
    /// Installed skill or capability description.
    Skill,
    /// Workspace instruction or guidance record.
    ProjectGuidance,
    /// Project-owned artifact.
    ProjectArtifact,
    /// Grounded research artifact.
    ResearchArtifact,
    /// Root of a repository tree.
    RepositoryRoot,
    /// Logical area within a repository.
    RepositoryArea,
    /// Runtime-owned resource descriptor.
    RuntimeResource,
    /// Node type supplied by a registered extension.
    CustomRegistered,
}

/// Detail level requested for a context projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionLevel {
    /// Minimal structural identity without a substantive summary.
    Abstract,
    /// Bounded overview suitable for broad discovery.
    Overview,
    /// More specific content requiring separate authorization.
    Detail,
}

/// Maximum sensitivity level allowed by a context view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    /// Intended for unrestricted disclosure.
    Public,
    /// Intended for internal use.
    Internal,
    /// Private to the relevant user or workspace scope.
    Private,
    /// Secret material that must not enter ordinary context projections.
    Secret,
}

/// Trust or instruction status assigned to a context node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustClass {
    /// Source is authoritative for the represented fact or instruction.
    Authoritative,
    /// Source has been reviewed by an authorized actor.
    Reviewed,
    /// Source content is untrusted input.
    Untrusted,
    /// Source contains instructions and must follow instruction-specific policy.
    Instruction,
}

/// Health state of the index used to discover a context node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexHealth {
    /// Index is synchronized and available.
    Healthy,
    /// Index updates are behind the source state.
    Lagging,
    /// Only part of the source is represented in the index.
    PartiallyIndexed,
    /// Index contents predate the current source revision.
    Stale,
    /// Index data failed an integrity check.
    Corrupt,
    /// Index is currently unavailable.
    Unavailable,
    /// A rebuild is required before reliable lookup.
    RebuildRequired,
}

/// Freshness declaration for a source or projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// Content matches the current source revision.
    Current,
    /// Content is known to predate the current source revision.
    Stale,
    /// Freshness could not be determined.
    Unknown,
}

/// Mechanism that produced a projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedBy {
    /// Produced by deterministic Core logic.
    Deterministic,
    /// Produced by the source's native representation.
    SourceNative,
    /// Generated or summarized by a model.
    ModelGenerated,
    /// Imported content that was reviewed.
    ImportedReviewed,
}

/// Action recorded for a node during context retrieval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisitAction {
    /// Node was evaluated as a possible retrieval candidate.
    Considered,
    /// Traversal followed a relationship from this node.
    Expanded,
    /// One or more projections from the node were included.
    Selected,
    /// Node was skipped to respect a bound or score threshold.
    Pruned,
    /// Node was excluded by authorization, scope, or validation.
    Rejected,
}

/// Stable explanation category recorded for a retrieval decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    /// Query text semantically matched the node.
    SemanticMatch,
    /// Node is closely associated with the active scope.
    ScopeAffinity,
    /// Current content received a freshness preference.
    FreshnessBoost,
    /// Node source has high authority for the requested information.
    HighAuthority,
    /// A parent namespace was selected.
    ParentSelected,
    /// Candidate score did not meet the selection threshold.
    BelowThreshold,
    /// Candidate was omitted to stay within a resource budget.
    BudgetPruned,
    /// Candidate projection was stale.
    StaleProjection,
    /// Candidate was removed by authorization filtering.
    PermissionFiltered,
    /// Candidate exceeded the allowed sensitivity ceiling.
    SensitivityFiltered,
    /// Candidate duplicated content already represented in the context.
    DuplicateCoverage,
    /// Candidate was explicitly named by the caller.
    ExplicitReference,
    /// Required index data was unavailable.
    IndexUnavailable,
    /// Traversal reached its maximum depth.
    DepthExceeded,
}

/// Overall status returned by context retrieval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureStatus {
    /// Retrieval completed successfully.
    Ok,
    /// No relevant authorized context was found.
    NoRelevantContext,
    /// Index updates are behind the source state.
    IndexLagging,
    /// Required index data is unavailable.
    IndexUnavailable,
    /// Index integrity validation failed.
    IndexCorrupt,
    /// A selected projection is stale.
    ProjectionStale,
    /// Projection generation failed.
    ProjectionGenerationFailed,
    /// Optional reranking could not be performed.
    RerankerUnavailable,
    /// Reranking exceeded its allocated resource budget.
    RerankBudgetExceeded,
    /// Retrieval exceeded its overall resource budget.
    RetrievalBudgetExceeded,
    /// Authorization filtering removed every candidate.
    PermissionFilteredAll,
    /// Required scope information is unavailable.
    ScopeUnavailable,
    /// Retrieval returned only partial context coverage.
    PartialCoverage,
}

/// Origin class used for provenance-aware context handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceClass {
    /// Authored directly by the user.
    UserAuthored,
    /// Authored by an assistant or model.
    AssistantAuthored,
    /// Retrieved from persistent memory.
    RetrievedMemory,
    /// Retrieved from a knowledge source.
    RetrievedKnowledge,
    /// Loaded from project guidance.
    ProjectGuidance,
    /// Loaded from a skill instruction source.
    SkillInstruction,
    /// Produced as a tool result.
    ToolResult,
    /// Supplied by the system runtime.
    SystemInstruction,
}

/// Bounded descriptor for a context object owned by another subsystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextNodeDescriptor {
    /// Serialized descriptor schema version.
    pub schema_version: u32,
    /// Stable namespace node identifier.
    pub node_id: String,
    /// Stable locator for the object owned by another subsystem.
    pub stable_ref: String,
    /// Category of the described context object.
    pub kind: NodeKind,
    /// Bounded user-facing node name.
    pub display_name: String,
    /// Optional stable reference of the logical parent node.
    pub logical_parent_ref: Option<String>,
    /// Stable reference to the authoritative source object.
    pub source_ref: String,
    /// Revision of the source represented by the descriptor.
    pub source_revision: u64,
    /// Optional source content digest.
    pub source_content_hash: Option<String>,
    /// Scope reference used for authorization checks.
    pub scope_ref: String,
    /// Sensitivity classification used by the view ceiling.
    pub sensitivity: Sensitivity,
    /// Trust classification used by retrieval policy.
    pub trust_class: TrustClass,
    /// Projection levels available for this node.
    pub projection_capabilities: Vec<ProjectionLevel>,
    /// Health state of the discovery index for this node.
    pub index_health: IndexHealth,
    /// Freshness state of the source projection.
    pub freshness: Freshness,
    /// Digest of the canonical node descriptor.
    pub content_hash: String,
}

/// Bounded view of a context node at one requested detail level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextProjection {
    /// Node described by this projection.
    pub node_id: String,
    /// Detail level represented by the projection.
    pub level: ProjectionLevel,
    /// Source revision used to generate the projection.
    pub source_revision: u64,
    /// Source digest used to generate the projection, when available.
    pub source_content_hash: Option<String>,
    /// Registered profile defining the projection method.
    pub projection_profile_ref: String,
    /// Revision of the projection profile.
    pub projection_revision: u64,
    /// Parser revision used to process the source.
    pub parser_revision: u64,
    /// Policy revision applied when generating the projection.
    pub policy_revision: u64,
    /// Reference to the stored projection content.
    pub content_ref: String,
    /// Bounded summary content, when included inline.
    pub summary: Option<String>,
    /// Estimated token cost of this projection.
    pub token_estimate: u32,
    /// Whether the projection content was truncated.
    pub truncated: bool,
    /// Freshness state of the projection.
    pub freshness: Freshness,
    /// Mechanism that generated the projection.
    pub generated_by: GeneratedBy,
    /// Digest of the projection content or canonical metadata.
    pub content_hash: String,
    /// Provenance category of the projected content.
    pub provenance_class: ProvenanceClass,
}

/// Immutable authorization and scope snapshot for one run's context discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextViewSnapshot {
    /// Serialized context view schema version.
    pub schema_version: u32,
    /// Stable identifier for this view snapshot.
    pub id: String,
    /// Monotonically increasing view revision.
    pub revision: u64,
    /// Run that owns this view snapshot.
    pub run_id: String,
    /// Root nodes from which retrieval may traverse.
    pub root_nodes: Vec<String>,
    /// Node kinds permitted in this view.
    pub allowed_kinds: Vec<NodeKind>,
    /// Stable references explicitly excluded from the view.
    pub excluded_refs: Vec<String>,
    /// Maximum sensitivity permitted for selected content.
    pub max_sensitivity: Sensitivity,
    /// Source views contributing to this authorization snapshot.
    pub source_view_refs: Vec<String>,
    /// Digest of the policy used to create the snapshot.
    pub policy_hash: String,
    /// Index generations pinned for consistent retrieval.
    pub index_snapshot_refs: Vec<String>,
    /// Snapshot creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Digest of the canonical view snapshot.
    pub content_hash: String,
}

/// Deterministic bounds and query references for one retrieval operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRetrievalPlan {
    /// Query used to rank eligible context nodes.
    pub query: String,
    /// Candidate roots considered by the traversal.
    pub candidate_roots: Vec<String>,
    /// Maximum graph traversal depth.
    pub max_depth: u8,
    /// Maximum number of nodes visited.
    pub max_nodes_visited: usize,
    /// Overall budget for overview projections, in tokens.
    pub max_projection_tokens: u32,
    /// Separate budget for detail projections, in tokens.
    pub max_detail_tokens: u32,
    /// Per-projection-level item budgets.
    pub per_level_budgets: BTreeMap<ProjectionLevel, usize>,
    /// Whether tie-breaking and traversal order must be deterministic.
    pub deterministic: bool,
    /// View revision this plan was built against.
    pub view_revision: u64,
    /// Index generation reference used by the plan.
    pub index_snapshot_ref: String,
    /// Policy revision used when creating the plan.
    pub policy_revision: u64,
    /// Explicit stable references that should receive priority.
    pub explicit_refs: Vec<String>,
    /// Digest of the canonical retrieval plan.
    pub content_hash: String,
}

/// One auditable decision made while traversing a context namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextNodeVisit {
    /// Identifier of the visited namespace node.
    pub node_id: String,
    /// Projection level selected for the node, if any.
    pub level: Option<ProjectionLevel>,
    /// Action taken for this node.
    pub action: VisitAction,
    /// Stable reason explaining the action.
    pub reason_code: ReasonCode,
    /// Retrieval score assigned to the candidate.
    pub score: i32,
    /// Estimated token cost charged to the candidate.
    pub token_cost: u32,
}

/// Bounded audit record for one complete context retrieval attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextRetrievalTrace {
    /// Serialized trace schema version.
    pub schema_version: u32,
    /// Stable trace identifier.
    pub id: String,
    /// Run that owns this retrieval trace.
    pub run_id: String,
    /// Context view snapshot used for authorization.
    pub view_ref: String,
    /// Revision of the view snapshot.
    pub view_revision: u64,
    /// Retrieval policy reference used for this attempt.
    pub retrieval_policy_ref: String,
    /// Retrieval plan reference used for this attempt.
    pub query_plan_ref: String,
    /// Ordered node decisions made during traversal.
    pub visited_nodes: Vec<ContextNodeVisit>,
    /// Projection references selected for the result.
    pub selected_projections: Vec<String>,
    /// Candidate references rejected during retrieval.
    pub rejected_candidates: Vec<String>,
    /// Token cost attributed to each selected node.
    pub token_contributions: BTreeMap<String, u32>,
    /// Fallback stages attempted in order.
    pub fallback_path: Vec<String>,
    /// Aggregate index health observed during retrieval.
    pub index_health: IndexHealth,
    /// Final ordered references supplied as context.
    pub final_context_refs: Vec<String>,
    /// Overall retrieval outcome.
    pub status: FailureStatus,
    /// Trace creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Digest of the canonical trace record.
    pub content_hash: String,
}

/// Selected projections and the audit trace for a retrieval attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrievalResult {
    /// Overall retrieval outcome.
    pub status: FailureStatus,
    /// Projections selected for the final context.
    pub selected: Vec<ContextProjection>,
    /// Bounded trace of candidate decisions.
    pub trace: ContextRetrievalTrace,
}

/// Idempotent command envelope for namespace-owned mutations.
#[derive(Debug, Clone)]
pub struct NamespaceCommand {
    /// Registered operation name.
    pub operation: String,
    /// Namespace targeted by the operation.
    pub namespace_id: String,
    /// Serialized bounded operation payload.
    pub payload: Vec<u8>,
    /// Expected namespace revision for optimistic concurrency.
    pub expected_revision: u64,
    /// Key used to deduplicate retries of the same command.
    pub idempotency_key: String,
}

/// Validation, authorization, freshness, or resource failure in namespace operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespaceError {
    /// Input violated a named contract rule.
    Invalid(&'static str),
    /// Serialized data uses an unsupported schema version.
    UnsupportedVersion(u32),
    /// A unique node or reference was duplicated.
    Duplicate,
    /// Namespace relationships contain a cycle.
    Cycle,
    /// The requested access is not authorized.
    Unauthorized,
    /// Requested content exceeds the view's sensitivity ceiling.
    SensitivityDenied,
    /// Snapshot or projection is stale.
    Stale,
    /// Referenced namespace object does not exist.
    NotFound,
    /// Retrieval exceeded a configured resource budget.
    BudgetExceeded,
    /// A detail projection resolver is unavailable.
    DetailResolverUnavailable,
}

impl std::fmt::Display for NamespaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid(value) => value,
            Self::UnsupportedVersion(_) => "unsupported_version",
            Self::Duplicate => "duplicate",
            Self::Cycle => "cycle",
            Self::Unauthorized => "unauthorized",
            Self::SensitivityDenied => "sensitivity_denied",
            Self::Stale => "stale_projection",
            Self::NotFound => "not_found",
            Self::BudgetExceeded => "budget_exceeded",
            Self::DetailResolverUnavailable => "detail_resolver_unavailable",
        })
    }
}
impl std::error::Error for NamespaceError {}

fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid_id(value: &str) -> bool {
    bounded(value, MAX_ID_BYTES)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}

fn valid_revision(value: u64) -> bool {
    value > 0 && value <= i64::MAX as u64
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, NamespaceError> {
    let bytes = serde_json::to_vec(value).map_err(|_| NamespaceError::Invalid("serialization"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Supplies a source-authorized detail projection for a context node.
pub trait ContextDetailResolver {
    /// Resolves a detail-level projection from its authoritative source.
    fn resolve_detail(
        &self,
        node: &ContextNodeDescriptor,
        projection: &ContextProjection,
    ) -> Result<ContextProjection, NamespaceError>;
}

/// Resolver placeholder that always reports detail content as unavailable.
pub struct UnavailableDetailResolver;

impl ContextDetailResolver for UnavailableDetailResolver {
    fn resolve_detail(
        &self,
        _node: &ContextNodeDescriptor,
        _projection: &ContextProjection,
    ) -> Result<ContextProjection, NamespaceError> {
        Err(NamespaceError::DetailResolverUnavailable)
    }
}

/// Resolves a detail projection and validates the returned projection against its node.
pub fn resolve_detail_with<R: ContextDetailResolver>(
    resolver: &R,
    node: &ContextNodeDescriptor,
    projection: &ContextProjection,
) -> Result<ContextProjection, NamespaceError> {
    if projection.level != ProjectionLevel::Detail {
        return Err(NamespaceError::Invalid("detail_projection_required"));
    }
    validate_projection(projection, node)?;
    let resolved = resolver.resolve_detail(node, projection)?;
    validate_projection(&resolved, node)?;
    Ok(resolved)
}

fn sensitivity_allowed(actual: Sensitivity, ceiling: Sensitivity) -> bool {
    actual <= ceiling
}

/// Validates a node descriptor's identifiers, bounds, capabilities, and digest.
pub fn validate_node(node: &ContextNodeDescriptor) -> Result<(), NamespaceError> {
    if node.schema_version != SCHEMA_VERSION {
        return Err(NamespaceError::UnsupportedVersion(node.schema_version));
    }
    if !valid_id(&node.node_id)
        || !bounded(&node.stable_ref, MAX_REF_BYTES)
        || !bounded(&node.display_name, MAX_DISPLAY_BYTES)
        || !bounded(&node.source_ref, MAX_REF_BYTES)
        || !valid_id(&node.scope_ref)
        || !valid_revision(node.source_revision)
        || node.projection_capabilities.is_empty()
        || node.projection_capabilities.len() > MAX_PROJECTIONS_PER_NODE
        || !is_sha256(&node.content_hash)
    {
        return Err(NamespaceError::Invalid("node"));
    }
    if node
        .logical_parent_ref
        .as_deref()
        .is_some_and(|value| !bounded(value, MAX_REF_BYTES))
        || node
            .source_content_hash
            .as_deref()
            .is_some_and(|value| !bounded(value, MAX_REF_BYTES))
    {
        return Err(NamespaceError::Invalid("parent"));
    }
    let levels: BTreeSet<_> = node.projection_capabilities.iter().copied().collect();
    if levels.len() != node.projection_capabilities.len() {
        return Err(NamespaceError::Duplicate);
    }
    Ok(())
}

/// Validates projection identity, source revision, level, size, freshness, and digest.
pub fn validate_projection(
    projection: &ContextProjection,
    node: &ContextNodeDescriptor,
) -> Result<(), NamespaceError> {
    validate_node(node)?;
    if projection.node_id != node.node_id
        || !node.projection_capabilities.contains(&projection.level)
        || projection.source_revision != node.source_revision
        || projection.source_content_hash != node.source_content_hash
        || !valid_revision(projection.source_revision)
        || !valid_id(&projection.projection_profile_ref)
        || !valid_revision(projection.projection_revision)
        || !valid_revision(projection.parser_revision)
        || !valid_revision(projection.policy_revision)
        || !bounded(&projection.content_ref, MAX_REF_BYTES)
        || projection
            .source_content_hash
            .as_deref()
            .is_some_and(|value| !bounded(value, MAX_REF_BYTES))
        || projection.token_estimate == 0
        || projection.token_estimate > MAX_PROJECTION_BYTES as u32
        || !is_sha256(&projection.content_hash)
    {
        return Err(NamespaceError::Invalid("projection"));
    }
    if projection.summary.as_deref().is_some_and(|value| {
        value.is_empty()
            || value.len() > MAX_PROJECTION_BYTES
            || value.chars().any(char::is_control)
    }) {
        return Err(NamespaceError::Invalid("projection_summary"));
    }
    if projection.level == ProjectionLevel::Detail && projection.summary.is_some() {
        return Err(NamespaceError::Invalid("detail_must_use_source_ref"));
    }
    if projection.level == ProjectionLevel::Detail
        && !projection.content_ref.starts_with("source:")
        && !projection.content_ref.starts_with("artifact:")
    {
        return Err(NamespaceError::Invalid("detail_source_locator_required"));
    }
    if projection.freshness == Freshness::Stale {
        return Err(NamespaceError::Stale);
    }
    Ok(())
}

/// Validates a view's roots, allowed kinds, exclusions, and policy references.
pub fn validate_view(view: &ContextViewSnapshot) -> Result<(), NamespaceError> {
    if view.schema_version != SCHEMA_VERSION {
        return Err(NamespaceError::UnsupportedVersion(view.schema_version));
    }
    if !valid_id(&view.id)
        || !valid_revision(view.revision)
        || !valid_id(&view.run_id)
        || view.root_nodes.is_empty()
        || view.root_nodes.len() > MAX_VIEW_ROOTS
        || view.allowed_kinds.is_empty()
        || view.allowed_kinds.len() > MAX_VIEW_KINDS
        || view.excluded_refs.len() > MAX_EXCLUSIONS
        || view.source_view_refs.len() > MAX_VIEW_REFS
        || view.index_snapshot_refs.len() > MAX_VIEW_REFS
        || !bounded(&view.policy_hash, MAX_REF_BYTES)
        || !is_sha256(&view.content_hash)
    {
        return Err(NamespaceError::Invalid("view"));
    }
    if view.root_nodes.iter().any(|value| !valid_id(value))
        || view
            .excluded_refs
            .iter()
            .any(|value| !bounded(value, MAX_REF_BYTES))
        || view
            .source_view_refs
            .iter()
            .any(|value| !bounded(value, MAX_REF_BYTES))
        || view
            .index_snapshot_refs
            .iter()
            .any(|value| !bounded(value, MAX_REF_BYTES))
    {
        return Err(NamespaceError::Invalid("view_refs"));
    }
    Ok(())
}

/// Validates query size, traversal and token budgets, references, and plan digest.
pub fn validate_plan(plan: &ContextRetrievalPlan) -> Result<(), NamespaceError> {
    if !bounded(&plan.query, MAX_QUERY_BYTES)
        || plan.max_depth == 0
        || plan.max_depth > MAX_DEPTH
        || plan.max_nodes_visited == 0
        || plan.max_nodes_visited > MAX_VISITED
        || plan.max_projection_tokens == 0
        || plan.max_detail_tokens == 0
        || plan.max_projection_tokens > MAX_TOKEN_BUDGET
        || plan.max_detail_tokens > MAX_TOKEN_BUDGET
        || !valid_revision(plan.view_revision)
        || !bounded(&plan.index_snapshot_ref, MAX_REF_BYTES)
        || !valid_revision(plan.policy_revision)
        || !is_sha256(&plan.content_hash)
        || plan.candidate_roots.len() > MAX_VIEW_ROOTS
        || plan.explicit_refs.len() > MAX_EXCLUSIONS
        || plan
            .candidate_roots
            .iter()
            .chain(plan.explicit_refs.iter())
            .any(|value| !bounded(value, MAX_REF_BYTES))
    {
        return Err(NamespaceError::Invalid("retrieval_plan"));
    }
    for level in [
        ProjectionLevel::Abstract,
        ProjectionLevel::Overview,
        ProjectionLevel::Detail,
    ] {
        let budget = plan.per_level_budgets.get(&level).copied().unwrap_or(0);
        if budget == 0 || budget > MAX_VISITED {
            return Err(NamespaceError::Invalid("level_budget"));
        }
    }
    Ok(())
}

/// Validates bounded trace collections, references, visits, and content digest.
pub fn validate_trace(trace: &ContextRetrievalTrace) -> Result<(), NamespaceError> {
    if trace.schema_version != SCHEMA_VERSION
        || !valid_id(&trace.id)
        || !valid_id(&trace.run_id)
        || !bounded(&trace.view_ref, MAX_REF_BYTES)
        || !valid_revision(trace.view_revision)
        || !bounded(&trace.retrieval_policy_ref, MAX_REF_BYTES)
        || !bounded(&trace.query_plan_ref, MAX_REF_BYTES)
        || trace.visited_nodes.len() > MAX_VISITED
        || trace.selected_projections.len() > MAX_TRACE_ITEMS
        || trace.rejected_candidates.len() > MAX_TRACE_ITEMS
        || trace.final_context_refs.len() > MAX_TRACE_ITEMS
        || trace
            .selected_projections
            .iter()
            .chain(trace.rejected_candidates.iter())
            .chain(trace.final_context_refs.iter())
            .any(|value| !bounded(value, MAX_REF_BYTES))
        || !is_sha256(&trace.content_hash)
    {
        return Err(NamespaceError::Invalid("trace"));
    }
    if trace
        .visited_nodes
        .iter()
        .any(|visit| !valid_id(&visit.node_id) || visit.token_cost > MAX_PROJECTION_BYTES as u32)
    {
        return Err(NamespaceError::Invalid("trace_visit"));
    }
    if trace.fallback_path.len() > MAX_TRACE_ITEMS
        || trace.token_contributions.len() > MAX_TRACE_ITEMS
        || trace
            .token_contributions
            .keys()
            .any(|value| !bounded(value, MAX_REF_BYTES))
        || trace
            .token_contributions
            .values()
            .any(|value| *value > MAX_PROJECTION_BYTES as u32)
        || trace
            .fallback_path
            .iter()
            .any(|value| !bounded(value, MAX_REF_BYTES))
    {
        return Err(NamespaceError::Invalid("trace_budget"));
    }
    Ok(())
}

/// Validates a catalog of nodes and rejects duplicate identities or invalid parent relationships.
pub fn validate_catalog(nodes: &[ContextNodeDescriptor]) -> Result<(), NamespaceError> {
    if nodes.is_empty() || nodes.len() > MAX_NODES {
        return Err(NamespaceError::Invalid("catalog"));
    }
    let ids: BTreeSet<_> = nodes.iter().map(|node| node.node_id.as_str()).collect();
    let stable_refs: BTreeSet<_> = nodes.iter().map(|node| node.stable_ref.as_str()).collect();
    if ids.len() != nodes.len()
        || stable_refs.len() != nodes.len()
        || nodes.iter().any(|node| validate_node(node).is_err())
    {
        return Err(NamespaceError::Duplicate);
    }
    for node in nodes {
        let mut seen = BTreeSet::new();
        let mut parent = node.logical_parent_ref.as_deref();
        while let Some(value) = parent {
            if !seen.insert(value) {
                return Err(NamespaceError::Cycle);
            }
            parent = nodes
                .iter()
                .find(|candidate| candidate.stable_ref == value || candidate.node_id == value)
                .and_then(|candidate| candidate.logical_parent_ref.as_deref());
        }
    }
    Ok(())
}

impl ContextViewSnapshot {
    /// Authorizes a node against this view's roots, exclusions, kinds, and sensitivity ceiling.
    pub fn authorize_node(
        &self,
        node: &ContextNodeDescriptor,
        nodes: &[ContextNodeDescriptor],
    ) -> Result<(), NamespaceError> {
        validate_view(self)?;
        validate_catalog(nodes)?;
        if !self.allowed_kinds.contains(&node.kind) {
            return Err(NamespaceError::Unauthorized);
        }
        if !sensitivity_allowed(node.sensitivity, self.max_sensitivity) {
            return Err(NamespaceError::SensitivityDenied);
        }
        if self
            .excluded_refs
            .iter()
            .any(|value| value == &node.stable_ref || value == &node.node_id)
        {
            return Err(NamespaceError::Unauthorized);
        }
        let roots: BTreeSet<_> = self.root_nodes.iter().map(String::as_str).collect();
        let mut current = Some(node);
        let mut depth = 0;
        while let Some(candidate) = current {
            if roots.contains(candidate.node_id.as_str())
                || roots.contains(candidate.stable_ref.as_str())
            {
                return Ok(());
            }
            current = candidate
                .logical_parent_ref
                .as_deref()
                .and_then(|ref_value| {
                    nodes.iter().find(|candidate| {
                        candidate.node_id == ref_value || candidate.stable_ref == ref_value
                    })
                });
            depth += 1;
            if depth > MAX_DEPTH {
                break;
            }
        }
        Err(NamespaceError::Unauthorized)
    }
}

impl ContextRetrievalPlan {
    /// Computes the canonical plan digest with `content_hash` cleared.
    pub fn canonical_hash_without_self(&self) -> Result<String, NamespaceError> {
        let mut copy = self.clone();
        copy.content_hash.clear();
        canonical_hash(&copy)
    }
}

impl ContextViewSnapshot {
    /// Computes the canonical view digest with `content_hash` cleared.
    pub fn canonical_hash_without_self(&self) -> Result<String, NamespaceError> {
        let mut copy = self.clone();
        copy.content_hash.clear();
        canonical_hash(&copy)
    }
}

impl ContextRetrievalTrace {
    /// Computes the canonical trace digest with `content_hash` cleared.
    pub fn canonical_hash_without_self(&self) -> Result<String, NamespaceError> {
        let mut copy = self.clone();
        copy.content_hash.clear();
        canonical_hash(&copy)
    }
}

fn score(node: &ContextNodeDescriptor, query: &str, explicit: bool) -> i32 {
    if explicit {
        return 10_000;
    }
    let query = query.to_ascii_lowercase();
    let display = node.display_name.to_ascii_lowercase();
    let reference = node.stable_ref.to_ascii_lowercase();
    let mut result = if display == query {
        800
    } else if display.starts_with(&query) {
        500
    } else if display.contains(&query) || reference.contains(&query) {
        250
    } else {
        0
    };
    if node.trust_class == TrustClass::Authoritative {
        result += 20;
    }
    if node.freshness == Freshness::Current {
        result += 10;
    }
    result
}

fn level_for(node: &ContextNodeDescriptor, explicit: bool) -> ProjectionLevel {
    if explicit
        && node
            .projection_capabilities
            .contains(&ProjectionLevel::Detail)
    {
        ProjectionLevel::Detail
    } else if node
        .projection_capabilities
        .contains(&ProjectionLevel::Overview)
    {
        ProjectionLevel::Overview
    } else {
        ProjectionLevel::Abstract
    }
}

fn within_candidate_roots(
    node: &ContextNodeDescriptor,
    roots: &[String],
    nodes: &[ContextNodeDescriptor],
    max_depth: u8,
) -> bool {
    if roots.is_empty() {
        return true;
    }
    let wanted: BTreeSet<&str> = roots.iter().map(String::as_str).collect();
    let mut current = Some(node);
    let mut depth = 0_u8;
    while let Some(candidate) = current {
        if wanted.contains(candidate.node_id.as_str())
            || wanted.contains(candidate.stable_ref.as_str())
        {
            return true;
        }
        if depth >= max_depth {
            return false;
        }
        current = candidate
            .logical_parent_ref
            .as_deref()
            .and_then(|parent_ref| {
                nodes
                    .iter()
                    .find(|parent| parent.node_id == parent_ref || parent.stable_ref == parent_ref)
            });
        depth = depth.saturating_add(1);
    }
    false
}

/// Selects authorized projections deterministically within the plan's depth and token budgets.
///
/// Every candidate is checked against the immutable view and source revisions;
/// the result includes a bounded trace of selection, rejection, and pruning decisions.
pub fn retrieve(
    nodes: &[ContextNodeDescriptor],
    projections: &[ContextProjection],
    view: &ContextViewSnapshot,
    plan: &ContextRetrievalPlan,
    trace_id: String,
    created_at_ms: i64,
) -> Result<RetrievalResult, NamespaceError> {
    validate_catalog(nodes)?;
    validate_view(view)?;
    validate_plan(plan)?;
    if plan.view_revision != view.revision {
        return Err(NamespaceError::Invalid("view_revision"));
    }
    let map: HashMap<&str, &ContextNodeDescriptor> = nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect();
    let mut candidates = Vec::new();
    let mut trace_visits = Vec::new();
    for node in nodes {
        let explicit = plan
            .explicit_refs
            .iter()
            .any(|value| value == &node.node_id || value == &node.stable_ref);
        if view.authorize_node(node, nodes).is_err() {
            trace_visits.push(ContextNodeVisit {
                node_id: node.node_id.clone(),
                level: None,
                action: VisitAction::Rejected,
                reason_code: ReasonCode::PermissionFiltered,
                score: 0,
                token_cost: 0,
            });
            continue;
        }
        if !within_candidate_roots(node, &plan.candidate_roots, nodes, MAX_DEPTH) {
            continue;
        }
        let mut depth = 0_u8;
        let mut current = node.logical_parent_ref.as_deref();
        while let Some(parent_ref) = current {
            depth = depth.saturating_add(1);
            if depth > plan.max_depth {
                break;
            }
            current = nodes
                .iter()
                .find(|candidate| {
                    candidate.node_id == parent_ref || candidate.stable_ref == parent_ref
                })
                .and_then(|candidate| candidate.logical_parent_ref.as_deref());
        }
        if depth > plan.max_depth {
            trace_visits.push(ContextNodeVisit {
                node_id: node.node_id.clone(),
                level: None,
                action: VisitAction::Pruned,
                reason_code: ReasonCode::DepthExceeded,
                score: 0,
                token_cost: 0,
            });
            continue;
        }
        let level = level_for(node, explicit);
        let score = score(node, &plan.query, explicit);
        candidates.push((score, node.node_id.clone(), level, explicit));
        trace_visits.push(ContextNodeVisit {
            node_id: node.node_id.clone(),
            level: Some(level),
            action: VisitAction::Considered,
            reason_code: if explicit {
                ReasonCode::ExplicitReference
            } else {
                ReasonCode::SemanticMatch
            },
            score,
            token_cost: 0,
        });
    }
    candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let mut used = BTreeMap::<ProjectionLevel, usize>::new();
    let mut tokens = 0_u32;
    let mut selected = Vec::new();
    let mut selected_refs = Vec::new();
    let mut rejected = Vec::new();
    let mut contributions = BTreeMap::new();
    let mut health = IndexHealth::Healthy;
    for node in nodes {
        if view.authorize_node(node, nodes).is_ok() && node.index_health != IndexHealth::Healthy {
            health = node.index_health;
            break;
        }
    }
    for (score, node_id, level, explicit) in candidates.into_iter().take(plan.max_nodes_visited) {
        let Some(node) = map.get(node_id.as_str()) else {
            continue;
        };
        let Some(projection) = projections
            .iter()
            .find(|value| value.node_id == node_id && value.level == level)
        else {
            rejected.push(node_id.clone());
            trace_visits.push(ContextNodeVisit {
                node_id,
                level: Some(level),
                action: VisitAction::Rejected,
                reason_code: ReasonCode::StaleProjection,
                score,
                token_cost: 0,
            });
            continue;
        };
        if validate_projection(projection, node).is_err() {
            rejected.push(node_id.clone());
            continue;
        }
        let quota = plan.per_level_budgets[&level];
        if used.get(&level).copied().unwrap_or(0) >= quota
            || tokens.saturating_add(projection.token_estimate) > plan.max_projection_tokens
            || (level == ProjectionLevel::Detail
                && projection.token_estimate > plan.max_detail_tokens)
        {
            rejected.push(node_id.clone());
            trace_visits.push(ContextNodeVisit {
                node_id,
                level: Some(level),
                action: VisitAction::Pruned,
                reason_code: ReasonCode::BudgetPruned,
                score,
                token_cost: projection.token_estimate,
            });
            continue;
        }
        used.entry(level)
            .and_modify(|value| *value += 1)
            .or_insert(1);
        tokens = tokens.saturating_add(projection.token_estimate);
        contributions.insert(
            format!("{}:{:?}", node.scope_ref, level),
            projection.token_estimate,
        );
        selected_refs.push(projection.content_ref.clone());
        selected.push(projection.clone());
        trace_visits.push(ContextNodeVisit {
            node_id,
            level: Some(level),
            action: if explicit {
                VisitAction::Selected
            } else {
                VisitAction::Expanded
            },
            reason_code: if explicit {
                ReasonCode::ExplicitReference
            } else {
                ReasonCode::SemanticMatch
            },
            score,
            token_cost: projection.token_estimate,
        });
    }
    let status = if selected.is_empty() {
        if !trace_visits.is_empty()
            && trace_visits
                .iter()
                .all(|visit| visit.reason_code == ReasonCode::PermissionFiltered)
        {
            FailureStatus::PermissionFilteredAll
        } else if health != IndexHealth::Healthy {
            match health {
                IndexHealth::Lagging | IndexHealth::PartiallyIndexed | IndexHealth::Stale => {
                    FailureStatus::IndexLagging
                }
                IndexHealth::Corrupt | IndexHealth::RebuildRequired => FailureStatus::IndexCorrupt,
                _ => FailureStatus::IndexUnavailable,
            }
        } else if !rejected.is_empty() {
            FailureStatus::ProjectionStale
        } else {
            FailureStatus::NoRelevantContext
        }
    } else if !rejected.is_empty() || health != IndexHealth::Healthy {
        FailureStatus::PartialCoverage
    } else {
        FailureStatus::Ok
    };
    let fallback_path = if health == IndexHealth::Healthy {
        Vec::new()
    } else {
        vec!["direct_source_ref".into(), "degraded_catalog".into()]
    };
    let mut trace = ContextRetrievalTrace {
        schema_version: SCHEMA_VERSION,
        id: trace_id,
        run_id: view.run_id.clone(),
        view_ref: view.id.clone(),
        view_revision: view.revision,
        retrieval_policy_ref: format!("policy:{}", plan.policy_revision),
        query_plan_ref: plan.index_snapshot_ref.clone(),
        visited_nodes: trace_visits.into_iter().take(MAX_TRACE_ITEMS).collect(),
        selected_projections: selected
            .iter()
            .map(|value| format!("{}:{:?}", value.node_id, value.level))
            .collect(),
        rejected_candidates: rejected.into_iter().take(MAX_TRACE_ITEMS).collect(),
        token_contributions: contributions,
        fallback_path,
        index_health: health,
        final_context_refs: selected_refs,
        status,
        created_at_ms,
        content_hash: String::new(),
    };
    trace.content_hash = trace.canonical_hash_without_self()?;
    Ok(RetrievalResult {
        status,
        selected,
        trace,
    })
}

/// Creates a child view whose authority is constrained by its parent snapshot.
pub fn child_view(
    parent: &ContextViewSnapshot,
    child_id: String,
    run_id: String,
    roots: Vec<String>,
    max_sensitivity: Sensitivity,
    excluded_refs: Vec<String>,
    created_at_ms: i64,
) -> Result<ContextViewSnapshot, NamespaceError> {
    validate_view(parent)?;
    if max_sensitivity > parent.max_sensitivity
        || roots.is_empty()
        || roots.iter().any(|root| !parent.root_nodes.contains(root))
        || excluded_refs
            .iter()
            .any(|value| !bounded(value, MAX_REF_BYTES))
    {
        return Err(NamespaceError::Unauthorized);
    }
    let revision = parent
        .revision
        .checked_add(1)
        .filter(|value| *value <= i64::MAX as u64)
        .ok_or(NamespaceError::BudgetExceeded)?;
    let mut view = ContextViewSnapshot {
        schema_version: SCHEMA_VERSION,
        id: child_id,
        revision,
        run_id,
        root_nodes: roots,
        allowed_kinds: parent.allowed_kinds.clone(),
        excluded_refs,
        max_sensitivity,
        source_view_refs: parent.source_view_refs.clone(),
        policy_hash: parent.policy_hash.clone(),
        index_snapshot_refs: parent.index_snapshot_refs.clone(),
        created_at_ms,
        content_hash: String::new(),
    };
    view.content_hash = view.canonical_hash_without_self()?;
    Ok(view)
}

fn read_view(payload: &[u8]) -> Result<ContextViewSnapshot, crate::StorageError> {
    if payload.is_empty() {
        return Err(crate::StorageError::InvalidInput(
            "context_view_required".into(),
        ));
    }
    let view: ContextViewSnapshot = serde_json::from_slice(payload)
        .map_err(|_| crate::StorageError::InvalidInput("invalid_context_view".into()))?;
    validate_view(&view).map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
    Ok(view)
}

fn read_nodes(
    connection: &rusqlite::Connection,
) -> Result<Vec<ContextNodeDescriptor>, crate::StorageError> {
    use evohime_local_storage::context_namespace_store as store;
    store::list_nodes(connection, MAX_NODES)
        .map_err(crate::StorageError::from)
        .map(|values| {
            values
                .into_iter()
                .filter_map(|value| serde_json::from_slice::<ContextNodeDescriptor>(&value).ok())
                .collect()
        })
}

fn authorize_target(
    view: &ContextViewSnapshot,
    target_id: &str,
    nodes: &[ContextNodeDescriptor],
) -> Result<(), crate::StorageError> {
    let target = nodes
        .iter()
        .find(|node| node.node_id == target_id || node.stable_ref == target_id)
        .ok_or_else(|| crate::StorageError::InvalidInput("context_node_not_found".into()))?;
    view.authorize_node(target, nodes)
        .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))
}

fn is_direct_child(
    node: &ContextNodeDescriptor,
    parent_id: &str,
    nodes: &[ContextNodeDescriptor],
) -> bool {
    let Some(parent) = nodes
        .iter()
        .find(|candidate| candidate.node_id == parent_id || candidate.stable_ref == parent_id)
    else {
        return false;
    };
    node.logical_parent_ref.as_deref() == Some(parent.node_id.as_str())
        || node.logical_parent_ref.as_deref() == Some(parent.stable_ref.as_str())
}

impl crate::EventJournal {
    /// Applies an idempotent namespace command after validating its target and payload bounds.
    pub async fn context_namespace_command(
        &self,
        command: NamespaceCommand,
    ) -> Result<Vec<u8>, crate::StorageError> {
        use evohime_local_storage::context_namespace_store as store;
        if !valid_id(&command.namespace_id)
            || command.payload.len() > store::MAX_RECORD_BYTES
            || command.operation.len() > 64
            || command.expected_revision > i64::MAX as u64
        {
            return Err(crate::StorageError::InvalidInput(
                "invalid_context_namespace_command".into(),
            ));
        }
        let command_hash = format!(
            "{:x}",
            Sha256::digest(
                [
                    command.operation.as_bytes(),
                    b"\0",
                    command.namespace_id.as_bytes(),
                    b"\0",
                    &command.expected_revision.to_le_bytes(),
                    b"\0",
                    &command.payload,
                ]
                .concat()
            )
        );
        let mutation = matches!(
            command.operation.as_str(),
            "register_node" | "register_projection" | "save_view" | "record_trace"
        );
        let database = self.database.lock().await;
        let connection = database.connection();
        if mutation {
            if command.idempotency_key.is_empty() || !valid_id(&command.idempotency_key) {
                return Err(crate::StorageError::InvalidInput(
                    "context_namespace_idempotency_required".into(),
                ));
            }
            if let Some((stored, response)) = store::load_idempotency(
                connection,
                &command.namespace_id,
                &command.idempotency_key,
            )? {
                if stored != command_hash {
                    return Err(crate::StorageError::InvalidInput(
                        "context_namespace_idempotency_conflict".into(),
                    ));
                }
                return Ok(response);
            }
        }
        let response = match command.operation.as_str() {
            "register_node" => {
                let node: ContextNodeDescriptor = serde_json::from_slice(&command.payload)?;
                validate_node(&node)
                    .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
                if command.expected_revision != 0 {
                    let current = store::get_node(connection, &node.node_id)?
                        .and_then(|json| {
                            serde_json::from_slice::<ContextNodeDescriptor>(&json).ok()
                        })
                        .map(|value| value.source_revision)
                        .unwrap_or(0);
                    if current != command.expected_revision {
                        return Err(crate::StorageError::VersionConflict {
                            entity: "context_node",
                            id: node.node_id,
                            expected: command.expected_revision as i64,
                            current: current as i64,
                        });
                    }
                }
                let json = serde_json::to_vec(&node)?;
                let saved = store::put_node(
                    connection,
                    &node.node_id,
                    node.source_revision,
                    &json,
                    crate::task_memory::now_millis() as i64,
                )?;
                serde_json::to_vec(
                    &serde_json::json!({"status": if saved {"registered"} else {"duplicate"}, "node_id": node.node_id, "revision": node.source_revision, "content_hash": node.content_hash}),
                )?
            }
            "register_projection" => {
                let projection: ContextProjection = serde_json::from_slice(&command.payload)?;
                let node_json =
                    store::get_node(connection, &projection.node_id)?.ok_or_else(|| {
                        crate::StorageError::InvalidInput("context_node_not_found".into())
                    })?;
                let node: ContextNodeDescriptor = serde_json::from_slice(&node_json)?;
                validate_projection(&projection, &node)
                    .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
                let json = serde_json::to_vec(&projection)?;
                let saved = store::put_projection(
                    connection,
                    &projection.node_id,
                    &format!("{:?}", projection.level),
                    projection.source_revision,
                    &json,
                    crate::task_memory::now_millis() as i64,
                )?;
                serde_json::to_vec(
                    &serde_json::json!({"status": if saved {"registered"} else {"duplicate"}, "node_id": projection.node_id, "level": projection.level}),
                )?
            }
            "save_view" => {
                let view: ContextViewSnapshot = serde_json::from_slice(&command.payload)?;
                validate_view(&view)
                    .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
                if command.expected_revision != 0 {
                    let current = store::get_view(connection, &view.id)?
                        .and_then(|json| serde_json::from_slice::<ContextViewSnapshot>(&json).ok())
                        .map(|value| value.revision)
                        .unwrap_or(0);
                    if current != command.expected_revision {
                        return Err(crate::StorageError::VersionConflict {
                            entity: "context_view",
                            id: view.id,
                            expected: command.expected_revision as i64,
                            current: current as i64,
                        });
                    }
                }
                let json = serde_json::to_vec(&view)?;
                let saved = store::put_view(
                    connection,
                    &view.id,
                    view.revision,
                    &json,
                    crate::task_memory::now_millis() as i64,
                )?;
                serde_json::to_vec(
                    &serde_json::json!({"status": if saved {"saved"} else {"duplicate"}, "view_id": view.id, "revision": view.revision}),
                )?
            }
            "list_children" => {
                let view = read_view(&command.payload)?;
                let all_nodes = read_nodes(connection)?;
                authorize_target(&view, &command.namespace_id, &all_nodes)?;
                let nodes = all_nodes
                    .iter()
                    .filter(|node| {
                        is_direct_child(node, &command.namespace_id, &all_nodes)
                            && view.authorize_node(node, &all_nodes).is_ok()
                    })
                    .take(256)
                    .cloned()
                    .collect::<Vec<_>>();
                serde_json::to_vec(&serde_json::json!({"status":"ok","nodes":nodes}))?
            }
            "get_abstract" | "get_overview" => {
                let view = read_view(&command.payload)?;
                let nodes = read_nodes(connection)?;
                authorize_target(&view, &command.namespace_id, &nodes)?;
                let level = match command.operation.as_str() {
                    "get_abstract" => ProjectionLevel::Abstract,
                    "get_overview" => ProjectionLevel::Overview,
                    _ => ProjectionLevel::Detail,
                };
                let values = store::list_projections(connection, 12288)?;
                let projection = values
                    .into_iter()
                    .filter_map(|value| serde_json::from_slice::<ContextProjection>(&value).ok())
                    .find(|value| value.node_id == command.namespace_id && value.level == level)
                    .ok_or_else(|| {
                        crate::StorageError::InvalidInput("context_projection_not_found".into())
                    })?;
                serde_json::to_vec(&projection)?
            }
            "resolve_detail" => {
                let view = read_view(&command.payload)?;
                let nodes = read_nodes(connection)?;
                authorize_target(&view, &command.namespace_id, &nodes)?;
                let values = store::list_projections(connection, 12288)?;
                let projection = values
                    .into_iter()
                    .filter_map(|value| serde_json::from_slice::<ContextProjection>(&value).ok())
                    .find(|value| {
                        value.node_id == command.namespace_id
                            && value.level == ProjectionLevel::Detail
                    })
                    .ok_or_else(|| {
                        crate::StorageError::InvalidInput("context_projection_not_found".into())
                    })?;
                serde_json::to_vec(
                    &serde_json::json!({"status": FailureStatus::ProjectionGenerationFailed, "error_code": NamespaceError::DetailResolverUnavailable.to_string(), "node_id": projection.node_id, "content_ref": projection.content_ref, "projection_level": projection.level}),
                )?
            }
            "find_descendants" | "search_within" => {
                #[derive(Deserialize)]
                struct Input {
                    view: ContextViewSnapshot,
                    query: Option<String>,
                }
                let input: Input = serde_json::from_slice(&command.payload).map_err(|_| {
                    crate::StorageError::InvalidInput("context_view_required".into())
                })?;
                let view = input.view;
                validate_view(&view)
                    .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
                let query = input.query.unwrap_or_default().to_ascii_lowercase();
                let all_nodes = read_nodes(connection)?;
                authorize_target(&view, &command.namespace_id, &all_nodes)?;
                let nodes = all_nodes
                    .iter()
                    .filter(|node| {
                        view.authorize_node(node, &all_nodes).is_ok()
                            && if command.operation == "find_descendants" {
                                is_direct_child(node, &command.namespace_id, &all_nodes)
                            } else {
                                within_candidate_roots(
                                    node,
                                    std::slice::from_ref(&command.namespace_id),
                                    &all_nodes,
                                    MAX_DEPTH,
                                ) && (query.is_empty()
                                    || node.display_name.to_ascii_lowercase().contains(&query)
                                    || node.stable_ref.to_ascii_lowercase().contains(&query))
                            }
                    })
                    .take(256)
                    .cloned()
                    .collect::<Vec<_>>();
                serde_json::to_vec(&serde_json::json!({"status":"ok","nodes":nodes}))?
            }
            "retrieve" => {
                #[derive(Deserialize)]
                struct Input {
                    view: ContextViewSnapshot,
                    plan: ContextRetrievalPlan,
                    trace_id: String,
                }
                let input: Input = serde_json::from_slice(&command.payload)?;
                let nodes = store::list_nodes(connection, 4096)?
                    .into_iter()
                    .filter_map(|value| {
                        serde_json::from_slice::<ContextNodeDescriptor>(&value).ok()
                    })
                    .collect::<Vec<_>>();
                let projections = store::list_projections(connection, 12288)?
                    .into_iter()
                    .filter_map(|value| serde_json::from_slice::<ContextProjection>(&value).ok())
                    .collect::<Vec<_>>();
                let result = retrieve(
                    &nodes,
                    &projections,
                    &input.view,
                    &input.plan,
                    input.trace_id,
                    crate::task_memory::now_millis() as i64,
                )
                .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
                let trace_json = serde_json::to_vec(&result.trace)?;
                store::put_trace(
                    connection,
                    &result.trace.id,
                    &result.trace.run_id,
                    &trace_json,
                    crate::task_memory::now_millis() as i64,
                )?;
                serde_json::to_vec(&result)?
            }
            "explain_selection" => {
                let view = read_view(&command.payload)?;
                let trace_json =
                    store::get_trace(connection, &command.namespace_id)?.ok_or_else(|| {
                        crate::StorageError::InvalidInput("context_trace_not_found".into())
                    })?;
                let trace: ContextRetrievalTrace = serde_json::from_slice(&trace_json)?;
                if trace.view_ref != view.id || trace.view_revision != view.revision {
                    return Err(crate::StorageError::InvalidInput(
                        "context_trace_view_mismatch".into(),
                    ));
                }
                trace_json
            }
            "record_trace" => {
                let trace: ContextRetrievalTrace = serde_json::from_slice(&command.payload)?;
                if trace.id != command.namespace_id {
                    return Err(crate::StorageError::InvalidInput(
                        "context_trace_id_mismatch".into(),
                    ));
                }
                validate_trace(&trace)
                    .map_err(|error| crate::StorageError::InvalidInput(error.to_string()))?;
                let json = serde_json::to_vec(&trace)?;
                store::put_trace(
                    connection,
                    &trace.id,
                    &trace.run_id,
                    &json,
                    crate::task_memory::now_millis() as i64,
                )?;
                json
            }
            _ => {
                return Err(crate::StorageError::InvalidInput(
                    "unsupported_context_namespace_operation".into(),
                ))
            }
        };
        if mutation {
            store::save_idempotency(
                connection,
                &command.namespace_id,
                &command.idempotency_key,
                &command_hash,
                &response,
                crate::task_memory::now_millis() as i64,
            )?;
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, parent: Option<&str>, kind: NodeKind) -> ContextNodeDescriptor {
        ContextNodeDescriptor {
            schema_version: SCHEMA_VERSION,
            node_id: id.into(),
            stable_ref: format!("ref:{id}"),
            kind,
            display_name: id.into(),
            logical_parent_ref: parent.map(str::to_owned),
            source_ref: format!("source:{id}"),
            source_revision: 1,
            source_content_hash: Some(format!("hash-{id}")),
            scope_ref: "project:p".into(),
            sensitivity: Sensitivity::Internal,
            trust_class: TrustClass::Authoritative,
            projection_capabilities: vec![
                ProjectionLevel::Abstract,
                ProjectionLevel::Overview,
                ProjectionLevel::Detail,
            ],
            index_health: IndexHealth::Healthy,
            freshness: Freshness::Current,
            content_hash: "a".repeat(64),
        }
    }
    fn view() -> ContextViewSnapshot {
        ContextViewSnapshot {
            schema_version: SCHEMA_VERSION,
            id: "view".into(),
            revision: 1,
            run_id: "run".into(),
            root_nodes: vec!["root".into()],
            allowed_kinds: vec![NodeKind::Namespace, NodeKind::MemoryRecord],
            excluded_refs: Vec::new(),
            max_sensitivity: Sensitivity::Internal,
            source_view_refs: vec!["memory:view".into()],
            policy_hash: "policy".into(),
            index_snapshot_refs: vec!["index:1".into()],
            created_at_ms: 1,
            content_hash: "b".repeat(64),
        }
    }
    fn projection(id: &str, level: ProjectionLevel) -> ContextProjection {
        ContextProjection {
            node_id: id.into(),
            level,
            source_revision: 1,
            source_content_hash: Some(format!("hash-{id}")),
            projection_profile_ref: "profile:v1".into(),
            projection_revision: 1,
            parser_revision: 1,
            policy_revision: 1,
            content_ref: if level == ProjectionLevel::Detail {
                format!("source:detail:{id}")
            } else {
                format!("projection:{id}")
            },
            summary: if level == ProjectionLevel::Detail {
                None
            } else {
                Some("bounded summary".into())
            },
            token_estimate: 4,
            truncated: false,
            freshness: Freshness::Current,
            generated_by: GeneratedBy::Deterministic,
            content_hash: "c".repeat(64),
            provenance_class: ProvenanceClass::RetrievedMemory,
        }
    }
    fn plan() -> ContextRetrievalPlan {
        let mut budgets = BTreeMap::new();
        budgets.insert(ProjectionLevel::Abstract, 4);
        budgets.insert(ProjectionLevel::Overview, 4);
        budgets.insert(ProjectionLevel::Detail, 4);
        ContextRetrievalPlan {
            query: "child".into(),
            candidate_roots: Vec::new(),
            max_depth: 4,
            max_nodes_visited: 32,
            max_projection_tokens: 100,
            max_detail_tokens: 50,
            per_level_budgets: budgets,
            deterministic: true,
            view_revision: 1,
            index_snapshot_ref: "index:1".into(),
            policy_revision: 1,
            explicit_refs: Vec::new(),
            content_hash: "d".repeat(64),
        }
    }

    #[test]
    fn catalog_rejects_cycles() {
        let a = node("a", Some("ref:b"), NodeKind::Namespace);
        let b = node("b", Some("ref:a"), NodeKind::MemoryRecord);
        assert_eq!(validate_catalog(&[a, b]), Err(NamespaceError::Cycle));
    }
    #[test]
    fn guessed_path_does_not_bypass_view() {
        let n = node("hidden", None, NodeKind::MemoryRecord);
        assert_eq!(
            view().authorize_node(&n, std::slice::from_ref(&n)),
            Err(NamespaceError::Unauthorized)
        );
    }
    #[test]
    fn sensitivity_is_enforced() {
        let mut n = node("root", None, NodeKind::Namespace);
        n.sensitivity = Sensitivity::Secret;
        assert_eq!(
            view().authorize_node(&n, &[n.clone()]),
            Err(NamespaceError::SensitivityDenied)
        );
    }
    #[test]
    fn child_view_can_only_narrow() {
        let child = child_view(
            &view(),
            "child-view".into(),
            "run-2".into(),
            vec!["root".into()],
            Sensitivity::Public,
            Vec::new(),
            2,
        )
        .unwrap();
        assert_eq!(child.max_sensitivity, Sensitivity::Public);
        assert!(child_view(
            &view(),
            "bad".into(),
            "run".into(),
            vec!["root".into()],
            Sensitivity::Secret,
            Vec::new(),
            2
        )
        .is_err());
    }
    #[test]
    fn stale_projection_is_not_accepted() {
        let mut p = projection("root", ProjectionLevel::Abstract);
        p.freshness = Freshness::Stale;
        assert_eq!(
            validate_projection(&p, &node("root", None, NodeKind::Namespace)),
            Err(NamespaceError::Stale)
        );
    }
    #[test]
    fn detail_has_no_inline_summary() {
        let mut p = projection("root", ProjectionLevel::Detail);
        p.summary = Some("raw".into());
        assert_eq!(
            validate_projection(&p, &node("root", None, NodeKind::Namespace)),
            Err(NamespaceError::Invalid("detail_must_use_source_ref"))
        );
    }
    #[test]
    fn explicit_ref_has_priority() {
        let n = node("root", None, NodeKind::Namespace);
        let mut p = plan();
        p.explicit_refs = vec!["ref:root".into()];
        let result = retrieve(
            &[n],
            &[projection("root", ProjectionLevel::Detail)],
            &view(),
            &p,
            "trace".into(),
            3,
        )
        .unwrap();
        assert_eq!(result.status, FailureStatus::Ok);
        assert_eq!(result.selected[0].level, ProjectionLevel::Detail);
    }
    #[test]
    fn deterministic_ties_use_node_id() {
        let a = node("a", None, NodeKind::Namespace);
        let b = node("b", None, NodeKind::Namespace);
        let mut p = plan();
        p.query = "no-match".into();
        let r1 = retrieve(
            &[b.clone(), a.clone()],
            &[
                projection("a", ProjectionLevel::Overview),
                projection("b", ProjectionLevel::Overview),
            ],
            &view(),
            &p,
            "t1".into(),
            3,
        )
        .unwrap();
        let r2 = retrieve(
            &[a, b],
            &[
                projection("a", ProjectionLevel::Overview),
                projection("b", ProjectionLevel::Overview),
            ],
            &view(),
            &p,
            "t2".into(),
            3,
        )
        .unwrap();
        assert_eq!(
            r1.selected.iter().map(|v| &v.node_id).collect::<Vec<_>>(),
            r2.selected.iter().map(|v| &v.node_id).collect::<Vec<_>>()
        );
    }
    #[test]
    fn missing_projection_is_visible_failure() {
        let n = node("root", None, NodeKind::Namespace);
        let result = retrieve(&[n], &[], &view(), &plan(), "trace".into(), 3).unwrap();
        assert_eq!(result.status, FailureStatus::ProjectionStale);
        assert!(!result.trace.rejected_candidates.is_empty());
    }
    #[test]
    fn dense_detail_cannot_consume_overview_quota() {
        let root = node("root", None, NodeKind::Namespace);
        let child = node("child", Some("ref:root"), NodeKind::MemoryRecord);
        let mut p = plan();
        p.explicit_refs = vec!["ref:child".into()];
        let result = retrieve(
            &[root, child],
            &[
                projection("root", ProjectionLevel::Overview),
                projection("child", ProjectionLevel::Detail),
            ],
            &view(),
            &p,
            "trace".into(),
            3,
        )
        .unwrap();
        assert!(result.selected.iter().any(|value| value.node_id == "child"));
    }
    #[test]
    fn retrieved_memory_is_not_user_evidence() {
        assert_ne!(
            ProvenanceClass::RetrievedMemory,
            ProvenanceClass::UserAuthored
        );
    }
    #[test]
    fn invalid_node_hash_is_rejected() {
        let mut n = node("root", None, NodeKind::Namespace);
        n.content_hash = "not-a-hash".into();
        assert_eq!(validate_node(&n), Err(NamespaceError::Invalid("node")));
    }
    #[test]
    fn invalid_plan_hash_is_rejected() {
        let mut p = plan();
        p.content_hash = "not-a-hash".into();
        assert_eq!(
            validate_plan(&p),
            Err(NamespaceError::Invalid("retrieval_plan"))
        );
    }
    #[test]
    fn detail_resolver_absence_is_typed() {
        let n = node("root", None, NodeKind::Namespace);
        let p = projection("root", ProjectionLevel::Detail);
        assert_eq!(
            resolve_detail_with(&UnavailableDetailResolver, &n, &p),
            Err(NamespaceError::DetailResolverUnavailable)
        );
    }
    #[test]
    fn detail_requires_authoritative_locator() {
        let n = node("root", None, NodeKind::Namespace);
        let mut p = projection("root", ProjectionLevel::Detail);
        p.content_ref = "projection:root".into();
        assert_eq!(
            validate_projection(&p, &n),
            Err(NamespaceError::Invalid("detail_source_locator_required"))
        );
    }
    #[test]
    fn child_view_rejects_new_root() {
        assert_eq!(
            child_view(
                &view(),
                "child-view".into(),
                "run-2".into(),
                vec!["outside".into()],
                Sensitivity::Internal,
                Vec::new(),
                2
            ),
            Err(NamespaceError::Unauthorized)
        );
    }
    #[test]
    fn candidate_root_reaches_its_descendants_only() {
        let root = node("root", None, NodeKind::Namespace);
        let child = node("child", Some("ref:root"), NodeKind::MemoryRecord);
        let sibling = node("sibling", None, NodeKind::MemoryRecord);
        let mut p = plan();
        p.candidate_roots = vec!["root".into()];
        p.query = "child".into();
        let result = retrieve(
            &[root, child, sibling],
            &[projection("child", ProjectionLevel::Overview)],
            &view(),
            &p,
            "trace".into(),
            3,
        )
        .unwrap();
        assert!(result.selected.iter().any(|value| value.node_id == "child"));
        assert!(!result
            .selected
            .iter()
            .any(|value| value.node_id == "sibling"));
    }
    #[test]
    fn depth_bound_prunes_deep_nodes() {
        let root = node("root", None, NodeKind::Namespace);
        let child = node("child", Some("ref:root"), NodeKind::MemoryRecord);
        let grandchild = node("grandchild", Some("ref:child"), NodeKind::MemoryRecord);
        let mut p = plan();
        p.max_depth = 1;
        p.candidate_roots = vec!["root".into()];
        let result = retrieve(
            &[root, child, grandchild],
            &[
                projection("child", ProjectionLevel::Overview),
                projection("grandchild", ProjectionLevel::Overview),
            ],
            &view(),
            &p,
            "trace".into(),
            3,
        )
        .unwrap();
        assert!(result
            .trace
            .visited_nodes
            .iter()
            .any(|visit| visit.node_id == "grandchild"
                && visit.reason_code == ReasonCode::DepthExceeded));
    }
    #[test]
    fn stale_catalog_health_is_not_clean_success() {
        let mut n = node("root", None, NodeKind::Namespace);
        n.index_health = IndexHealth::Lagging;
        let result = retrieve(
            &[n],
            &[projection("root", ProjectionLevel::Overview)],
            &view(),
            &plan(),
            "trace".into(),
            3,
        )
        .unwrap();
        assert_eq!(result.status, FailureStatus::PartialCoverage);
    }
    #[test]
    fn trace_hash_is_deterministic_for_same_metadata() {
        let root = node("root", None, NodeKind::Namespace);
        let result = retrieve(
            &[root],
            &[projection("root", ProjectionLevel::Overview)],
            &view(),
            &plan(),
            "trace".into(),
            3,
        )
        .unwrap();
        assert_eq!(
            result.trace.content_hash,
            result.trace.canonical_hash_without_self().unwrap()
        );
    }
    #[test]
    fn failure_statuses_serialize_as_bounded_machine_values() {
        for value in [
            FailureStatus::NoRelevantContext,
            FailureStatus::IndexUnavailable,
            FailureStatus::ProjectionGenerationFailed,
            FailureStatus::RerankerUnavailable,
            FailureStatus::RetrievalBudgetExceeded,
            FailureStatus::PartialCoverage,
        ] {
            assert!(serde_json::to_string(&value).unwrap().len() < 64);
        }
    }
}
