//! Core-owned cache contract. Cache is an optimization for safe reads only.
use evohime_tool_runtime::{SideEffectClass, ToolManifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ENTRIES: usize = 512;
pub const MAX_RESULT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cacheability {
    Never,
    ReadOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    UseCache,
    RequireFresh,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    Fresh,
    Stale,
    Invalidated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedToolCacheMetadata {
    pub schema_version: u32,
    pub tool_id: String,
    pub tool_version: String,
    pub cacheability: Cacheability,
    pub metadata_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    pub schema_version: u32,
    pub key: String,
    pub tool_id: String,
    pub tool_version: String,
    pub resource_scope: String,
    pub authority_scope: String,
    pub policy_hash: String,
    pub result_ref: String,
    pub observed_at_ms: i64,
    pub expires_at_ms: i64,
    pub provenance_ref: String,
    pub status: CacheStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachePolicy {
    pub schema_version: u32,
    pub max_entries: usize,
    pub default_ttl_ms: i64,
    pub allow_sensitive: bool,
}
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum CacheError {
    #[error("unsupported cache schema {0}")]
    UnsupportedVersion(u32),
    #[error("cache is not trusted read-only metadata")]
    NotCacheable,
    #[error("cache contract is invalid")]
    Invalid,
    #[error("cache result exceeds bound")]
    Limit,
    #[error("cache entry is stale or invalidated")]
    Stale,
}
pub fn default_policy() -> CachePolicy {
    CachePolicy {
        schema_version: SCHEMA_VERSION,
        max_entries: MAX_ENTRIES,
        default_ttl_ms: 60_000,
        allow_sensitive: false,
    }
}
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
