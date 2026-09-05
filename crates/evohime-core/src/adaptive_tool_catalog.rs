//! Core-owned bounded selection of tool manifests.
//!
//! The registry and permission engine remain authoritative. This module only
//! narrows an already authorized snapshot and never creates a capability.

use evohime_tool_runtime::ToolManifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const CATALOG_REVISION: &str = "adaptive-tool-catalog/v1";
pub const DEFAULT_MAX_TOOLS: usize = 8;
pub const HARD_MAX_TOOLS: usize = 32;
pub const MAX_COMPACT_DESCRIPTION_CHARS: usize = 256;
pub const MAX_FULL_SCHEMA_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectorKind {
    Deterministic,
    SemanticModel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackPolicy {
    DeterministicTopRanked,
    Empty,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactTool {
    pub tool_id: String,
    pub display_name: String,
    pub description: String,
    pub manifest_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCatalogProjection {
    pub revision: String,
    pub registry_hash: String,
    pub policy_hash: String,
    pub grant_hash: String,
    pub tools: Vec<CompactTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionResult {
    pub selector: SelectorKind,
    pub fallback: Option<FallbackPolicy>,
    pub selected_ids: Vec<String>,
    pub candidate_count: usize,
    pub cache_key: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CatalogError {
    #[error("max_tools exceeds hard limit")]
    MaxToolsTooLarge,
    #[error("full tool schema exceeds limit: {0}")]
    SchemaTooLarge(String),
    #[error("unknown selected tool: {0}")]
    UnknownSelectedTool(String),
    #[error("duplicate selected tool: {0}")]
    DuplicateSelectedTool(String),
    #[error("invalid tool manifest hash: {0}")]
    InvalidManifestHash(String),
}

/// Process-local cache. The complete key is intentionally derived from every
/// authority and query input, so a policy/registry/grant change cannot reuse
/// an older loadout. It contains ids and hashes only, never schemas.
#[derive(Debug, Default)]
pub struct SelectionCache {
    entries: BTreeMap<String, SelectionResult>,
}

impl SelectionCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&SelectionResult> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, result: SelectionResult) {
        self.entries.insert(result.cache_key.clone(), result);
    }

    pub fn invalidate(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn build_projection(
    manifests: &[ToolManifest],
    policy_hash: &str,
    grant_hash: &str,
) -> Result<ToolCatalogProjection, CatalogError> {
    let mut tools = Vec::new();
    for manifest in manifests {
        let schema = serde_json::to_vec(&manifest.input_schema)
            .map_err(|_| CatalogError::SchemaTooLarge(manifest.tool_id.clone()))?;
        if schema.len() > MAX_FULL_SCHEMA_BYTES {
            return Err(CatalogError::SchemaTooLarge(manifest.tool_id.clone()));
        }
        let description = manifest
            .description
            .chars()
            .take(MAX_COMPACT_DESCRIPTION_CHARS)
            .collect::<String>();
        let manifest_hash = manifest.canonical_hash().map_err(|error| {
            tracing::error!(tool_id = %manifest.tool_id, %error, "tool manifest hash failed");
            CatalogError::InvalidManifestHash(manifest.tool_id.clone())
        })?;
        tools.push(CompactTool {
            tool_id: manifest.tool_id.clone(),
            display_name: manifest.display_name.clone(),
            description,
            manifest_hash,
        });
    }
    tools.sort_by(|a, b| a.tool_id.cmp(&b.tool_id));
    Ok(ToolCatalogProjection {
        revision: CATALOG_REVISION.into(),
        registry_hash: hash_json(&tools),
        policy_hash: bounded_hash(policy_hash),
        grant_hash: bounded_hash(grant_hash),
        tools,
    })
}

pub fn select_deterministic(
    projection: &ToolCatalogProjection,
    query: &str,
    max_tools: usize,
) -> Result<SelectionResult, CatalogError> {
    if max_tools > HARD_MAX_TOOLS {
        return Err(CatalogError::MaxToolsTooLarge);
    }
    let limit = if max_tools == 0 {
        DEFAULT_MAX_TOOLS
    } else {
        max_tools
    };
    let query_tokens = tokens(query);
    let mut ranked = projection
        .tools
        .iter()
        .map(|tool| {
            let haystack = tokens(&format!(
                "{} {} {}",
                tool.tool_id, tool.display_name, tool.description
            ));
            let score = query_tokens
                .iter()
                .filter(|token| haystack.contains(*token))
                .count();
            (score, tool.tool_id.clone())
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let fallback = if query_tokens.is_empty() || ranked.iter().all(|item| item.0 == 0) {
        Some(FallbackPolicy::DeterministicTopRanked)
    } else {
        None
    };
    let selected_ids = ranked
        .into_iter()
        .filter(|(score, _)| fallback.is_some() || *score > 0)
        .take(limit)
        .map(|(_, id)| id)
        .collect::<Vec<_>>();
    Ok(SelectionResult {
        selector: SelectorKind::Deterministic,
        fallback,
        selected_ids,
        candidate_count: projection.tools.len(),
        cache_key: cache_key(projection, query, SelectorKind::Deterministic, limit),
    })
}

pub fn validate_model_ids(
    projection: &ToolCatalogProjection,
    ids: &[String],
    query: &str,
    max_tools: usize,
) -> Result<SelectionResult, CatalogError> {
    if max_tools > HARD_MAX_TOOLS {
        return Err(CatalogError::MaxToolsTooLarge);
    }
    let allowed = projection
        .tools
        .iter()
        .map(|tool| tool.tool_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for id in ids {
        if !allowed.contains(id.as_str()) {
            return Err(CatalogError::UnknownSelectedTool(id.clone()));
        }
        if !seen.insert(id) {
            return Err(CatalogError::DuplicateSelectedTool(id.clone()));
        }
    }
    let limit = if max_tools == 0 {
        DEFAULT_MAX_TOOLS
    } else {
        max_tools
    };
    let selected_ids = ids.iter().take(limit).cloned().collect::<Vec<_>>();
    Ok(SelectionResult {
        selector: SelectorKind::SemanticModel,
        fallback: None,
        selected_ids,
        candidate_count: projection.tools.len(),
        cache_key: cache_key(projection, query, SelectorKind::SemanticModel, limit),
    })
}

/// Optional model/semantic selector adapter. The adapter sees compact metadata
/// only; its output is validated against the same authorized projection. Any
/// unavailable, malformed or out-of-set answer falls back deterministically.
pub fn select_semantic_model<F>(
    projection: &ToolCatalogProjection,
    query: &str,
    max_tools: usize,
    selector: F,
) -> Result<SelectionResult, CatalogError>
where
    F: FnOnce(&[CompactTool], &str) -> Result<Vec<String>, String>,
{
    let ids = match selector(&projection.tools, query) {
        Ok(ids) => ids,
        Err(_) => return select_deterministic(projection, query, max_tools),
    };
    match validate_model_ids(projection, &ids, query, max_tools) {
        Ok(result) => Ok(result),
        Err(_) => select_deterministic(projection, query, max_tools),
    }
}

fn cache_key(
    projection: &ToolCatalogProjection,
    query: &str,
    selector: SelectorKind,
    limit: usize,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(CATALOG_REVISION.as_bytes());
    hasher.update(projection.registry_hash.as_bytes());
    hasher.update(projection.policy_hash.as_bytes());
    hasher.update(projection.grant_hash.as_bytes());
    hasher.update(query.trim().as_bytes());
    hasher.update(format!("{selector:?}:{limit}").as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn hash_json<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn bounded_hash(value: &str) -> String {
    hash_json(&value.chars().take(256).collect::<String>())
}

fn tokens(value: &str) -> BTreeSet<String> {
    value
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_permissions::Permission;
    use evohime_tool_runtime::{ApprovalMode, SideEffectClass, ToolOrigin, MANIFEST_KIND};

    fn manifest(id: &str, description: &str) -> ToolManifest {
        ToolManifest {
            kind: MANIFEST_KIND.into(),
            tool_id: id.into(),
            version: "1".into(),
            display_name: id.into(),
            description: description.into(),
            input_schema: serde_json::json!({"type":"object","additionalProperties":false}),
            output_schema: serde_json::json!({"type":"object"}),
            capability_class: "read".into(),
            side_effect: SideEffectClass::ReadOnly,
            provider_identity: "builtin".into(),
            required_permissions: vec![Permission::FilesystemRead],
            approval: ApprovalMode::OnPermission,
            workspace_scope: "workspace".into(),
            network_domains: vec![],
            secret_references: vec![],
            timeout_ms: 1000,
            output_size_limit: 1024,
            retry_class: "none".into(),
            supports_cancellation: true,
            origin: ToolOrigin::Builtin,
            source_reference: "test".into(),
            package_hash: None,
            license: None,
            compatible_core: ">=0.1".into(),
            protocol_version: "1".into(),
        }
    }

    #[test]
    fn deterministic_selection_is_bounded_and_stable() {
        let projection = build_projection(
            &[
                manifest("filesystem.read", "read files"),
                manifest("git.status", "show status"),
            ],
            "p",
            "g",
        )
        .unwrap();
        let result = select_deterministic(&projection, "read file", 1).unwrap();
        assert_eq!(result.selected_ids, ["filesystem.read"]);
        assert!(result.cache_key.starts_with("sha256:"));
    }

    #[test]
    fn model_ids_cannot_escalate_or_duplicate() {
        let projection = build_projection(&[manifest("safe.read", "read")], "p", "g").unwrap();
        assert!(matches!(
            validate_model_ids(&projection, &["shell.execute".into()], "x", 8),
            Err(CatalogError::UnknownSelectedTool(_))
        ));
        assert!(matches!(
            validate_model_ids(
                &projection,
                &["safe.read".into(), "safe.read".into()],
                "x",
                8
            ),
            Err(CatalogError::DuplicateSelectedTool(_))
        ));
    }

    #[test]
    fn semantic_selector_sees_metadata_and_falls_back_on_invalid_output() {
        let projection = build_projection(&[manifest("safe.read", "read")], "p", "g").unwrap();
        let result = select_semantic_model(&projection, "unrelated", 8, |metadata, query| {
            assert_eq!(metadata.len(), 1);
            assert_eq!(query, "unrelated");
            Ok(vec!["not-authorized".into()])
        })
        .unwrap();
        assert_eq!(result.selector, SelectorKind::Deterministic);
        assert_eq!(
            result.fallback,
            Some(FallbackPolicy::DeterministicTopRanked)
        );
    }

    #[test]
    fn empty_query_uses_explicit_fallback_without_persistence() {
        let projection =
            build_projection(&[manifest("a", "a"), manifest("b", "b")], "p", "g").unwrap();
        let result = select_deterministic(&projection, "", 0).unwrap();
        assert_eq!(
            result.fallback,
            Some(FallbackPolicy::DeterministicTopRanked)
        );
        assert_eq!(result.selected_ids.len(), 2);
    }

    #[test]
    fn cache_is_process_local_and_explicitly_invalidated() {
        let projection = build_projection(&[manifest("a", "a")], "p", "g").unwrap();
        let result = select_deterministic(&projection, "a", 8).unwrap();
        let mut cache = SelectionCache::new();
        cache.insert(result.clone());
        assert_eq!(cache.get(&result.cache_key), Some(&result));
        assert_eq!(cache.len(), 1);
        cache.invalidate();
        assert_eq!(cache.len(), 0);
    }
}
