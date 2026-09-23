//! Core-owned cache contract. Cache is an optimization for safe reads only.
use evohime_tool_runtime::{SideEffectClass, ToolManifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current serialized schema version for cache metadata and entries.
pub const SCHEMA_VERSION: u32 = 1;
/// Default upper bound for retained cache entries.
pub const MAX_ENTRIES: usize = 512;
/// Maximum result size eligible for storage in this cache contract.
pub const MAX_RESULT_BYTES: usize = 64 * 1024;

/// Whether a tool's result is eligible for reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cacheability {
    /// Result must always be recomputed.
    Never,
    /// Result may be reused when its policy and freshness checks pass.
    ReadOnly,
}
/// Whether a caller permits a cached response or requires a fresh execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// A valid unexpired cached result may be used.
    UseCache,
    /// Cached data must not satisfy the request.
    RequireFresh,
}
/// Lifecycle validity of a stored result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    /// Result passed validation and has not expired.
    Fresh,
    /// Result is no longer within its validity period.
    Stale,
    /// Result was explicitly invalidated by the caller or policy.
    Invalidated,
}

/// Cache eligibility attestation derived from a validated tool manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedToolCacheMetadata {
    /// Schema version for this metadata record.
    pub schema_version: u32,
    /// Stable tool identifier from the manifest.
    pub tool_id: String,
    /// Tool version whose results this metadata describes.
    pub tool_version: String,
    /// Whether results from this tool may be cached.
    pub cacheability: Cacheability,
    /// Digest binding the schema, identity, version, and eligibility decision.
    pub metadata_hash: String,
}
/// Reference to a cached result and the scopes under which it was observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Schema version for this entry.
    pub schema_version: u32,
    /// Deterministic key computed from tool, input, and authorization context.
    pub key: String,
    /// Tool that produced the result.
    pub tool_id: String,
    /// Tool version that produced the result.
    pub tool_version: String,
    /// Resource identity to which the result applies.
    pub resource_scope: String,
    /// Principal or authority boundary under which it was produced.
    pub authority_scope: String,
    /// Policy digest active when the result was produced.
    pub policy_hash: String,
    /// Reference to the stored result payload.
    pub result_ref: String,
    /// Unix timestamp in milliseconds when the result was observed.
    pub observed_at_ms: i64,
    /// Unix timestamp in milliseconds when the result expires.
    pub expires_at_ms: i64,
    /// Reference to the event establishing result provenance.
    pub provenance_ref: String,
    /// Current cache validity state.
    pub status: CacheStatus,
}
/// Limits and sensitivity policy applied when retaining cache entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachePolicy {
    /// Schema version for this policy.
    pub schema_version: u32,
    /// Maximum number of entries to retain.
    pub max_entries: usize,
    /// Default entry lifetime in milliseconds.
    pub default_ttl_ms: i64,
    /// Whether callers may retain results classified as sensitive.
    pub allow_sensitive: bool,
}
/// Reason a cache entry or tool result cannot be used.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum CacheError {
    /// Stored metadata uses a schema unsupported by this implementation.
    #[error("unsupported cache schema {0}")]
    UnsupportedVersion(u32),
    /// Tool or request context is not eligible for caching.
    #[error("cache is not trusted read-only metadata")]
    NotCacheable,
    /// The cache contract or digest is malformed.
    #[error("cache contract is invalid")]
    Invalid,
    /// Result payload exceeds the configured storage bound.
    #[error("cache result exceeds bound")]
    Limit,
    /// Entry is expired, invalidated, or freshness was required.
    #[error("cache entry is stale or invalidated")]
    Stale,
}
/// Returns the conservative default policy with sensitive caching disabled.
pub fn default_policy() -> CachePolicy {
    CachePolicy {
        schema_version: SCHEMA_VERSION,
        max_entries: MAX_ENTRIES,
        default_ttl_ms: 60_000,
        allow_sensitive: false,
    }
}
/// Creates trusted cache metadata and binds its eligibility in a digest.
pub fn metadata(
    tool_id: String,
    tool_version: String,
    cacheability: Cacheability,
) -> Result<TrustedToolCacheMetadata, CacheError> {
    if tool_id.is_empty() || tool_version.is_empty() {
        return Err(CacheError::Invalid);
    }
    let raw = serde_json::to_vec(&(SCHEMA_VERSION, &tool_id, &tool_version, cacheability))
        .map_err(|_| CacheError::Invalid)?;
    Ok(TrustedToolCacheMetadata {
        schema_version: SCHEMA_VERSION,
        tool_id,
        tool_version,
        cacheability,
        metadata_hash: hex::encode(Sha256::digest(raw)),
    })
}

/// Derives cache eligibility from a validated manifest's side effects and secrets.
pub fn metadata_from_manifest(
    manifest: &ToolManifest,
) -> Result<TrustedToolCacheMetadata, CacheError> {
    manifest.validate().map_err(|_| CacheError::Invalid)?;
    if manifest.side_effect != SideEffectClass::ReadOnly || !manifest.secret_references.is_empty() {
        return metadata(
            manifest.tool_id.clone(),
            manifest.version.clone(),
            Cacheability::Never,
        );
    }
    metadata(
        manifest.tool_id.clone(),
        manifest.version.clone(),
        Cacheability::ReadOnly,
    )
}
/// Computes a key scoped to tool version, input, resource, authority, and policy.
pub fn cache_key(
    meta: &TrustedToolCacheMetadata,
    input_hash: &str,
    resource_scope: &str,
    authority_scope: &str,
    policy_hash: &str,
) -> Result<String, CacheError> {
    if meta.schema_version != SCHEMA_VERSION
        || meta.cacheability != Cacheability::ReadOnly
        || input_hash.is_empty()
        || resource_scope.is_empty()
        || authority_scope.is_empty()
        || policy_hash.is_empty()
    {
        return Err(CacheError::NotCacheable);
    }
    let raw = serde_json::to_vec(&(
        meta.schema_version,
        &meta.tool_id,
        &meta.tool_version,
        &meta.metadata_hash,
        input_hash,
        resource_scope,
        authority_scope,
        policy_hash,
    ))
    .map_err(|_| CacheError::Invalid)?;
    Ok(hex::encode(Sha256::digest(raw)))
}
/// Rejects entries whose schema, references, freshness, or status is invalid.
pub fn validate_entry(
    entry: &CacheEntry,
    policy: &CachePolicy,
    now_ms: i64,
    freshness: Freshness,
) -> Result<(), CacheError> {
    if entry.schema_version != SCHEMA_VERSION
        || policy.schema_version != SCHEMA_VERSION
        || entry.key.len() != 64
        || entry.result_ref.is_empty()
        || entry.provenance_ref.is_empty()
        || entry.expires_at_ms < entry.observed_at_ms
    {
        return Err(CacheError::Invalid);
    }
    if freshness == Freshness::RequireFresh
        || entry.status != CacheStatus::Fresh
        || entry.expires_at_ms <= now_ms
    {
        return Err(CacheError::Stale);
    }
    Ok(())
}
/// Removes expired entries and retains the newest entries up to the policy limit.
pub fn evict(entries: &mut Vec<CacheEntry>, policy: &CachePolicy, now_ms: i64) {
    entries.retain(|e| e.status == CacheStatus::Fresh && e.expires_at_ms > now_ms);
    entries.sort_by_key(|e| e.observed_at_ms);
    if entries.len() > policy.max_entries {
        let drop_count = entries.len() - policy.max_entries;
        entries.drain(..drop_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_never_is_not_cacheable() {
        let m = metadata("read".into(), "1".into(), Cacheability::Never).unwrap();
        assert_eq!(
            cache_key(&m, "input", "resource", "account", "policy"),
            Err(CacheError::NotCacheable)
        );
    }
    #[test]
    fn key_contains_authority_and_require_fresh_bypasses() {
        let m = metadata("read".into(), "1".into(), Cacheability::ReadOnly).unwrap();
        let a = cache_key(&m, "input", "resource-a", "account", "policy").unwrap();
        let b = cache_key(&m, "input", "resource-b", "account", "policy").unwrap();
        assert_ne!(a, b);
        let e = CacheEntry {
            schema_version: 1,
            key: a,
            tool_id: "read".into(),
            tool_version: "1".into(),
            resource_scope: "resource-a".into(),
            authority_scope: "account".into(),
            policy_hash: "policy".into(),
            result_ref: "artifact:1".into(),
            observed_at_ms: 1,
            expires_at_ms: 1000,
            provenance_ref: "event:1".into(),
            status: CacheStatus::Fresh,
        };
        assert_eq!(
            validate_entry(&e, &default_policy(), 2, Freshness::RequireFresh),
            Err(CacheError::Stale)
        );
    }
}
