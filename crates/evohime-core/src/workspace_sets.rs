//! Core-owned multi-root workspace set contract.
//!
//! A set is a collection of independently authorized roots.  This module only
//! validates and canonicalizes the logical contract; runtime effects belong to
//! later workspace-set stages.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use std::{fs, path::PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ROOTS: usize = 8;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_PATH_HINT_BYTES: usize = 260;
pub const MAX_DEFINITION_BYTES: usize = 64 * 1024;
pub const MAX_BINDING_SNAPSHOT_BYTES: usize = 256 * 1024;
pub const MAX_GRANTS_PER_ROOT: usize = 32;
pub const MAX_SEARCH_MATCHES_PER_ROOT: usize = 1024;
pub const MAX_EVIDENCE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootKind {
    GitRepository,
    OtherVcsRepository,
    PlainFolder,
    GeneratedOutputRoot,
    ReadOnlyReferenceRoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantMode {
    NotGranted,
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootGrant {
    pub capability: String,
    pub mode: GrantMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootVcsState {
    pub repository_id: String,
    pub head_commit: String,
    pub branch: Option<String>,
    pub dirty_state_hash: Option<String>,
    pub working_tree_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRootBinding {
    pub root_id: String,
    pub alias: String,
    pub canonical_path: String,
    pub kind: RootKind,
    pub vcs: Option<RootVcsState>,
    pub grants: Vec<RootGrant>,
    pub execution_policy_ref: Option<String>,
    pub sensitivity_policy_ref: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSet {
    pub schema_version: u32,
    pub id: String,
    pub version: u64,
    pub name: String,
    pub roots: Vec<WorkspaceRootBinding>,
    pub default_root_id: Option<String>,
    pub shared_instruction_policy: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSetsPolicy {
    pub schema_version: u32,
    pub max_roots: usize,
    pub max_grants_per_root: usize,
    pub max_definition_bytes: usize,
    pub max_binding_snapshot_bytes: usize,
    pub max_search_matches_per_root: usize,
    pub max_evidence_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceResourceRef {
    pub workspace_set_id: String,
    pub root_id: String,
    pub logical_path: String,
    pub revision: u64,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchScope {
    pub root_ids: Vec<String>,
    pub query: String,
    pub path_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    pub resource: WorkspaceResourceRef,
    pub match_kind: String,
}

pub fn default_policy() -> WorkspaceSetsPolicy {
    WorkspaceSetsPolicy {
        schema_version: SCHEMA_VERSION,
        max_roots: MAX_ROOTS,
        max_grants_per_root: MAX_GRANTS_PER_ROOT,
        max_definition_bytes: MAX_DEFINITION_BYTES,
        max_binding_snapshot_bytes: MAX_BINDING_SNAPSHOT_BYTES,
        max_search_matches_per_root: MAX_SEARCH_MATCHES_PER_ROOT,
        max_evidence_bytes: MAX_EVIDENCE_BYTES,
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum WorkspaceSetError {
    #[error("unsupported workspace set schema version {0}")]
    UnsupportedVersion(u32),
    #[error("workspace set id or alias is invalid")]
    InvalidIdentifier,
    #[error("workspace set has too many roots")]
    RootLimit,
    #[error("workspace root aliases or ids are not unique")]
    DuplicateRootIdentity,
    #[error("workspace root path is not canonical and absolute")]
    InvalidRootPath,
    #[error("workspace root path does not exist")]
    MissingRoot,
    #[error("workspace root grants exceed the limit")]
    GrantLimit,
    #[error("workspace set default root is unknown")]
    UnknownDefaultRoot,
    #[error("workspace set contains authority-bearing imported metadata")]
    AuthorityMetadata,
    #[error("workspace set exceeds its serialized bound")]
    DefinitionTooLarge,
    #[error("workspace set policy exceeds a supported bound")]
    InvalidPolicy,
    #[error("workspace set serialization failed")]
    Serialization,
    #[error("workspace resource reference is invalid")]
    InvalidResourceRef,
    #[error("workspace resource is outside its selected root")]
    ResourcePathEscape,
    #[error("workspace search query or scope is invalid")]
    InvalidSearch,
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn valid_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && !value.contains('\0')
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_HINT_BYTES
        && !path.contains('\0')
        && Path::new(path).is_absolute()
}

fn path_matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.replace('\\', "/").to_ascii_lowercase();
    let path = path.replace('\\', "/").to_ascii_lowercase();
    if pattern == "**" || pattern == "*" {
        return true;
    }
    let pieces: Vec<_> = pattern
        .split('*')
        .filter(|piece| !piece.is_empty())
        .collect();
    let mut cursor = 0usize;
    for piece in pieces {
        let Some(index) = path[cursor..].find(piece) else {
            return false;
        };
        cursor += index + piece.len();
    }
    true
}

pub fn validate_policy(policy: &WorkspaceSetsPolicy) -> Result<(), WorkspaceSetError> {
    if policy.schema_version != SCHEMA_VERSION
        || policy.max_roots == 0
        || policy.max_roots > MAX_ROOTS
        || policy.max_grants_per_root == 0
        || policy.max_grants_per_root > MAX_GRANTS_PER_ROOT
        || policy.max_definition_bytes == 0
        || policy.max_definition_bytes > MAX_DEFINITION_BYTES
        || policy.max_binding_snapshot_bytes == 0
        || policy.max_binding_snapshot_bytes > MAX_BINDING_SNAPSHOT_BYTES
        || policy.max_search_matches_per_root == 0
        || policy.max_search_matches_per_root > MAX_SEARCH_MATCHES_PER_ROOT
        || policy.max_evidence_bytes == 0
        || policy.max_evidence_bytes > MAX_EVIDENCE_BYTES
    {
        return Err(WorkspaceSetError::InvalidPolicy);
    }
    Ok(())
}

pub fn validate_workspace_set(
    set: &WorkspaceSet,
    policy: &WorkspaceSetsPolicy,
) -> Result<(), WorkspaceSetError> {
    validate_policy(policy)?;
    if set.schema_version != SCHEMA_VERSION {
        return Err(WorkspaceSetError::UnsupportedVersion(set.schema_version));
    }
    if !valid_identifier(&set.id) || !valid_name(&set.name) || set.roots.is_empty() {
        return Err(WorkspaceSetError::InvalidIdentifier);
    }
    if set.roots.len() > policy.max_roots {
        return Err(WorkspaceSetError::RootLimit);
    }
    let mut ids = BTreeSet::new();
    let mut aliases = BTreeSet::new();
    for root in &set.roots {
        if !valid_identifier(&root.root_id)
            || !valid_identifier(&root.alias)
            || !ids.insert(&root.root_id)
            || !aliases.insert(&root.alias)
        {
            return Err(WorkspaceSetError::DuplicateRootIdentity);
        }
        if !valid_path(&root.canonical_path) {
            return Err(WorkspaceSetError::InvalidRootPath);
        }
        if !Path::new(&root.canonical_path).is_dir() {
            return Err(WorkspaceSetError::MissingRoot);
        }
        if root.grants.len() > policy.max_grants_per_root
            || root
                .grants
                .iter()
                .any(|grant| !valid_identifier(&grant.capability))
        {
            return Err(WorkspaceSetError::GrantLimit);
        }
        if matches!(root.kind, RootKind::ReadOnlyReferenceRoot)
            && root
                .grants
                .iter()
                .any(|grant| grant.mode == GrantMode::ReadWrite)
        {
            return Err(WorkspaceSetError::AuthorityMetadata);
        }
    }
    if let Some(default_root_id) = &set.default_root_id {
        if !ids.contains(default_root_id) {
            return Err(WorkspaceSetError::UnknownDefaultRoot);
        }
    }
    let bytes = serde_json::to_vec(set).map_err(|_| WorkspaceSetError::Serialization)?;
    if bytes.len() > policy.max_definition_bytes {
        return Err(WorkspaceSetError::DefinitionTooLarge);
    }
    Ok(())
}

pub fn content_hash(set: &WorkspaceSet) -> Result<String, WorkspaceSetError> {
    let mut copy = set.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| WorkspaceSetError::Serialization)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn canonicalize_and_hash(
    mut set: WorkspaceSet,
    policy: &WorkspaceSetsPolicy,
) -> Result<WorkspaceSet, WorkspaceSetError> {
    for root in &mut set.roots {
        let canonical = Path::new(&root.canonical_path)
            .canonicalize()
            .map_err(|_| WorkspaceSetError::MissingRoot)?;
        root.canonical_path = canonical.to_string_lossy().to_string();
    }
    validate_workspace_set(&set, policy)?;
    set.content_hash = content_hash(&set)?;
    Ok(set)
}

pub fn validate_resource_ref(
    set: &WorkspaceSet,
    resource: &WorkspaceResourceRef,
) -> Result<PathBuf, WorkspaceSetError> {
    if resource.workspace_set_id != set.id
        || !valid_identifier(&resource.root_id)
        || resource.logical_path.is_empty()
        || resource.logical_path.len() > MAX_PATH_HINT_BYTES
        || Path::new(&resource.logical_path).is_absolute()
        || resource
            .logical_path
            .split(['/', '\\'])
            .any(|part| part == "..")
    {
        return Err(WorkspaceSetError::InvalidResourceRef);
    }
    let root = set
        .roots
        .iter()
        .find(|root| root.root_id == resource.root_id && root.enabled)
        .ok_or(WorkspaceSetError::InvalidResourceRef)?;
    let candidate = Path::new(&root.canonical_path).join(&resource.logical_path);
    let canonical = candidate
        .canonicalize()
        .map_err(|_| WorkspaceSetError::ResourcePathEscape)?;
    let canonical_root = Path::new(&root.canonical_path)
        .canonicalize()
        .map_err(|_| WorkspaceSetError::MissingRoot)?;
    if !canonical.starts_with(&canonical_root) {
        return Err(WorkspaceSetError::ResourcePathEscape);
    }
    Ok(canonical)
}

pub fn search(
    set: &WorkspaceSet,
    scope: &SearchScope,
    policy: &WorkspaceSetsPolicy,
) -> Result<Vec<SearchMatch>, WorkspaceSetError> {
    validate_policy(policy)?;
    if scope.query.is_empty()
        || scope.query.len() > MAX_ID_BYTES
        || scope.root_ids.len() > policy.max_roots
    {
        return Err(WorkspaceSetError::InvalidSearch);
    }
    let roots: Vec<_> = if scope.root_ids.is_empty() {
        set.roots.iter().filter(|root| root.enabled).collect()
    } else {
        set.roots
            .iter()
            .filter(|root| root.enabled && scope.root_ids.iter().any(|id| id == &root.root_id))
            .collect()
    };
    let mut matches = Vec::new();
    for root in roots {
        if !root
            .grants
            .iter()
            .any(|grant| grant.mode != GrantMode::NotGranted)
        {
            continue;
        }
        let base = Path::new(&root.canonical_path);
        let mut pending = vec![base.to_path_buf()];
        let mut root_count = 0usize;
        while let Some(path) = pending.pop() {
            for entry in fs::read_dir(&path).map_err(|_| WorkspaceSetError::ResourcePathEscape)? {
                let entry = entry.map_err(|_| WorkspaceSetError::ResourcePathEscape)?;
                let candidate = entry.path();
                let metadata = fs::symlink_metadata(&candidate)
                    .map_err(|_| WorkspaceSetError::ResourcePathEscape)?;
                if metadata.file_type().is_symlink() {
                    continue;
                }
                if metadata.is_dir() {
                    pending.push(candidate);
                    continue;
                }
                let logical = candidate
                    .strip_prefix(base)
                    .map_err(|_| WorkspaceSetError::ResourcePathEscape)?
                    .to_string_lossy()
                    .replace('\\', "/");
                if !scope.path_patterns.is_empty()
                    && !scope
                        .path_patterns
                        .iter()
                        .any(|pattern| path_matches(pattern, &logical))
                {
                    continue;
                }
                let name_match = logical
                    .to_ascii_lowercase()
                    .contains(&scope.query.to_ascii_lowercase());
                let content_match = if !name_match && metadata.len() <= 64 * 1024 {
                    fs::read(&candidate)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .is_some_and(|text| text.contains(&scope.query))
                } else {
                    false
                };
                if name_match || content_match {
                    root_count += 1;
                    if root_count > policy.max_search_matches_per_root
                        || matches.len() >= policy.max_search_matches_per_root * policy.max_roots
                    {
                        return Err(WorkspaceSetError::RootLimit);
                    }
                    matches.push(SearchMatch {
                        resource: WorkspaceResourceRef {
                            workspace_set_id: set.id.clone(),
                            root_id: root.root_id.clone(),
                            logical_path: logical,
                            revision: root.vcs.as_ref().map_or(0, |v| v.working_tree_revision),
                            content_hash: None,
                        },
                        match_kind: if name_match {
                            "path".into()
                        } else {
                            "content".into()
                        },
                    });
                }
            }
        }
    }
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fixture(path: &Path) -> WorkspaceSet {
        WorkspaceSet {
            schema_version: SCHEMA_VERSION,
            id: "set-1".into(),
            version: 1,
            name: "multi-root".into(),
            roots: vec![WorkspaceRootBinding {
                root_id: "root-1".into(),
                alias: "frontend".into(),
                canonical_path: path.to_string_lossy().into_owned(),
                kind: RootKind::PlainFolder,
                vcs: None,
                grants: vec![RootGrant {
                    capability: "filesystem.read".into(),
                    mode: GrantMode::ReadOnly,
                }],
                execution_policy_ref: None,
                sensitivity_policy_ref: None,
                enabled: true,
            }],
            default_root_id: Some("root-1".into()),
            shared_instruction_policy: None,
            created_at_ms: 1,
            updated_at_ms: 1,
            content_hash: String::new(),
        }
    }

    #[test]
    fn canonicalizes_and_hashes_unique_roots() {
        let directory = tempdir().unwrap();
        let set = canonicalize_and_hash(fixture(directory.path()), &default_policy()).unwrap();
        assert!(!set.content_hash.is_empty());
        assert!(validate_workspace_set(&set, &default_policy()).is_ok());
    }

    #[test]
    fn duplicate_alias_and_reference_root_write_are_rejected() {
        let directory = tempdir().unwrap();
        let mut set = fixture(directory.path());
        set.roots.push(WorkspaceRootBinding {
            root_id: "root-2".into(),
            alias: "frontend".into(),
            kind: RootKind::ReadOnlyReferenceRoot,
            grants: vec![RootGrant {
                capability: "filesystem.write".into(),
                mode: GrantMode::ReadWrite,
            }],
            ..set.roots[0].clone()
        });
        assert_eq!(
            validate_workspace_set(&set, &default_policy()),
            Err(WorkspaceSetError::DuplicateRootIdentity)
        );
        set.roots[1].alias = "reference".into();
        assert_eq!(
            validate_workspace_set(&set, &default_policy()),
            Err(WorkspaceSetError::AuthorityMetadata)
        );
    }

    #[test]
    fn search_returns_root_qualified_refs_and_rejects_parent_escape() {
        let directory = tempdir().unwrap();
        std::fs::write(directory.path().join("needle.txt"), "needle").unwrap();
        let set = canonicalize_and_hash(fixture(directory.path()), &default_policy()).unwrap();
        let matches = search(
            &set,
            &SearchScope {
                root_ids: vec!["root-1".into()],
                query: "needle".into(),
                path_patterns: vec![],
            },
            &default_policy(),
        )
        .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].resource.logical_path, "needle.txt");
        assert!(validate_resource_ref(
            &set,
            &WorkspaceResourceRef {
                workspace_set_id: set.id.clone(),
                root_id: "root-1".into(),
                logical_path: "../needle.txt".into(),
                revision: 0,
                content_hash: None
            }
        )
        .is_err());
    }
}
