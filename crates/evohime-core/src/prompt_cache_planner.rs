//! Deterministic, security-neutral prompt cache planning.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SEGMENTS: usize = 128;
pub const MAX_SEGMENT_BYTES: usize = 256 * 1024;
pub const MAX_KEEPALIVE_MS: i64 = 5 * 60 * 1000;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptSegment {
    pub id: String,
    pub stable: bool,
    pub content_hash: String,
    pub revision: u64,
    pub policy_version: String,
    pub sensitivity: String,
    pub bytes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCacheProfile {
    pub profile_id: String,
    pub cache_supported: bool,
    pub min_prefix_tokens: u32,
    pub max_keepalive_ms: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptCachePlan {
    pub schema_version: u32,
    pub segments: Vec<PromptSegment>,
    pub provider_profile_id: String,
    pub context_revision: String,
    pub policy_version: String,
    pub cache_key: String,
    pub keepalive_ms: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheMetric {
    pub cache_key: String,
    pub hit: bool,
    pub input_tokens: u32,
    pub cached_tokens: u32,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PromptCacheError {
    #[error("invalid prompt cache contract: {0}")]
    Invalid(&'static str),
    #[error("unsupported prompt cache schema")]
    UnsupportedVersion,
    #[error("keepalive exceeds bounded policy")]
    KeepaliveLimit,
    #[error("sensitive content cannot be labelled as cacheable")]
    SensitiveCache,
}
fn hash(v: &[u8]) -> String {
    hex::encode(Sha256::digest(v))
}
fn valid(v: &str, n: usize) -> bool {
    !v.is_empty() && v.len() <= n && !v.contains('\0')
}
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
}
