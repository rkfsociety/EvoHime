//! Deterministic, security-neutral prompt cache planning.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Serialized schema version for prompt cache plans.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of content segments accepted in one plan.
pub const MAX_SEGMENTS: usize = 128;
/// Maximum content size accepted for an individual segment, in bytes.
pub const MAX_SEGMENT_BYTES: usize = 256 * 1024;
/// Hard upper bound for a planned provider cache keepalive duration.
pub const MAX_KEEPALIVE_MS: i64 = 5 * 60 * 1000;
/// Prompt portion represented by one bounded cache-planning segment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptSegment {
    /// Stable segment identifier.
    pub id: String,
    /// Whether the segment is stable across requests for the same revision.
    pub stable: bool,
    /// SHA-256 digest of the segment content.
    pub content_hash: String,
    /// Revision of the content represented by this segment.
    pub revision: u64,
    /// Policy version under which the content was prepared.
    pub policy_version: String,
    /// Sensitivity classification; secret content is rejected by plan creation.
    pub sensitivity: String,
    /// Segment content size in bytes.
    pub bytes: usize,
}
/// Provider cache limits used to construct a deterministic cache plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCacheProfile {
    /// Stable identifier for the provider profile.
    pub profile_id: String,
    /// Whether the provider advertises prompt cache support.
    pub cache_supported: bool,
    /// Minimum prefix length in tokens required by the provider.
    pub min_prefix_tokens: u32,
    /// Provider-specific maximum keepalive duration in milliseconds.
    pub max_keepalive_ms: i64,
}
/// Deterministic cache key and segment ordering for one model context revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptCachePlan {
    /// Serialized plan schema version.
    pub schema_version: u32,
    /// Cache segments in deterministic stable-first order.
    pub segments: Vec<PromptSegment>,
    /// Provider profile used in the cache-key calculation.
    pub provider_profile_id: String,
    /// Revision identifier for the assembled context.
    pub context_revision: String,
    /// Policy version included in cache keying.
    pub policy_version: String,
    /// SHA-256 key derived from segments and plan inputs.
    pub cache_key: String,
    /// Requested cache keepalive duration in milliseconds.
    pub keepalive_ms: i64,
}
/// Bounded provider cache hit/miss accounting for one cache key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheMetric {
    /// Cache key associated with the observation.
    pub cache_key: String,
    /// Whether the provider reported a cache hit.
    pub hit: bool,
    /// Input tokens reported for the request.
    pub input_tokens: u32,
    /// Tokens reported as served from cache.
    pub cached_tokens: u32,
}
/// Invalid input, unsupported schema, excessive keepalive, or secret segment.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PromptCacheError {
    /// A plan field or metric violated its bounds or format.
    #[error("invalid prompt cache contract: {0}")]
    Invalid(&'static str),
    /// The plan schema is not supported by this implementation.
    #[error("unsupported prompt cache schema")]
    UnsupportedVersion,
    /// Requested keepalive exceeds the provider or implementation bound.
    #[error("keepalive exceeds bounded policy")]
    KeepaliveLimit,
    /// At least one segment is classified as secret.
    #[error("sensitive content cannot be labelled as cacheable")]
    SensitiveCache,
}
fn hash(v: &[u8]) -> String {
    hex::encode(Sha256::digest(v))
}
fn valid(v: &str, n: usize) -> bool {
    !v.is_empty() && v.len() <= n && !v.contains('\0')
}
/// Creates a bounded segment and hashes its supplied content.
pub fn segment(
    id: &str,
    content: &str,
    stable: bool,
    revision: u64,
    policy_version: &str,
    sensitivity: &str,
) -> Result<PromptSegment, PromptCacheError> {
    if !valid(id, 128)
        || content.len() > MAX_SEGMENT_BYTES
        || revision == 0
        || !valid(policy_version, 128)
        || !valid(sensitivity, 32)
    {
        return Err(PromptCacheError::Invalid("segment"));
    };
    let content_hash = hash(content.as_bytes());
    Ok(PromptSegment {
        id: id.into(),
        stable,
        content_hash,
        revision,
        policy_version: policy_version.into(),
        sensitivity: sensitivity.into(),
        bytes: content.len(),
    })
}

/// Creates a stable, untrusted-classified segment from a project guidance snapshot digest.
pub fn guidance_segment(
    snapshot: &crate::project_instruction_stack::InstructionSnapshot,
) -> Result<PromptSegment, PromptCacheError> {
    segment(
        "project-guidance",
        &snapshot.content_hash,
        true,
        1,
        "project-guidance-v1",
        "untrusted",
    )
}
/// Builds a deterministic metadata plan and cache key without contacting a provider.
///
/// Segments are sorted stable-first and secret-classified content is rejected.
/// Provider execution and whether caching is actually available remain the
/// responsibility of the caller and provider adapter.
///
/// # Example
///
/// ```
/// use evohime_core::prompt_cache_planner::{build_plan, segment, ProviderCacheProfile};
///
/// let profile = ProviderCacheProfile {
///     profile_id: "local".into(),
///     cache_supported: true,
///     min_prefix_tokens: 32,
///     max_keepalive_ms: 30_000,
/// };
/// let stable = segment("instructions", "Be concise.", true, 1, "policy-v1", "internal")?;
/// let plan = build_plan(vec![stable], &profile, "context-r4", "policy-v1", 1_000)?;
/// assert_eq!(plan.provider_profile_id, "local");
/// assert_eq!(plan.cache_key.len(), 64);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_plan(
    mut segments: Vec<PromptSegment>,
    profile: &ProviderCacheProfile,
    context_revision: &str,
    policy_version: &str,
    keepalive_ms: i64,
) -> Result<PromptCachePlan, PromptCacheError> {
    if segments.is_empty()
        || segments.len() > MAX_SEGMENTS
        || !valid(&profile.profile_id, 128)
        || !valid(context_revision, 128)
        || !valid(policy_version, 128)
    {
        return Err(PromptCacheError::Invalid("plan"));
    };
    if keepalive_ms < 0 || keepalive_ms > profile.max_keepalive_ms.min(MAX_KEEPALIVE_MS) {
        return Err(PromptCacheError::KeepaliveLimit);
    };
    if segments.iter().any(|s| s.sensitivity == "secret") {
        return Err(PromptCacheError::SensitiveCache);
    };
    segments.sort_by(|a, b| a.stable.cmp(&b.stable).reverse().then(a.id.cmp(&b.id)));
    let key = hash(
        &serde_json::to_vec(&(segments.clone(), profile, context_revision, policy_version))
            .map_err(|_| PromptCacheError::Invalid("serialization"))?,
    );
    Ok(PromptCachePlan {
        schema_version: SCHEMA_VERSION,
        segments,
        provider_profile_id: profile.profile_id.clone(),
        context_revision: context_revision.into(),
        policy_version: policy_version.into(),
        cache_key: key,
        keepalive_ms,
    })
}
/// Validates that a cache metric has a digest-shaped key and consistent token counts.
pub fn validate_metric(m: &CacheMetric) -> Result<(), PromptCacheError> {
    if m.cache_key.len() != 64 || m.cached_tokens > m.input_tokens {
        return Err(PromptCacheError::Invalid("metric"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_order_and_key_are_deterministic() {
        let p = ProviderCacheProfile {
            profile_id: "profile".into(),
            cache_supported: true,
            min_prefix_tokens: 10,
            max_keepalive_ms: 1000,
        };
        let a = segment("dynamic", "d", false, 1, "policy", "public").unwrap();
        let b = segment("stable", "s", true, 1, "policy", "public").unwrap();
        let x = build_plan(vec![a.clone(), b.clone()], &p, "ctx", "policy", 0).unwrap();
        let y = build_plan(vec![b, a], &p, "ctx", "policy", 0).unwrap();
        assert_eq!(x.cache_key, y.cache_key);
        assert!(x.segments[0].stable)
    }
    #[test]
    fn invalidation_and_keepalive_fail_closed() {
        let p = ProviderCacheProfile {
            profile_id: "p".into(),
            cache_supported: true,
            min_prefix_tokens: 1,
            max_keepalive_ms: 1,
        };
        let s = segment("s", "x", true, 1, "v", "public").unwrap();
        assert!(build_plan(vec![s.clone()], &p, "ctx-1", "v", 2).is_err());
        assert!(build_plan(vec![s], &p, "ctx-2", "v", 0).is_ok());
    }

    #[test]
    fn guidance_projection_has_a_stable_cache_segment() {
        let snapshot = crate::project_instruction_stack::InstructionSnapshot {
            schema_version: 1,
            workspace_root: "workspace-bound".into(),
            active_rules: vec![],
            inactive_relevant_rules: vec![],
            diagnostics: vec![],
            source_hashes: vec![],
            total_bytes: 0,
            estimated_tokens: 0,
            created_at_ms: 1,
            content_hash: "a".repeat(64),
        };
        let first = guidance_segment(&snapshot).unwrap();
        let second = guidance_segment(&snapshot).unwrap();
        assert_eq!(first.content_hash, second.content_hash);
        assert!(first.stable);
    }
}
