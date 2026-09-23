//! Core-owned memory scopes, bounded views and adaptive retrieval decisions.
//!
//! A view only narrows Memory Governance.  It is not an ACL encoded in a path,
//! and retrieval never grants permission to mutate a memory or perform an
//! effect.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;

/// Current schema version for memory views and adaptive recall policies.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum logical scopes in one view.
pub const MAX_SCOPES: usize = 64;
/// Maximum root scopes exposed by one view.
pub const MAX_VIEW_SCOPES: usize = 16;
/// Maximum identifier length for scopes, views, and memory records.
pub const MAX_ID: usize = 128;
/// Maximum query length accepted for adaptive retrieval.
pub const MAX_QUERY: usize = 512;
/// Maximum results returned from a single recall decision.
pub const MAX_RESULTS: usize = 64;
/// Maximum descendant depth available to one view.
pub const MAX_DEPTH: u8 = 8;

/// Node in the logical memory-scope hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalMemoryScope {
    /// Stable scope identifier.
    pub id: String,
    /// Optional parent scope identifier.
    pub parent_id: Option<String>,
    /// Sensitivity classification of records in this scope.
    pub sensitivity: Sensitivity,
}

/// Sensitivity class attached to a logical memory scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    /// Safe for broad disclosure.
    Public,
    /// Internal workspace or product data.
    Internal,
    /// Private user data requiring restricted access.
    Private,
    /// Secret data, excluded from memory view construction.
    Secret,
}

/// Read and write rights associated with a memory view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryViewRights {
    /// Whether retrieval from visible scopes is permitted.
    pub read: bool,
    /// Whether writes to visible scopes are permitted.
    pub write: bool,
}

/// Bounded view that narrows access to a subset of the memory hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryView {
    /// View schema version.
    pub schema_version: u32,
    /// Stable view identifier.
    pub id: String,
    /// Monotonic view revision.
    pub revision: u64,
    /// Owner scope responsible for the view.
    pub owner_scope: String,
    /// Scope hierarchy considered by this view.
    pub scopes: Vec<LogicalMemoryScope>,
    /// Root scopes visible to the view.
    pub root_scope_ids: Vec<String>,
    /// Read and write rights for visible scopes.
    pub rights: MemoryViewRights,
    /// Maximum descendant depth available beneath a root scope.
    pub max_depth: u8,
    /// Maximum number of candidates returned by recall.
    pub max_results: usize,
}

/// Retrieval depth strategy requested by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallMode {
    /// Retrieve from the shallow configured depth.
    Shallow,
    /// Retrieve as deeply as the view permits.
    Deep,
    /// Choose shallow or composite depth based on query complexity.
    Auto,
}

/// Estimated structure of the retrieval query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryComplexity {
    /// Query asks for one simple fact or relation.
    Simple,
    /// Query combines multiple criteria or reasoning steps.
    Composite,
    /// Complexity was not determined.
    Unknown,
}

/// Depth and result limits used by adaptive recall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveRecallPolicy {
    /// Policy schema version.
    pub schema_version: u32,
    /// Default depth for shallow retrieval.
    pub shallow_depth: u8,
    /// Maximum configured depth for explicit deep retrieval.
    pub deep_depth: u8,
    /// Depth chosen automatically for composite queries.
    pub auto_composite_depth: u8,
    /// Maximum result count requested by policy.
    pub max_results: usize,
}

/// Resolved retrieval depth, visible scopes, scoring signals, and read barrier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallDecision {
    /// Mode requested by the caller.
    pub mode: RecallMode,
    /// Effective depth after applying policy and view bounds.
    pub effective_depth: u8,
    /// Effective result limit after applying policy and view bounds.
    pub result_limit: usize,
    /// Scope identifiers visible to the recall operation.
    pub visible_scope_ids: Vec<String>,
    /// Score components available to candidate ranking.
    pub score_components: Vec<String>,
    /// Memory read-barrier generation captured for freshness checks.
    pub read_barrier_generation: u64,
    /// Stable reason explaining the chosen recall depth.
    pub reason_code: String,
}

/// Candidate memory record and independent ranking signals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallCandidate {
    /// Stable memory record identifier.
    pub record_id: String,
    /// Scope containing the candidate.
    pub scope_id: String,
    /// Lexical relevance score.
    pub lexical_score: i64,
    /// Freshness score for the candidate.
    pub freshness_score: i64,
    /// Provenance quality score.
    pub provenance_score: i64,
}

/// Candidate after deterministic weighted scoring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoredRecallCandidate {
    /// Stable memory record identifier.
    pub record_id: String,
    /// Scope containing the candidate.
    pub scope_id: String,
    /// Total score used to order candidates.
    pub total_score: i64,
    /// Component scores retained for explanation.
    pub score_breakdown: BTreeScoreBreakdown,
}

/// Score components contributing to candidate ranking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BTreeScoreBreakdown {
    /// Lexical relevance contribution.
    pub lexical: i64,
    /// Freshness contribution.
    pub freshness: i64,
    /// Provenance contribution.
    pub provenance: i64,
}

/// Invalid view, scope, policy, query, or candidate input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryViewError {
    /// A field or view invariant is invalid.
    Invalid(&'static str),
    /// Input uses an unsupported schema version.
    UnsupportedVersion(u32),
    /// Identifier conflicts with another record.
    Duplicate,
    /// Requested record or scope does not exist.
    NotFound,
    /// View does not permit reading from the requested scope.
    ReadDenied,
    /// View does not permit writing to the requested scope.
    WriteDenied,
    /// Requested scope is outside the view's root hierarchy.
    ScopeOutsideView,
    /// Requested retrieval depth exceeds the view limit.
    DepthOutsideView,
    /// Query text or complexity metadata is invalid.
    InvalidQuery,
    /// Candidate identity or score inputs are invalid.
    InvalidCandidate,
}

impl fmt::Display for MemoryViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid(value) => value,
            Self::UnsupportedVersion(_) => "unsupported_version",
            Self::Duplicate => "duplicate",
            Self::NotFound => "not_found",
            Self::ReadDenied => "read_denied",
            Self::WriteDenied => "write_denied",
            Self::ScopeOutsideView => "scope_outside_view",
            Self::DepthOutsideView => "depth_outside_view",
            Self::InvalidQuery => "invalid_query",
            Self::InvalidCandidate => "invalid_candidate",
        })
    }
}
impl std::error::Error for MemoryViewError {}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._/-".contains(&byte))
}

/// Validates scope identifiers, parent references, uniqueness, and acyclic hierarchy.
pub fn validate_scopes(scopes: &[LogicalMemoryScope]) -> Result<(), MemoryViewError> {
    if scopes.is_empty() || scopes.len() > MAX_SCOPES {
        return Err(MemoryViewError::Invalid("scopes"));
    }
    let ids: BTreeSet<&str> = scopes.iter().map(|scope| scope.id.as_str()).collect();
    if ids.len() != scopes.len()
        || scopes.iter().any(|scope| {
            !valid_id(&scope.id)
                || scope
                    .parent_id
                    .as_deref()
                    .is_some_and(|parent| !ids.contains(parent))
        })
    {
        return Err(MemoryViewError::Invalid("scope"));
    }
    for scope in scopes {
        let mut seen = BTreeSet::new();
        let mut current = Some(scope.id.as_str());
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(MemoryViewError::Invalid("scope_cycle"));
            }
            current = scopes
                .iter()
                .find(|candidate| candidate.id == id)
                .and_then(|candidate| candidate.parent_id.as_deref());
        }
    }
    Ok(())
}

/// Validates view bounds, rights, roots, and excluded secret scopes.
pub fn validate_view(view: &MemoryView) -> Result<(), MemoryViewError> {
    if view.schema_version != SCHEMA_VERSION {
        return Err(MemoryViewError::UnsupportedVersion(view.schema_version));
    }
    validate_scopes(&view.scopes)?;
    if !valid_id(&view.id)
        || view.revision == 0
        || !valid_id(&view.owner_scope)
        || view.root_scope_ids.is_empty()
        || view.root_scope_ids.len() > MAX_VIEW_SCOPES
        || view.max_depth == 0
        || view.max_depth > MAX_DEPTH
        || !(1..=MAX_RESULTS).contains(&view.max_results)
        || view
            .root_scope_ids
            .iter()
            .any(|root| !view.scopes.iter().any(|scope| scope.id == *root))
    {
        return Err(MemoryViewError::Invalid("view"));
    }
    if !view.rights.read && view.rights.write {
        return Err(MemoryViewError::Invalid("write_without_read"));
    }
    if view
        .scopes
        .iter()
        .any(|scope| matches!(scope.sensitivity, Sensitivity::Secret))
    {
        return Err(MemoryViewError::Invalid("secret_scope"));
    }
    Ok(())
}

/// Validates a view and computes its canonical serialized digest.
pub fn canonical_hash(view: &MemoryView) -> Result<String, MemoryViewError> {
    validate_view(view)?;
    let bytes = serde_json::to_vec(view).map_err(|_| MemoryViewError::Invalid("serialization"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// Returns whether a scope is within the roots and descendant depth of a view.
pub fn scope_visible(view: &MemoryView, scope_id: &str) -> Result<bool, MemoryViewError> {
    validate_view(view)?;
    if !valid_id(scope_id) {
        return Err(MemoryViewError::ScopeOutsideView);
    }
    let roots: BTreeSet<&str> = view.root_scope_ids.iter().map(String::as_str).collect();
    let mut current = Some(scope_id);
    let mut depth = 0_u8;
    while let Some(id) = current {
        if roots.contains(id) {
            return Ok(depth <= view.max_depth);
        }
        current = view
            .scopes
            .iter()
            .find(|scope| scope.id == id)
            .and_then(|scope| scope.parent_id.as_deref());
        depth = depth.saturating_add(1);
        if depth > view.max_depth {
            return Ok(false);
        }
    }
    Ok(false)
}

/// Requires read rights and scope visibility for a memory read.
pub fn authorize_read(view: &MemoryView, scope_id: &str) -> Result<(), MemoryViewError> {
    validate_view(view)?;
    if !view.rights.read {
        return Err(MemoryViewError::ReadDenied);
    }
    scope_visible(view, scope_id)?
        .then_some(())
        .ok_or(MemoryViewError::ScopeOutsideView)
}

/// Requires both read and write rights for a visible memory scope.
pub fn authorize_write(view: &MemoryView, scope_id: &str) -> Result<(), MemoryViewError> {
    authorize_read(view, scope_id)?;
    if !view.rights.write {
        return Err(MemoryViewError::WriteDenied);
    }
    Ok(())
}

/// Validates shallow/deep depth relationships and result limits.
pub fn validate_recall_policy(policy: &AdaptiveRecallPolicy) -> Result<(), MemoryViewError> {
    if policy.schema_version != SCHEMA_VERSION {
        return Err(MemoryViewError::UnsupportedVersion(policy.schema_version));
    }
    if policy.shallow_depth == 0
        || policy.shallow_depth > policy.deep_depth
        || policy.deep_depth > MAX_DEPTH
        || policy.auto_composite_depth == 0
        || policy.auto_composite_depth > policy.deep_depth
        || !(1..=MAX_RESULTS).contains(&policy.max_results)
    {
        return Err(MemoryViewError::Invalid("recall_policy"));
    }
    Ok(())
}

/// Selects effective depth and result count within policy and view bounds.
pub fn decide_recall(
    view: &MemoryView,
    policy: &AdaptiveRecallPolicy,
    mode: RecallMode,
    complexity: QueryComplexity,
    query: &str,
    read_barrier_generation: u64,
) -> Result<RecallDecision, MemoryViewError> {
    validate_view(view)?;
    validate_recall_policy(policy)?;
    if query.is_empty() || query.len() > MAX_QUERY || query.chars().any(char::is_control) {
        return Err(MemoryViewError::InvalidQuery);
    }
    let (effective_depth, reason_code) = match mode {
        RecallMode::Shallow => (policy.shallow_depth, "explicit_shallow"),
        RecallMode::Deep => (
            policy.deep_depth.min(view.max_depth),
            "explicit_deep_bounded",
        ),
        RecallMode::Auto => match complexity {
            QueryComplexity::Composite => (
                policy.auto_composite_depth.min(view.max_depth),
                "auto_composite",
            ),
            QueryComplexity::Simple | QueryComplexity::Unknown => {
                (policy.shallow_depth.min(view.max_depth), "auto_shallow")
            }
        },
    };
    if effective_depth == 0 {
        return Err(MemoryViewError::DepthOutsideView);
    }
    Ok(RecallDecision {
        mode,
        effective_depth,
        result_limit: policy.max_results.min(view.max_results),
        visible_scope_ids: view.root_scope_ids.clone(),
        score_components: vec!["lexical".into(), "freshness".into(), "provenance".into()],
        read_barrier_generation,
        reason_code: reason_code.into(),
    })
}

/// Deterministic, explainable composite score.  All components are supplied
/// by Core-owned retrieval adapters; the view only filters the eligible set.
/// Ranks visible candidates deterministically using lexical, freshness, and provenance scores.
pub fn rank_candidates(
    view: &MemoryView,
    mut candidates: Vec<RecallCandidate>,
) -> Result<Vec<ScoredRecallCandidate>, MemoryViewError> {
    validate_view(view)?;
    if candidates.len() > MAX_RESULTS {
        return Err(MemoryViewError::InvalidCandidate);
    }
    let mut scored = Vec::with_capacity(candidates.len());
    for candidate in candidates.drain(..) {
        if !valid_id(&candidate.record_id)
            || !scope_visible(view, &candidate.scope_id)?
            || [
                candidate.lexical_score,
                candidate.freshness_score,
                candidate.provenance_score,
            ]
            .iter()
            .any(|score| *score < 0 || *score > 1_000)
        {
            return Err(MemoryViewError::ScopeOutsideView);
        }
        let breakdown = BTreeScoreBreakdown {
            lexical: candidate.lexical_score * 60 / 100,
            freshness: candidate.freshness_score * 25 / 100,
            provenance: candidate.provenance_score * 15 / 100,
        };
        scored.push(ScoredRecallCandidate {
            record_id: candidate.record_id,
            scope_id: candidate.scope_id,
            total_score: breakdown.lexical + breakdown.freshness + breakdown.provenance,
            score_breakdown: breakdown,
        });
    }
    scored.sort_by(|left, right| {
        right
            .total_score
            .cmp(&left.total_score)
            .then_with(|| left.record_id.cmp(&right.record_id))
    });
    scored.truncate(view.max_results);
    Ok(scored)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(rights: MemoryViewRights) -> MemoryView {
        MemoryView {
            schema_version: 1,
            id: "view".into(),
            revision: 1,
            owner_scope: "agent".into(),
            scopes: vec![
                LogicalMemoryScope {
                    id: "workspace".into(),
                    parent_id: None,
                    sensitivity: Sensitivity::Internal,
                },
                LogicalMemoryScope {
                    id: "workspace/project".into(),
                    parent_id: Some("workspace".into()),
                    sensitivity: Sensitivity::Private,
                },
            ],
            root_scope_ids: vec!["workspace".into()],
            rights,
            max_depth: 4,
            max_results: 16,
        }
    }

    #[test]
    fn hierarchy_and_read_only_rights_are_enforced() {
        let read_only = view(MemoryViewRights {
            read: true,
            write: false,
        });
        assert!(authorize_read(&read_only, "workspace/project").is_ok());
        assert_eq!(
            authorize_write(&read_only, "workspace/project"),
            Err(MemoryViewError::WriteDenied)
        );
        assert_eq!(
            authorize_read(&read_only, "other"),
            Err(MemoryViewError::ScopeOutsideView)
        );
    }

    #[test]
    fn adaptive_depth_is_bounded_and_explainable() {
        let policy = AdaptiveRecallPolicy {
            schema_version: 1,
            shallow_depth: 1,
            deep_depth: 8,
            auto_composite_depth: 4,
            max_results: 32,
        };
        let decision = decide_recall(
            &view(MemoryViewRights {
                read: true,
                write: false,
            }),
            &policy,
            RecallMode::Deep,
            QueryComplexity::Composite,
            "bounded query",
            17,
        )
        .unwrap();
        assert_eq!(decision.effective_depth, 4);
        assert_eq!(decision.read_barrier_generation, 17);
        assert_eq!(decision.score_components.len(), 3);
    }

    #[test]
    fn invalid_hierarchy_and_control_query_fail_closed() {
        let mut invalid = view(MemoryViewRights {
            read: true,
            write: false,
        });
        invalid.scopes[0].parent_id = Some("workspace/project".into());
        assert_eq!(
            validate_view(&invalid),
            Err(MemoryViewError::Invalid("scope_cycle"))
        );
        assert_eq!(
            decide_recall(
                &view(MemoryViewRights {
                    read: true,
                    write: false
                }),
                &AdaptiveRecallPolicy {
                    schema_version: 1,
                    shallow_depth: 1,
                    deep_depth: 2,
                    auto_composite_depth: 2,
                    max_results: 2,
                },
                RecallMode::Auto,
                QueryComplexity::Unknown,
                "bad\nquery",
                0,
            ),
            Err(MemoryViewError::InvalidQuery)
        );
    }

    #[test]
    fn composite_ranking_is_explainable_and_cannot_cross_view() {
        let view = view(MemoryViewRights {
            read: true,
            write: false,
        });
        let ranked = rank_candidates(
            &view,
            vec![RecallCandidate {
                record_id: "memory-1".into(),
                scope_id: "workspace/project".into(),
                lexical_score: 900,
                freshness_score: 800,
                provenance_score: 700,
            }],
        )
        .unwrap();
        assert_eq!(ranked[0].total_score, 845);
        assert_eq!(ranked[0].score_breakdown.lexical, 540);
        assert_eq!(
            rank_candidates(
                &view,
                vec![RecallCandidate {
                    record_id: "memory-2".into(),
                    scope_id: "outside".into(),
                    lexical_score: 1,
                    freshness_score: 1,
                    provenance_score: 1,
                }],
            ),
            Err(MemoryViewError::ScopeOutsideView)
        );
    }
}
