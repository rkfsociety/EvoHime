//! Core-owned discovery and compilation of project instructions.
//!
//! Markdown is treated as untrusted text.  This module only reads an explicit
//! allowlist of filenames, never executes fenced code, and never turns rule
//! metadata into capabilities or approvals.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_RULES: usize = 64;
pub const MAX_SINGLE_RULE_BYTES: usize = 64 * 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 256 * 1024;
pub const MAX_TOKENS: usize = 16_384;
pub const MAX_ID_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Global,
    Workspace,
    Nested,
    Compatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    Always,
    RelevantPath,
    Explicit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRule {
    pub schema_version: u32,
    pub id: String,
    pub source_kind: SourceKind,
    pub source_ref: String,
    pub source_revision: u64,
    pub scope: String,
    pub paths: Vec<String>,
    pub exclude_paths: Vec<String>,
    pub activation: Activation,
    pub priority: i32,
    pub content: String,
    pub enabled: bool,
    pub sensitivity: String,
    pub content_hash: String,
    pub parsed_metadata: BTreeMap<String, String>,
}

/// Canonical Project Guidance Registry document.  The existing instruction
/// stack is the storage and precedence authority for this alias.
pub type ProjectGuidanceDocument = ProjectRule;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInstructionStackPolicy {
    pub schema_version: u32,
    pub max_rule_files: usize,
    pub max_single_rule_bytes: usize,
    pub max_total_rule_bytes: usize,
    pub max_total_tokens: usize,
}

pub fn default_policy() -> ProjectInstructionStackPolicy {
    ProjectInstructionStackPolicy {
        schema_version: SCHEMA_VERSION,
        max_rule_files: MAX_RULES,
        max_single_rule_bytes: MAX_SINGLE_RULE_BYTES,
        max_total_rule_bytes: MAX_SNAPSHOT_BYTES,
        max_total_tokens: MAX_TOKENS,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionSnapshot {
    pub schema_version: u32,
    pub workspace_root: String,
    pub active_rules: Vec<ProjectRule>,
    pub inactive_relevant_rules: Vec<String>,
    pub diagnostics: Vec<String>,
    pub source_hashes: Vec<String>,
    pub total_bytes: usize,
    pub estimated_tokens: usize,
    pub created_at_ms: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRuleProjection {
    pub id: String,
    pub source_kind: SourceKind,
    pub source_ref: String,
    pub scope: String,
    pub paths: Vec<String>,
    pub activation: Activation,
    pub priority: i32,
    pub enabled: bool,
    pub sensitivity: String,
    pub content_hash: String,
    pub source_revision: u64,
    pub why_active: String,
    pub trust_class: String,
}

pub fn trust_class(rule: &ProjectRule) -> &'static str {
    match rule.source_kind {
        SourceKind::Global
        | SourceKind::Workspace
        | SourceKind::Nested
        | SourceKind::Compatible => {
            if rule.sensitivity == "sensitive" {
                "allowlisted_sensitive"
            } else {
                "allowlisted_untrusted"
            }
        }
    }
}

pub fn project_rule(rule: &ProjectRule, why_active: &str) -> ProjectRuleProjection {
    ProjectRuleProjection {
        id: rule.id.clone(),
        source_kind: rule.source_kind,
        source_ref: rule.source_ref.clone(),
        scope: rule.scope.clone(),
        paths: rule.paths.clone(),
        activation: rule.activation,
        priority: rule.priority,
        enabled: rule.enabled,
        sensitivity: rule.sensitivity.clone(),
        content_hash: rule.content_hash.clone(),
        source_revision: rule.source_revision,
        why_active: why_active.to_owned(),
        trust_class: trust_class(rule).to_owned(),
    }
}

pub fn project_snapshot(snapshot: &InstructionSnapshot) -> serde_json::Value {
    serde_json::json!({
        "schema_version": snapshot.schema_version,
        "workspace_root": "workspace-bound",
        "active_rules": snapshot.active_rules.iter().map(|rule| project_rule(rule, "active")).collect::<Vec<_>>(),
        "inactive_relevant_rules": snapshot.inactive_relevant_rules,
        "diagnostics": snapshot.diagnostics,
        "source_hashes": snapshot.source_hashes,
        "total_bytes": snapshot.total_bytes,
        "estimated_tokens": snapshot.estimated_tokens,
        "content_hash": snapshot.content_hash,
        "redacted": true,
    })
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InstructionError {
    #[error("unsupported instruction schema version {0}")]
    UnsupportedVersion(u32),
    #[error("instruction path escapes the workspace root")]
    PathEscape,
    #[error("instruction source is not allowlisted")]
    UnallowlistedSource,
    #[error("instruction file is too large")]
    RuleTooLarge,
    #[error("instruction rule limit exceeded")]
    RuleLimit,
    #[error("instruction snapshot budget exceeded")]
    BudgetExceeded,
    #[error("invalid instruction frontmatter")]
    InvalidFrontmatter,
    #[error("invalid instruction text or identifier")]
    InvalidText,
    #[error("instruction contains authority-bearing metadata")]
    AuthorityMetadata,
    #[error("instruction serialization failed")]
    Serialization,
    #[error("instruction filesystem error: {0}")]
    Io(String),
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}
fn normalize_path(value: &str) -> String {
    value
        .replace('\\', "/")
        .trim_matches('/')
        .to_ascii_lowercase()
}
fn relative_path(root: &Path, path: &Path) -> Result<String, InstructionError> {
    path.strip_prefix(root)
        .map(|p| normalize_path(&p.to_string_lossy()))
        .map_err(|_| InstructionError::PathEscape)
}
fn source_revision(bytes: &[u8]) -> u64 {
    let digest = Sha256::digest(bytes);
    u64::from_be_bytes(digest[..8].try_into().unwrap_or_default())
}
fn content_hash<T: Serialize>(value: &T) -> Result<String, InstructionError> {
    serde_json::to_vec(value)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|_| InstructionError::Serialization)
}

fn parse_frontmatter(text: &str) -> Result<(BTreeMap<String, String>, String), InstructionError> {
    if !text.starts_with("---\n") {
        return Ok((BTreeMap::new(), text.to_owned()));
    }
    let rest = &text[4..];
    let Some(end) = rest.find("\n---") else {
        return Err(InstructionError::InvalidFrontmatter);
    };
    let mut metadata: BTreeMap<String, String> = BTreeMap::new();
    let mut current_list: Option<String> = None;
    for line in rest[..end].lines() {
        let line = line.trim();
        if let Some(item) = line.strip_prefix("- ") {
            if let Some(key) = &current_list {
                metadata
                    .entry(key.clone())
                    .and_modify(|v| {
                        if !v.is_empty() {
                            v.push(',');
                        }
                        v.push_str(item.trim());
                    })
                    .or_insert_with(|| item.trim().to_owned());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(InstructionError::InvalidFrontmatter);
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches(['"', '\'']).to_owned();
        if key.is_empty()
            || [
                "tools",
                "capabilities",
                "credentials",
                "approvals",
                "grants",
            ]
            .contains(&key.as_str())
        {
            return Err(InstructionError::AuthorityMetadata);
        }
        current_list = (value.is_empty()
            && ["paths", "exclude_paths", "recommended_skills"].contains(&key.as_str()))
        .then_some(key.clone());
        metadata.insert(key, value);
    }
    Ok((
        metadata,
        rest[end + 5..].trim_start_matches('\n').to_owned(),
    ))
}

fn parse_rule(
    root: &Path,
    path: &Path,
    source_kind: SourceKind,
    source_ref: String,
    bytes: Vec<u8>,
) -> Result<ProjectRule, InstructionError> {
    if bytes.len() > MAX_SINGLE_RULE_BYTES {
        return Err(InstructionError::RuleTooLarge);
    }
    let text = String::from_utf8(bytes.clone()).map_err(|_| InstructionError::InvalidText)?;
    let (metadata, content) = parse_frontmatter(&text)?;
    let fallback = path.file_stem().and_then(|v| v.to_str()).unwrap_or("rule");
    let id = metadata.get("id").cloned().unwrap_or_else(|| {
        if source_kind == SourceKind::Compatible && fallback.eq_ignore_ascii_case("AGENTS") {
            "AGENTS.md".to_owned()
        } else {
            format!("{}-{}", source_ref.replace('/', "-"), fallback)
        }
    });
    if !valid_id(&id) || content.is_empty() {
        return Err(InstructionError::InvalidText);
    }
    let paths: Vec<String> = metadata
        .get("paths")
        .map(|v| {
            v.split(',')
                .filter(|p| !p.is_empty())
                .map(normalize_path)
                .collect()
        })
        .unwrap_or_default();
    let exclude_paths: Vec<String> = metadata
        .get("exclude_paths")
        .map(|v| {
            v.split(',')
                .filter(|p| !p.is_empty())
                .map(normalize_path)
                .collect()
        })
        .unwrap_or_default();
    let activation =
        match metadata
            .get("activation")
            .map(String::as_str)
            .unwrap_or(if paths.is_empty() {
                "always"
            } else {
                "relevant-path"
            }) {
            "always" => Activation::Always,
            "relevant-path" => Activation::RelevantPath,
            "explicit" => Activation::Explicit,
            _ => return Err(InstructionError::InvalidFrontmatter),
        };
    let priority = metadata
        .get("priority")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let scope = relative_path(root, path.parent().unwrap_or(root)).unwrap_or_default();
    let sensitivity = if metadata.keys().any(|key| {
        ["secret", "password", "token"]
            .iter()
            .any(|part| key.contains(part))
    }) {
        "sensitive"
    } else {
        "untrusted"
    };
    let hash = content_hash(&(SCHEMA_VERSION, &id, &source_ref, &content))?;
    Ok(ProjectRule {
        schema_version: SCHEMA_VERSION,
        id,
        source_kind,
        source_ref,
        source_revision: source_revision(&bytes),
        scope,
        paths,
        exclude_paths,
        activation,
        priority,
        content,
        enabled: metadata
            .get("enabled_by_default")
            .map(|v| v != "false")
            .unwrap_or(true),
        sensitivity: sensitivity.to_owned(),
        content_hash: hash,
        parsed_metadata: metadata,
    })
}

fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern = normalize_path(pattern);
    let path = normalize_path(path);
    let p: Vec<_> = pattern.split('/').collect();
    let s: Vec<_> = path.split('/').collect();
    fn walk(p: &[&str], s: &[&str]) -> bool {
        if p.is_empty() {
            return s.is_empty();
        }
        if p[0] == "**" {
            return walk(&p[1..], s) || (!s.is_empty() && walk(p, &s[1..]));
        }
        if s.is_empty() {
            return false;
        }
        (p[0] == "*" || p[0] == s[0]) && walk(&p[1..], &s[1..])
    }
    walk(&p, &s)
}

pub fn rule_applies(
    rule: &ProjectRule,
    relevant_paths: &[String],
    explicit_ids: &[String],
) -> bool {
    if !rule.enabled {
        return false;
    }
    match rule.activation {
        Activation::Always => true,
        Activation::Explicit => explicit_ids.iter().any(|id| id == &rule.id),
        Activation::RelevantPath => relevant_paths.iter().any(|path| {
            rule.paths.is_empty()
                || (rule.paths.iter().any(|pattern| glob_match(pattern, path))
                    && !rule
                        .exclude_paths
                        .iter()
                        .any(|pattern| glob_match(pattern, path)))
        }),
    }
}

pub fn validate_policy(policy: &ProjectInstructionStackPolicy) -> Result<(), InstructionError> {
    if policy.schema_version != SCHEMA_VERSION {
        return Err(InstructionError::UnsupportedVersion(policy.schema_version));
    }
    if policy.max_rule_files == 0
        || policy.max_rule_files > MAX_RULES
        || policy.max_single_rule_bytes == 0
        || policy.max_single_rule_bytes > MAX_SINGLE_RULE_BYTES
        || policy.max_total_rule_bytes == 0
        || policy.max_total_rule_bytes > MAX_SNAPSHOT_BYTES
        || policy.max_total_tokens == 0
        || policy.max_total_tokens > MAX_TOKENS
    {
        return Err(InstructionError::BudgetExceeded);
    }
    Ok(())
}

pub fn compile_snapshot(
    root: &Path,
    mut rules: Vec<ProjectRule>,
    relevant_paths: &[String],
    explicit_ids: &[String],
    policy: &ProjectInstructionStackPolicy,
    now_ms: i64,
) -> Result<InstructionSnapshot, InstructionError> {
    validate_policy(policy)?;
    let root = root
        .canonicalize()
        .map_err(|e| InstructionError::Io(e.to_string()))?;
    if rules.len() > policy.max_rule_files {
        return Err(InstructionError::RuleLimit);
    }
    rules.retain(|rule| rule.schema_version == SCHEMA_VERSION);
    rules.sort_by(|a, b| {
        (b.priority, b.scope.matches('/').count(), &a.id).cmp(&(
            a.priority,
            a.scope.matches('/').count(),
            &b.id,
        ))
    });
    let inactive_relevant_rules = rules
        .iter()
        .filter(|rule| !rule_applies(rule, relevant_paths, explicit_ids) && rule.enabled)
        .map(|rule| rule.id.clone())
        .collect();
    let active_rules: Vec<_> = rules
        .into_iter()
        .filter(|rule| rule_applies(rule, relevant_paths, explicit_ids))
        .collect();
    let total_bytes: usize = active_rules.iter().map(|rule| rule.content.len()).sum();
    let estimated_tokens = total_bytes.div_ceil(4);
    if total_bytes > policy.max_total_rule_bytes || estimated_tokens > policy.max_total_tokens {
        return Err(InstructionError::BudgetExceeded);
    }
    let source_hashes = active_rules
        .iter()
        .map(|rule| rule.content_hash.clone())
        .collect();
    let mut snapshot = InstructionSnapshot {
        schema_version: SCHEMA_VERSION,
        workspace_root: root.to_string_lossy().to_string(),
        active_rules,
        inactive_relevant_rules,
        diagnostics: Vec::new(),
        source_hashes,
        total_bytes,
        estimated_tokens,
        created_at_ms: now_ms,
        content_hash: String::new(),
    };
    snapshot.content_hash = content_hash(&snapshot.clone())?;
    Ok(snapshot)
}

fn collect_files(
    root: &Path,
    base: &Path,
    out: &mut Vec<(PathBuf, SourceKind, String)>,
) -> Result<(), InstructionError> {
    if out.len() >= MAX_RULES {
        return Err(InstructionError::RuleLimit);
    }
    let metadata = fs::symlink_metadata(root).map_err(|e| InstructionError::Io(e.to_string()))?;
    if metadata.file_type().is_symlink() {
        return Err(InstructionError::PathEscape);
    }
    if metadata.is_file() {
        let name = root
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default();
        if name == "AGENTS.md"
            || (root
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|v| v.to_str())
                == Some("rules")
                && root.extension().and_then(|v| v.to_str()) == Some("md"))
        {
            let kind = if name == "AGENTS.md" {
                if root.parent() == Some(base) {
                    SourceKind::Compatible
                } else {
                    SourceKind::Nested
                }
            } else if root.parent() == Some(&base.join(".evohime").join("rules")) {
                SourceKind::Workspace
            } else {
                SourceKind::Global
            };
            let reference = if kind == SourceKind::Global {
                format!("global/{}", name)
            } else {
                relative_path(base, root)?
            };
            out.push((root.to_path_buf(), kind, reference));
        }
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|e| InstructionError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| InstructionError::Io(e.to_string()))?;
        let path = entry.path();
        if fs::symlink_metadata(&path)
            .map_err(|e| InstructionError::Io(e.to_string()))?
            .file_type()
            .is_symlink()
        {
            continue;
        }
        collect_files(&path, base, out)?;
    }
    Ok(())
}

pub fn discover_rules(
    workspace_root: &Path,
    global_rules_root: Option<&Path>,
) -> Result<Vec<ProjectRule>, InstructionError> {
    let root = workspace_root
        .canonicalize()
        .map_err(|e| InstructionError::Io(e.to_string()))?;
    let mut files = Vec::new();
    if root.join("AGENTS.md").exists() {
        collect_files(&root.join("AGENTS.md"), &root, &mut files)?;
    }
    let project_rules = root.join(".evohime").join("rules");
    if project_rules.exists() {
        collect_files(&project_rules, &root, &mut files)?;
    }
    fn nested(
        dir: &Path,
        base: &Path,
        files: &mut Vec<(PathBuf, SourceKind, String)>,
    ) -> Result<(), InstructionError> {
        for entry in fs::read_dir(dir).map_err(|e| InstructionError::Io(e.to_string()))? {
            let p = entry
                .map_err(|e| InstructionError::Io(e.to_string()))?
                .path();
            if p.file_name().and_then(|value| value.to_str()) == Some(".evohime") {
                continue;
            }
            let m = fs::symlink_metadata(&p).map_err(|e| InstructionError::Io(e.to_string()))?;
            if m.file_type().is_symlink() {
                continue;
            }
            if m.is_dir() {
                let agents = p.join("AGENTS.md");
                if agents.exists() {
                    collect_files(&agents, base, files)?;
                }
                nested(&p, base, files)?;
            }
        }
        Ok(())
    }
    nested(&root, &root, &mut files)?;
    if let Some(global) = global_rules_root {
        if global.exists() {
            collect_files(global, &root, &mut files)?;
        }
    }
    files.sort_by(|a, b| a.2.cmp(&b.2));
    let mut rules = Vec::new();
    for (path, kind, reference) in files {
        let bytes = fs::read(&path).map_err(|e| InstructionError::Io(e.to_string()))?;
        rules.push(parse_rule(&root, &path, kind, reference, bytes)?);
    }
    Ok(rules)
}

pub fn global_rules_root_from_env() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|value| PathBuf::from(value).join("EvoHime").join("rules"))
}

pub fn discover_guidance(
    root: &Path,
    global_root: Option<&Path>,
) -> Result<Vec<ProjectGuidanceDocument>, InstructionError> {
    discover_rules(root, global_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_activation_is_canonical_and_excludes_nested_match() {
        let rule = ProjectRule {
            schema_version: SCHEMA_VERSION,
            id: "rust".into(),
            source_kind: SourceKind::Nested,
            source_ref: "crates/AGENTS.md".into(),
            source_revision: 1,
            scope: "crates".into(),
            paths: vec!["crates/**".into()],
            exclude_paths: vec!["crates/generated/**".into()],
            activation: Activation::RelevantPath,
            priority: 1,
            content: "do not use unsafe".into(),
            enabled: true,
            sensitivity: "untrusted".into(),
            content_hash: "h".into(),
            parsed_metadata: BTreeMap::new(),
        };
        assert!(rule_applies(&rule, &["crates/core/lib.rs".into()], &[]));
        assert!(!rule_applies(
            &rule,
            &["crates/generated/lib.rs".into()],
            &[]
        ));
        assert_eq!(trust_class(&rule), "allowlisted_untrusted");
    }
    #[test]
    fn snapshot_hash_and_budget_are_deterministic() {
        let policy = default_policy();
        let rule = ProjectRule {
            schema_version: SCHEMA_VERSION,
            id: "a".into(),
            source_kind: SourceKind::Workspace,
            source_ref: ".evohime/rules/a.md".into(),
            source_revision: 1,
            scope: String::new(),
            paths: vec![],
            exclude_paths: vec![],
            activation: Activation::Always,
            priority: 1,
            content: "rule".into(),
            enabled: true,
            sensitivity: "untrusted".into(),
            content_hash: "h".into(),
            parsed_metadata: BTreeMap::new(),
        };
        let left =
            compile_snapshot(Path::new("."), vec![rule.clone()], &[], &[], &policy, 1).unwrap();
        let right = compile_snapshot(Path::new("."), vec![rule], &[], &[], &policy, 1).unwrap();
        assert_eq!(left.content_hash, right.content_hash);
    }
}
