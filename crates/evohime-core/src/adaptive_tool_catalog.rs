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

/// Resolves a model-supplied tool name to a canonical id from the active
/// allow-listed loadout.
///
/// A canonical id is returned unchanged. A dotted name is accepted only when
/// the same id with `_` separators is in the loadout, so a model that still
/// remembers the pre-rename spelling dispatches to that tool and cannot reach
/// anything outside the loadout. Everything else returns `None`.
pub(crate) fn resolve_tool_call_name(name: &str, allowed_tool_ids: &[String]) -> Option<String> {
    if allowed_tool_ids.iter().any(|tool_id| tool_id == name) {
        return Some(name.to_owned());
    }
    if !name.contains('.') {
        return None;
    }
    let legacy_rewritten = name.replace('.', "_");
    allowed_tool_ids
        .iter()
        .find(|tool_id| **tool_id == legacy_rewritten)
        .cloned()
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
    let query_tokens = expand_query_tokens(query);
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
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn hash_json<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn bounded_hash(value: &str) -> String {
    hash_json(&value.chars().take(256).collect::<String>())
}

fn tokens(value: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for token in value
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|token| !token.is_empty())
    {
        let normalized = normalize_yo(token);
        if normalized != token {
            result.insert(normalized.clone());
        }
        result.insert(token.to_owned());
        if token.contains('_') {
            for part in token.split('_').filter(|part| !part.is_empty()) {
                result.insert(part.to_owned());
            }
        }
    }
    result
}

fn normalize_yo(token: &str) -> String {
    if token.contains('ё') {
        token.replace('ё', "е")
    } else {
        token.to_owned()
    }
}

fn expand_query_tokens(query: &str) -> BTreeSet<String> {
    let base = tokens(query);
    let mut expanded = base.clone();
    for token in &base {
        let normalized = normalize_yo(token);
        for (prefix, targets) in CROSS_LINGUAL_PREFIXES {
            if normalized.starts_with(prefix) {
                for target in *targets {
                    expanded.insert((*target).to_owned());
                }
            }
        }
    }
    expanded
}

/// Cross-lingual prefix rules mapping Russian (and common stem) query tokens
/// onto canonical English/tool_id tokens used in tool manifests.
const CROSS_LINGUAL_PREFIXES: &[(&str, &[&str])] = &[
    // Filesystem
    ("файл", &["file", "files", "filesystem"]),
    ("прочит", &["read", "filesystem_read"]),
    ("чита", &["read", "filesystem_read"]),
    ("чтен", &["read", "filesystem_read"]),
    ("содержим", &["read", "filesystem_read"]),
    ("запи", &["write", "filesystem_write"]),
    ("пиш", &["write", "filesystem_write"]),
    ("созда", &["create", "write", "filesystem_write", "filesystem_mkdir"]),
    ("сохран", &["write", "save", "filesystem_write"]),
    ("исправ", &["patch", "edit", "filesystem_patch"]),
    ("редакт", &["patch", "edit", "filesystem_patch"]),
    ("патч", &["patch", "filesystem_patch"]),
    ("замен", &["patch", "replace", "filesystem_patch"]),
    ("спис", &["list", "filesystem_list"]),
    ("покаж", &["show", "list"]),
    ("директор", &["directory", "directories", "list", "mkdir", "filesystem_list", "filesystem_mkdir"]),
    ("папк", &["directory", "directories", "list", "mkdir", "filesystem_list", "filesystem_mkdir"]),
    ("каталог", &["directory", "directories", "list", "catalog", "filesystem_list"]),
    ("дерев", &["list", "filesystem_list"]),
    ("структур", &["list", "filesystem_list"]),
    ("найд", &["search", "find", "grep", "filesystem_search"]),
    ("поиск", &["search", "find", "grep", "filesystem_search"]),
    ("поищ", &["search", "find", "grep", "filesystem_search"]),
    ("ищ", &["search", "find", "filesystem_search"]),
    ("удал", &["delete", "remove", "filesystem_delete"]),
    ("сотр", &["delete", "remove", "filesystem_delete"]),
    ("перемест", &["move", "rename", "filesystem_move"]),
    ("перенес", &["move", "rename", "filesystem_move"]),
    ("переимен", &["move", "rename", "filesystem_move"]),
    ("копир", &["copy", "filesystem_copy"]),
    ("скопир", &["copy", "filesystem_copy"]),
    ("размер", &["stat", "metadata", "filesystem_stat"]),
    ("метадан", &["stat", "metadata", "filesystem_stat"]),
    // Git
    ("гит", &["git", "repository"]),
    ("репозитор", &["git", "repository"]),
    ("статус", &["status", "git_status"]),
    ("состоян", &["status", "git_status"]),
    ("дифф", &["diff", "git_diff"]),
    ("разниц", &["diff", "git_diff"]),
    ("отлич", &["diff", "git_diff"]),
    ("изменен", &["diff", "changed", "git_diff", "git_changed_files"]),
    ("измен", &["patch", "diff", "changed", "git_diff", "git_changed_files", "filesystem_patch"]),
    ("коммит", &["commit", "commits", "git_commit", "git_log", "git_show"]),
    ("закоммит", &["commit", "git_commit"]),
    ("истор", &["history", "log", "git_log"]),
    ("журнал", &["history", "log", "logs", "git_log", "logs_tail"]),
    ("лог", &["log", "logs", "logs_tail", "logs_grep", "git_log"]),
    ("автор", &["blame", "ownership", "git_blame"]),
    ("блейм", &["blame", "git_blame"]),
    ("ветк", &["branch", "branches", "git_branch"]),
    ("веток", &["branch", "branches", "git_branch"]),
    ("слиян", &["merge", "git_merge"]),
    ("смерж", &["merge", "git_merge"]),
    ("мерж", &["merge", "git_merge"]),
    ("сброс", &["reset", "git_reset"]),
    ("откат", &["reset", "revert", "git_reset", "git_revert"]),
    ("ребейз", &["rebase", "git_rebase"]),
    ("перебаз", &["rebase", "git_rebase"]),
    ("тег", &["tag", "tags", "git_tag"]),
    ("стэш", &["stash", "stashes", "git_stash"]),
    ("стеш", &["stash", "stashes", "git_stash"]),
    ("спря", &["stash", "git_stash"]),
    ("пул", &["pull", "git_pull"]),
    ("подтян", &["pull", "git_pull"]),
    ("пуш", &["push", "git_push"]),
    ("запуш", &["push", "git_push"]),
    ("ремоут", &["remote", "remotes", "git_remote"]),
    ("ворктри", &["worktree", "git_worktree_create", "git_worktree_remove", "git_worktree_preflight"]),
    // Shell / Script / Process / Cargo
    ("команд", &["shell", "execute", "command", "shell_execute"]),
    ("терминал", &["shell", "execute", "shell_execute"]),
    ("консол", &["shell", "execute", "shell_execute"]),
    ("выполн", &["execute", "run", "shell_execute", "process_run"]),
    ("запуст", &["run", "execute", "start", "shell_execute", "process_run", "cargo_test"]),
    ("скрипт", &["script", "scripts", "script_execute"]),
    ("пайтон", &["script", "python", "script_execute"]),
    ("питон", &["script", "python", "script_execute"]),
    ("процесс", &["process", "run", "process_run"]),
    ("программ", &["process", "app", "application", "process_run", "app_open", "app_list"]),
    ("приложен", &["app", "applications", "open", "app_open", "app_list"]),
    ("собери", &["build", "cargo", "cargo_build"]),
    ("сборк", &["build", "cargo", "cargo_build"]),
    ("скомпил", &["build", "check", "cargo", "cargo_build", "cargo_check"]),
    ("тест", &["test", "tests", "cargo", "cargo_test"]),
    ("протест", &["test", "cargo", "cargo_test"]),
    ("провер", &["check", "test", "cargo", "cargo_check", "cargo_test", "goal_verify"]),
    ("формат", &["fmt", "format", "cargo", "cargo_fmt"]),
    ("линт", &["clippy", "lint", "cargo", "cargo_clippy"]),
    // Browser / HTTP
    ("браузер", &["browser", "web", "page", "browser_open", "browser_session_navigate"]),
    ("сайт", &["browser", "web", "page", "url", "browser_open", "browser_session_navigate"]),
    ("страниц", &["browser", "web", "page", "browser_open", "browser_extract", "browser_session_read"]),
    ("откр", &["open", "read", "app_open", "browser_open", "filesystem_read"]),
    ("ссылк", &["url", "browser", "http", "browser_open", "http_request"]),
    ("скриншот", &["screenshot", "browser", "browser_session_screenshot"]),
    ("снимок", &["screenshot", "browser", "browser_session_screenshot"]),
    ("клик", &["click", "browser", "browser_session_click"]),
    ("нажм", &["click", "browser", "browser_session_click"]),
    ("запрос", &["http", "request", "http_request"]),
    ("скач", &["http", "request", "pull", "http_request", "git_pull"]),
    // Archive
    ("архив", &["archive", "zip", "tar", "archive_create", "archive_extract", "archive_list"]),
    ("упак", &["archive", "create", "zip", "archive_create"]),
    ("сжа", &["archive", "create", "archive_create"]),
    ("сожм", &["archive", "create", "archive_create"]),
    ("распак", &["archive", "extract", "archive_extract"]),
    ("извлек", &["archive", "extract", "browser_extract", "archive_extract"]),
    // Memory / Goal / Agent / Workflow / Analysis / MCP
    ("памят", &["memory", "search", "memory_search"]),
    ("запомн", &["memory", "memory_search"]),
    ("вспомн", &["memory", "search", "memory_search"]),
    ("цел", &["goal", "goals", "goal_create", "goal_verify"]),
    ("агент", &["agent", "subagent", "hive", "agent_run", "hive_delegate_agent"]),
    ("субагент", &["agent", "subagent", "agent_run"]),
    ("делег", &["delegate", "hive", "hive_delegate_agent"]),
    ("улей", &["hive", "hive_delegate_agent", "hive_register_role"]),
    ("рол", &["role", "hive", "hive_register_role"]),
    ("сценар", &["workflow", "workflow_catalog", "workflow_start", "workflow_status", "workflow_cancel"]),
    ("воркфлоу", &["workflow", "workflow_catalog", "workflow_start", "workflow_status", "workflow_cancel"]),
    ("составн", &["workflow", "workflow_catalog", "workflow_start"]),
    ("шаблон", &["workflow", "template", "workflow_catalog"]),
    ("анализ", &["analysis", "computation", "analysis_run"]),
    ("вычисл", &["analysis", "computation", "analysis_run"]),
];

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
                manifest("filesystem_read", "read files"),
                manifest("git_status", "show status"),
            ],
            "p",
            "g",
        )
        .unwrap();
        let result = select_deterministic(&projection, "read file", 1).unwrap();
        assert_eq!(result.selected_ids, ["filesystem_read"]);
        assert!(result.cache_key.starts_with("sha256:"));
    }

    #[test]
    fn resolve_tool_call_name_accepts_the_legacy_dotted_spelling_from_the_loadout() {
        let allowed = vec!["filesystem_list".to_string(), "git_status".to_string()];

        assert_eq!(
            resolve_tool_call_name("filesystem_list", &allowed).as_deref(),
            Some("filesystem_list")
        );
        // Pre-rename spelling from a cached prompt or an older model answer.
        assert_eq!(
            resolve_tool_call_name("git.status", &allowed).as_deref(),
            Some("git_status")
        );
        // A tool outside the loadout stays unknown; the alias never grants it.
        assert_eq!(resolve_tool_call_name("filesystem.read", &allowed), None);
        assert_eq!(resolve_tool_call_name("shell_execute", &allowed), None);
    }

    #[test]
    fn resolve_tool_call_name_requires_a_dotted_legacy_form() {
        let allowed = vec!["filesystem_list".to_string()];
        assert_eq!(
            resolve_tool_call_name("filesystem_list", &allowed).as_deref(),
            Some("filesystem_list")
        );
        // Underscore names are matched exactly; no fuzzy rewriting happens.
        assert_eq!(resolve_tool_call_name("filesystem", &allowed), None);
        assert_eq!(
            resolve_tool_call_name("filesystem.list.extra", &allowed),
            None
        );
    }

    #[test]
    fn model_ids_cannot_escalate_or_duplicate() {
        let projection = build_projection(&[manifest("safe.read", "read")], "p", "g").unwrap();
        assert!(matches!(
            validate_model_ids(&projection, &["shell_execute".into()], "x", 8),
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

    fn realistic_catalog() -> ToolCatalogProjection {
        build_projection(
            &[
                manifest("analysis_run", "Run one bounded computation"),
                manifest("app_list", "List the applications that app_open can open"),
                manifest("app_open", "Open a desktop application"),
                manifest("archive_create", "Create a tar.gz or zip archive"),
                manifest("archive_extract", "Extract a tar.gz or zip archive"),
                manifest("archive_list", "List contents of an archive"),
                manifest("browser_extract", "Extract text from a web page"),
                manifest("browser_open", "Open a web page and summarize its contents"),
                manifest(
                    "browser_session_screenshot",
                    "Take a screenshot of the browser tab",
                ),
                manifest("cargo_test", "Run cargo test in the workspace"),
                manifest("filesystem_list", "List files and directories in the workspace"),
                manifest("filesystem_read", "Read UTF-8 files from the workspace"),
                manifest(
                    "filesystem_search",
                    "Search workspace files by regex or substring",
                ),
                manifest("git_branch", "List, create, checkout, or delete git branches"),
                manifest("git_commit", "Create a git commit"),
                manifest("git_diff", "Show repository diff"),
                manifest("git_log", "Show bounded repository history"),
                manifest("git_show", "Show a bounded commit or object diff"),
                manifest("git_status", "Show repository status"),
                manifest("logs_tail", "Read last N lines from a log file"),
                manifest("memory_search", "Search persistent memory entries"),
            ],
            "p",
            "g",
        )
        .unwrap()
    }

    #[test]
    fn russian_commit_history_query_selects_git_log_without_alphabetical_fallback() {
        let projection = realistic_catalog();
        let result = select_deterministic(&projection, "покажи историю коммитов", 8).unwrap();

        assert_eq!(result.fallback, None);
        assert_eq!(result.selected_ids.first().map(String::as_str), Some("git_log"));
        assert!(result.selected_ids.contains(&"git_show".to_string()));
        assert!(!result.selected_ids.contains(&"analysis_run".to_string()));
    }

    #[test]
    fn russian_queries_match_expected_tools_across_domains() {
        let projection = realistic_catalog();

        let cases = [
            ("прочитай файл конфига", "filesystem_read"),
            ("найди функцию в коде", "filesystem_search"),
            ("покажи ветки репозитория", "git_branch"),
            ("упакуй файлы в архив", "archive_create"),
            ("распакуй архив", "archive_extract"),
            ("посмотри последние строки логов", "logs_tail"),
            ("запусти тесты проекта", "cargo_test"),
            ("сделай скриншот страницы", "browser_session_screenshot"),
            ("вспомни прошлые решения из памяти", "memory_search"),
        ];

        for (query, expected_tool) in cases {
            let result = select_deterministic(&projection, query, 8).unwrap();
            assert_eq!(result.fallback, None, "unexpected fallback for '{query}'");
            assert!(
                result.selected_ids.iter().any(|id| id == expected_tool),
                "expected '{expected_tool}' for '{query}', got {:?}",
                result.selected_ids
            );
        }
    }

    #[test]
    fn unicode_descriptions_and_yo_match_directly() {
        let projection = build_projection(
            &[
                manifest("alpha_tool", "Unrelated tool"),
                manifest("weather_tool", "Погода и отчёт о температуре"),
            ],
            "p",
            "g",
        )
        .unwrap();

        let by_cyrillic = select_deterministic(&projection, "погода", 1).unwrap();
        assert_eq!(by_cyrillic.fallback, None);
        assert_eq!(by_cyrillic.selected_ids, ["weather_tool"]);

        let by_yo = select_deterministic(&projection, "отчет", 1).unwrap();
        assert_eq!(by_yo.fallback, None);
        assert_eq!(by_yo.selected_ids, ["weather_tool"]);
    }
}
