//! Local Agentic RAG owned by Core.
//!
//! The module deliberately keeps filesystem access, SQLite publication,
//! retrieval validation, optional embeddings and citation re-validation on
//! the trusted side of desktop IPC. The renderer only receives bounded JSON
//! projections produced by the command handlers.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use ring::digest::{digest, SHA256};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

const SCANNER_VERSION: &str = "workspace-scanner/v1";
const CHUNKER_VERSION: &str = "workspace-chunker/v1";
const TOKENIZER_VERSION: &str = "sqlite-fts5-trigram/v1";
pub const PLANNER_SCHEMA_VERSION: &str = "workspace-query-planner/v1";
pub const EVIDENCE_METRICS_VERSION: &str = "evidence_metrics/v1.0";
pub const CITATION_FORMAT_VERSION: u32 = 1;
const LOCAL_EMBEDDING_MODEL: &str = "evohime-feature-hash";
const LOCAL_EMBEDDING_VERSION: &str = "v1";
const VECTOR_DIMENSION: usize = 64;
const RRF_K: f64 = 60.0;

#[derive(Debug, thiserror::Error)]
pub enum RagError {
    #[error("workspace RAG configuration is invalid: {0}")]
    InvalidConfig(String),
    #[error("workspace path is invalid: {0}")]
    InvalidWorkspace(String),
    #[error("workspace path violates sandbox policy: {0}")]
    Sandbox(String),
    #[error("workspace index is unavailable")]
    IndexUnavailable,
    #[error("workspace index operation was cancelled")]
    Cancelled,
    #[error("workspace index operation timed out")]
    Timeout,
    #[error("workspace filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("workspace SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexConfig {
    pub max_file_bytes: u64,
    pub max_line_bytes: usize,
    pub max_chunks_per_document: usize,
    pub max_files_per_run: usize,
    pub max_chunks_per_run: usize,
    pub max_chunk_bytes: usize,
    pub min_chunk_bytes: usize,
    pub stable_read_retries: u8,
    pub run_timeout_ms: u64,
    pub progress_interval_ms: u64,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: 2 * 1024 * 1024,
            max_line_bytes: 32 * 1024,
            max_chunks_per_document: 256,
            max_files_per_run: 20_000,
            max_chunks_per_run: 100_000,
            max_chunk_bytes: 8 * 1024,
            min_chunk_bytes: 256,
            stable_read_retries: 2,
            run_timeout_ms: 5 * 60 * 1000,
            progress_interval_ms: 100,
        }
    }
}

impl IndexConfig {
    pub fn validate(&self) -> Result<(), RagError> {
        if !(4 * 1024..=16 * 1024 * 1024).contains(&self.max_file_bytes) {
            return Err(RagError::InvalidConfig(
                "max_file_bytes must be 4 KiB..16 MiB".into(),
            ));
        }
        if !(256..=256 * 1024).contains(&self.max_line_bytes) {
            return Err(RagError::InvalidConfig(
                "max_line_bytes must be 256 B..256 KiB".into(),
            ));
        }
        if !(1..=4096).contains(&self.max_chunks_per_document)
            || !(1..=100_000).contains(&self.max_files_per_run)
            || !(1..=1_000_000).contains(&self.max_chunks_per_run)
        {
            return Err(RagError::InvalidConfig(
                "file/chunk budgets are outside hard limits".into(),
            ));
        }
        if !(256..=64 * 1024).contains(&self.max_chunk_bytes)
            || self.min_chunk_bytes == 0
            || self.min_chunk_bytes > self.max_chunk_bytes
        {
            return Err(RagError::InvalidConfig(
                "chunk byte limits are invalid".into(),
            ));
        }
        if self.stable_read_retries > 5 || !(1_000..=30 * 60 * 1000).contains(&self.run_timeout_ms)
        {
            return Err(RagError::InvalidConfig(
                "retry or timeout limit is invalid".into(),
            ));
        }
        if !(100..=10_000).contains(&self.progress_interval_ms) {
            return Err(RagError::InvalidConfig(
                "progress interval must be 100..10000 ms".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexProgress {
    pub run_id: String,
    pub phase: String,
    pub scanned_files: usize,
    pub indexed_files: usize,
    pub chunks: usize,
    pub excluded: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexSummary {
    pub run_id: String,
    pub workspace_key: String,
    pub generation: i64,
    pub status: String,
    pub indexed_files: usize,
    pub reused_files: usize,
    pub chunks: usize,
    pub excluded: usize,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexStatus {
    pub workspace_key: String,
    pub generation: Option<i64>,
    pub status: String,
    pub indexed_files: usize,
    pub chunks: usize,
    pub excluded: usize,
    pub dirty: bool,
    pub published_at: Option<i64>,
    pub vector_mode: String,
    pub vector_index_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueryStrategy {
    ExactSymbol,
    Lexical,
    Path,
    Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QueryFilters {
    pub path: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QueryPlan {
    pub need_search: bool,
    pub strategy: QueryStrategy,
    pub query: String,
    pub filters: QueryFilters,
    pub reason: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetrievalLimits {
    pub max_retrieval_chunks: usize,
    pub max_evidence_chunks: usize,
    pub max_context_chunks: usize,
    pub max_tokens_per_chunk: usize,
}

impl Default for RetrievalLimits {
    fn default() -> Self {
        Self {
            max_retrieval_chunks: 50,
            max_evidence_chunks: 24,
            max_context_chunks: 12,
            max_tokens_per_chunk: 2048,
        }
    }
}

impl RetrievalLimits {
    pub fn validate(&self) -> Result<(), RagError> {
        if !(1..=200).contains(&self.max_retrieval_chunks)
            || !(1..=200).contains(&self.max_evidence_chunks)
            || !(1..=64).contains(&self.max_context_chunks)
            || !(64..=8192).contains(&self.max_tokens_per_chunk)
            || self.max_evidence_chunks > self.max_retrieval_chunks
            || self.max_context_chunks > self.max_evidence_chunks
        {
            return Err(RagError::InvalidConfig(
                "retrieval limits must satisfy retrieval >= evidence >= context".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreExplanation {
    pub algorithm: String,
    pub column_weights: BTreeMap<String, f64>,
    pub term_frequencies: BTreeMap<String, usize>,
    pub document_length: usize,
    pub matched_filters: Vec<String>,
    pub excluded_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RankingExplanation {
    pub algorithm: String,
    pub lexical_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub rrf_rank: usize,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetrievedChunk {
    pub source_id: String,
    pub chunk_id: String,
    pub relative_path: String,
    pub language: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub lines: Option<[u64; 2]>,
    pub chunk_hash: String,
    pub content_hash: String,
    pub content: Option<String>,
    pub symbol: Option<String>,
    pub parent_context: String,
    pub score: f64,
    pub score_explanation: ScoreExplanation,
    pub ranking_explanation: RankingExplanation,
    pub stale: bool,
    pub redaction_status: String,
    pub checker_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchDiagnostics {
    pub mode: String,
    pub fallback_reason: Option<String>,
    pub metrics_version: String,
    pub iterations: usize,
    pub coverage: f64,
    pub stop_reason: String,
    pub result_count: usize,
    pub duration_ms: u64,
    pub query_hash: String,
    pub conflict_flag: bool,
    pub reached_limits: Vec<String>,
    pub events: Vec<RetrievalProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetrievalProgress {
    pub event_type: String,
    pub iteration: usize,
    pub strategy: QueryStrategy,
    pub result_count: usize,
    pub coverage_millis: u16,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpansionRequest {
    pub request_type: String,
    pub suggested_path: String,
    pub languages: Vec<String>,
    pub reason: String,
    pub estimated_iterations: u8,
    pub estimated_tokens: u32,
    pub estimated_seconds: u16,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    pub query_id: String,
    pub plan: QueryPlan,
    pub evidence: Vec<RetrievedChunk>,
    pub diagnostics: SearchDiagnostics,
    pub uncertainty: Option<String>,
    pub expansion_request: Option<ExpansionRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoopConfig {
    pub max_iterations: usize,
    pub wall_clock_timeout_ms: u64,
    pub token_budget: usize,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 2,
            wall_clock_timeout_ms: 30_000,
            token_budget: 8_192,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HybridConfig {
    pub enabled: bool,
    pub allowed_languages: Vec<String>,
    pub allowed_path_prefixes: Vec<String>,
    pub max_build_bytes: u64,
    pub build_timeout_ms: u64,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_languages: Vec::new(),
            allowed_path_prefixes: Vec::new(),
            max_build_bytes: 64 * 1024 * 1024,
            build_timeout_ms: 60_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CitationStatus {
    Valid,
    Updated,
    Stale,
}

impl CitationStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Updated => "updated",
            Self::Stale => "stale",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Citation {
    pub citation_format_version: u32,
    pub id: String,
    pub path: String,
    pub line_range: Option<[u64; 2]>,
    pub chunk_hash: String,
    pub status: CitationStatus,
    pub reason: String,
}

impl Citation {
    pub fn compact(&self) -> String {
        let lines = self
            .line_range
            .map(|range| format!("{}-{}", range[0], range[1]))
            .unwrap_or_else(|| "?-?".into());
        format!(
            "[cite:{}|{}:{}|{}|{}]",
            self.id,
            self.path,
            lines,
            self.chunk_hash,
            self.status.as_str()
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextBuildResult {
    pub ledger_id: String,
    pub model_context: String,
    pub selected_block_ids: Vec<String>,
    pub citations: Vec<Citation>,
    pub rejected: Vec<String>,
    pub degraded: bool,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone)]
struct DecodedFile {
    bytes: Vec<u8>,
    text: String,
    encoding: &'static str,
    decode_status: &'static str,
    modified_ms: i64,
}

#[derive(Debug, Clone)]
struct PendingDocument {
    relative_path: String,
    language: String,
    mime: String,
    file_hash: String,
    size_bytes: u64,
    encoding: String,
    decode_status: String,
    last_modified: i64,
    chunks: Vec<PendingChunk>,
}

#[derive(Debug, Clone)]
struct PendingChunk {
    ordinal: usize,
    chunk_hash: String,
    byte_start: usize,
    byte_end: usize,
    line_start: u64,
    line_end: u64,
    parent_context: String,
    text: String,
    symbol: Option<String>,
    symbol_normalized: String,
}

pub fn workspace_key(root: &Path) -> Result<String, RagError> {
    let canonical = root
        .canonicalize()
        .map_err(|error| RagError::InvalidWorkspace(error.to_string()))?;
    if !canonical.is_dir() {
        return Err(RagError::InvalidWorkspace(
            "workspace is not a directory".into(),
        ));
    }
    let normalized = canonical
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    Ok(format!("workspace-{}", sha256_hex(normalized.as_bytes())))
}

pub fn plan_query(query: &str, filters: QueryFilters) -> Result<QueryPlan, RagError> {
    let trimmed = query.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 512 {
        return Err(RagError::InvalidConfig(
            "query must contain 1..512 characters".into(),
        ));
    }
    validate_filters(&filters)?;
    let lower = trimmed.to_lowercase();
    let looks_path = trimmed.contains('/')
        || trimmed.contains('\\')
        || Path::new(trimmed)
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_lowercase().as_str(),
                    "md" | "markdown"
                        | "rs"
                        | "ts"
                        | "tsx"
                        | "js"
                        | "jsx"
                        | "json"
                        | "toml"
                        | "yaml"
                        | "yml"
                        | "txt"
                )
            });
    let is_identifier = !trimmed.contains(char::is_whitespace)
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || "_.$:#-()".contains(c));
    let asks = trimmed.contains('?')
        || [
            "найди",
            "покажи",
            "где",
            "что",
            "как",
            "find",
            "where",
            "search",
        ]
        .iter()
        .any(|word| lower.split_whitespace().any(|term| term == *word));
    let (strategy, reason, confidence) = if filters.path.is_some() || filters.language.is_some() {
        (QueryStrategy::Metadata, "explicit_filters", 1.0)
    } else if looks_path {
        (QueryStrategy::Path, "path_shape", 0.95)
    } else if is_identifier && !asks {
        (QueryStrategy::ExactSymbol, "identifier_shape", 0.9)
    } else {
        (QueryStrategy::Lexical, "natural_language", 0.85)
    };
    let normalized_query = bounded_terms(trimmed).join(" ");
    if normalized_query.is_empty() {
        let plan = QueryPlan {
            need_search: false,
            strategy: QueryStrategy::Lexical,
            query: trimmed.to_string(),
            filters,
            reason: "no_searchable_terms".into(),
            confidence: 1.0,
        };
        validate_query_plan(&plan)?;
        return Ok(plan);
    }
    let plan = QueryPlan {
        need_search: true,
        strategy,
        query: normalized_query,
        filters,
        reason: reason.into(),
        confidence,
    };
    validate_query_plan(&plan)?;
    Ok(plan)
}

pub fn validate_query_plan(plan: &QueryPlan) -> Result<(), RagError> {
    if plan.query.trim().is_empty() || plan.query.chars().count() > 512 {
        return Err(RagError::InvalidConfig("planner.query".into()));
    }
    if plan.reason.trim().is_empty() || plan.reason.chars().count() > 256 {
        return Err(RagError::InvalidConfig("planner.reason".into()));
    }
    if !(0.0..=1.0).contains(&plan.confidence) || !plan.confidence.is_finite() {
        return Err(RagError::InvalidConfig("planner.confidence".into()));
    }
    validate_filters(&plan.filters)?;
    if !plan.need_search && (plan.filters.path.is_some() || plan.filters.language.is_some()) {
        return Err(RagError::InvalidConfig("planner.need_search".into()));
    }
    if plan.need_search && bounded_terms(&plan.query).is_empty() {
        return Err(RagError::InvalidConfig("planner.query_terms".into()));
    }
    Ok(())
}

pub fn validated_plan_or_fallback(
    candidate: QueryPlan,
    original_query: &str,
    safe_filters: QueryFilters,
) -> Result<QueryPlan, RagError> {
    if validate_query_plan(&candidate).is_ok() {
        return Ok(candidate);
    }
    validate_filters(&safe_filters)?;
    let query = bounded_terms(original_query).join(" ");
    if query.is_empty() {
        return Err(RagError::InvalidConfig("planner_validation_failed".into()));
    }
    let fallback = QueryPlan {
        need_search: true,
        strategy: QueryStrategy::Lexical,
        query,
        filters: safe_filters,
        reason: "validation_failed".into(),
        confidence: 0.0,
    };
    validate_query_plan(&fallback)?;
    Ok(fallback)
}

fn validate_filters(filters: &QueryFilters) -> Result<(), RagError> {
    if let Some(path) = &filters.path {
        let value = Path::new(path);
        if path.len() > 1024
            || value.is_absolute()
            || path.starts_with("\\\\")
            || path.contains(['%', '_'])
            || value
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(RagError::Sandbox("invalid relative path filter".into()));
        }
    }
    if let Some(language) = &filters.language {
        if language.is_empty()
            || language.len() > 32
            || !language
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
        {
            return Err(RagError::InvalidConfig("invalid language filter".into()));
        }
    }
    Ok(())
}

pub fn normalize_identifier(value: &str, language: &str) -> String {
    let mut normalized = value.nfc().collect::<String>().to_lowercase();
    if normalized.ends_with("()") {
        normalized.truncate(normalized.len() - 2);
    }
    normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    match language {
        "java" | "csharp" | "python" | "javascript" | "typescript" => normalized,
        _ => normalized,
    }
}

fn bounded_terms(query: &str) -> Vec<String> {
    const STOP: &[&str] = &["и", "в", "на", "по", "the", "a", "an", "of", "to", "is"];
    query
        .split(|c: char| c.is_whitespace() || ",;!?[]{}<>\"'`".contains(c))
        .map(|term| term.trim_matches(|c: char| c == '.' || c == ':'))
        .filter(|term| (2..=64).contains(&term.chars().count()))
        .filter(|term| !STOP.contains(&term.to_lowercase().as_str()))
        .take(8)
        .map(str::to_string)
        .collect()
}

fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    digest(&SHA256, bytes.as_ref())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn file_modified_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn language_for(path: &Path) -> Option<(&'static str, &'static str)> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if name.starts_with("readme") || name.ends_with(".md") || name.ends_with(".markdown") {
        return Some(("markdown", "text/markdown"));
    }
    match path.extension()?.to_string_lossy().to_lowercase().as_str() {
        "rs" => Some(("rust", "text/x-rust")),
        "ts" | "tsx" => Some(("typescript", "text/typescript")),
        "js" | "jsx" | "mjs" | "cjs" => Some(("javascript", "text/javascript")),
        "json" => Some(("json", "application/json")),
        "toml" => Some(("toml", "application/toml")),
        "yaml" | "yml" => Some(("yaml", "application/yaml")),
        "txt" | "log" | "csv" => Some(("text", "text/plain")),
        _ => None,
    }
}

fn is_secret_path(path: &str, ragignore: &[String]) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    let name = normalized.rsplit('/').next().unwrap_or(&normalized);
    let built_in = name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".key")
        || name.ends_with(".pem")
        || name.ends_with(".pfx")
        || name.ends_with(".p12")
        || normalized.split('/').any(|part| {
            matches!(
                part,
                "secrets" | ".git" | "node_modules" | "target" | "bin" | "obj"
            )
        });
    built_in
        || ragignore
            .iter()
            .any(|pattern| simple_ignore_match(pattern, &normalized))
}

fn simple_ignore_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern
        .trim()
        .trim_start_matches('/')
        .replace('\\', "/")
        .to_lowercase();
    if pattern.is_empty() || pattern.starts_with('#') {
        return false;
    }
    if pattern.ends_with('/') {
        return path
            .split('/')
            .any(|part| part == pattern.trim_end_matches('/'));
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return path.ends_with(&format!(".{suffix}"));
    }
    if pattern.contains('*') {
        let parts = pattern
            .split('*')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let mut offset = 0;
        return parts.into_iter().all(|part| {
            path[offset..].find(part).is_some_and(|found| {
                offset += found + part.len();
                true
            })
        });
    }
    path == pattern
        || path.starts_with(&format!("{pattern}/"))
        || path.ends_with(&format!("/{pattern}"))
}

fn load_ragignore(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join(".ragignore"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn collect_files(root: &Path, config: &IndexConfig) -> Result<(Vec<PathBuf>, usize), RagError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut excluded = 0;
    let ragignore = load_ragignore(root);
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
        for entry in entries {
            let file_type = entry.file_type()?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| RagError::Sandbox("scan escaped workspace".into()))?;
            let relative_string = relative.to_string_lossy().replace('\\', "/");
            if file_type.is_symlink() || is_secret_path(&relative_string, &ragignore) {
                excluded += 1;
                continue;
            }
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                if language_for(&path).is_none() {
                    excluded += 1;
                    continue;
                }
                if files.len() >= config.max_files_per_run {
                    return Err(RagError::InvalidConfig(
                        "index run file budget exceeded".into(),
                    ));
                }
                files.push(path);
            }
        }
    }
    files.sort_by_key(|path| path.to_string_lossy().replace('\\', "/").to_lowercase());
    Ok((files, excluded))
}

fn stable_read(
    root: &Path,
    path: &Path,
    config: &IndexConfig,
) -> Result<Option<DecodedFile>, RagError> {
    for _ in 0..=config.stable_read_retries {
        let before_path = path.canonicalize()?;
        if !before_path.starts_with(root) || before_path != path.canonicalize()? {
            return Err(RagError::Sandbox("canonical path escaped workspace".into()));
        }
        let mut file = open_stable_file(&before_path)?;
        let before = file.metadata()?;
        if before.len() > config.max_file_bytes {
            return Ok(None);
        }
        let mut bytes = Vec::with_capacity(before.len() as usize);
        file.read_to_end(&mut bytes)?;
        let after_path = path.canonicalize()?;
        let after = file.metadata()?;
        let final_path_metadata = fs::metadata(&after_path)?;
        if before_path == after_path
            && same_file_identity(&after, &final_path_metadata)
            && before.len() == after.len()
            && file_modified_ms(&before) == file_modified_ms(&after)
            && bytes.len() as u64 == after.len()
        {
            if bytes.iter().take(8192).any(|byte| *byte == 0)
                && !bytes.starts_with(&[0xff, 0xfe])
                && !bytes.starts_with(&[0xfe, 0xff])
            {
                return Ok(None);
            }
            let (text, encoding, decode_status) = decode_text(&bytes);
            if text.lines().any(|line| line.len() > config.max_line_bytes) {
                return Ok(None);
            }
            if contains_secret_content(&text) {
                return Ok(None);
            }
            return Ok(Some(DecodedFile {
                bytes,
                text,
                encoding,
                decode_status,
                modified_ms: file_modified_ms(&after),
            }));
        }
    }
    Err(RagError::InvalidWorkspace(
        "unstable source snapshot after bounded retries".into(),
    ))
}

fn contains_secret_content(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("-----begin ") && lower.contains(" private key-----") {
        return true;
    }
    if lower.contains("authorization: bearer ") || lower.contains("cookie: ") {
        return true;
    }
    if lower.split_whitespace().any(|token| {
        let token = token.trim_matches(|character: char| {
            !character.is_ascii_alphanumeric() && !"._-:/@".contains(character)
        });
        let jwt_parts = token.split('.').collect::<Vec<_>>();
        (token.starts_with("ghp_") && token.len() >= 36)
            || (token.starts_with("github_pat_") && token.len() >= 30)
            || (token.starts_with("sk-") && token.len() >= 24)
            || (token.starts_with("akia")
                && token.len() == 20
                && token
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric()))
            || (jwt_parts.len() == 3
                && jwt_parts[0].starts_with("eyj")
                && jwt_parts.iter().all(|part| part.len() >= 8))
            || (token.contains("://")
                && token.contains('@')
                && token.split_once("://").is_some_and(|(_, authority)| {
                    authority
                        .split('@')
                        .next()
                        .is_some_and(|credentials| credentials.contains(':'))
                }))
    }) {
        return true;
    }
    const SECRET_KEYS: [&str; 12] = [
        "api_key",
        "apikey",
        "access_token",
        "refresh_token",
        "auth_token",
        "client_secret",
        "private_key",
        "password",
        "passwd",
        "pwd",
        "cookie",
        "session_secret",
    ];
    lower.lines().any(|line| {
        let trimmed = line.trim().trim_start_matches(['/', '*', '#', '-']);
        let Some(separator) = trimmed.find(['=', ':']) else {
            return false;
        };
        let key = trimmed[..separator]
            .trim()
            .trim_matches(['"', '\'', '`'])
            .replace(['-', '.'], "_");
        let value = trimmed[separator + 1..]
            .trim()
            .trim_matches([',', ';', '"', '\'', '`']);
        !value.is_empty()
            && value.len() >= 4
            && SECRET_KEYS.iter().any(|candidate| key == *candidate)
    })
}

#[cfg(windows)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.len() == right.len()
        && left.created().ok() == right.created().ok()
        && left.modified().ok() == right.modified().ok()
}

#[cfg(unix)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn open_stable_file(path: &Path) -> std::io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
}

#[cfg(not(windows))]
fn open_stable_file(path: &Path) -> std::io::Result<fs::File> {
    fs::File::open(path)
}

#[cfg(not(any(windows, unix)))]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.len() == right.len() && file_modified_ms(left) == file_modified_ms(right)
}

fn decode_text(bytes: &[u8]) -> (String, &'static str, &'static str) {
    if bytes.starts_with(&[0xff, 0xfe]) {
        let words = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return (String::from_utf16_lossy(&words), "utf-16le", "valid");
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        let words = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return (String::from_utf16_lossy(&words), "utf-16be", "valid");
    }
    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => (text, "utf-8", "valid"),
        Err(_) => (
            String::from_utf8_lossy(bytes).into_owned(),
            "utf-8",
            "lossy",
        ),
    }
}

fn chunk_document(
    path: &Path,
    language: &str,
    file: &DecodedFile,
    config: &IndexConfig,
) -> Vec<PendingChunk> {
    let mut boundaries = vec![0usize];
    let mut offset = 0usize;
    for line in file.text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let logical = match language {
            "markdown" => trimmed.starts_with('#'),
            "rust" => [
                "fn ", "pub fn ", "struct ", "enum ", "impl ", "mod ", "trait ",
            ]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix)),
            "typescript" | "javascript" => [
                "function ",
                "class ",
                "interface ",
                "export function ",
                "export class ",
                "const ",
            ]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix)),
            "json" | "toml" | "yaml" => {
                !trimmed.is_empty() && !trimmed.starts_with([' ', '\t', '#', '-', '}', ']'])
            }
            _ => false,
        };
        if logical && offset > *boundaries.last().unwrap_or(&0) {
            boundaries.push(offset);
        }
        offset += line.len();
        if offset.saturating_sub(*boundaries.last().unwrap_or(&0)) >= config.max_chunk_bytes {
            boundaries.push(offset);
        }
    }
    if *boundaries.last().unwrap_or(&0) != file.text.len() {
        boundaries.push(file.text.len());
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut chunks = Vec::new();
    for pair in boundaries.windows(2) {
        if chunks.len() >= config.max_chunks_per_document {
            break;
        }
        let mut start = pair[0];
        let end = pair[1];
        while start < end {
            let mut chunk_end = (start + config.max_chunk_bytes).min(end);
            while chunk_end > start && !file.text.is_char_boundary(chunk_end) {
                chunk_end -= 1;
            }
            if chunk_end == start {
                break;
            }
            let text = file.text[start..chunk_end].trim().to_string();
            if !text.is_empty() {
                let (symbol, parent_context) = parent_for_chunk(path, language, &text);
                let payload = format!("{CHUNKER_VERSION}\n{language}\n{parent_context}\n{text}");
                chunks.push(PendingChunk {
                    ordinal: chunks.len(),
                    chunk_hash: sha256_hex(payload.as_bytes()),
                    byte_start: source_byte_offset(file, start),
                    byte_end: source_byte_offset(file, chunk_end),
                    line_start: byte_to_line(&file.text, start),
                    line_end: byte_to_line(&file.text, chunk_end),
                    parent_context,
                    symbol_normalized: symbol
                        .as_deref()
                        .map(|value| normalize_identifier(value, language))
                        .unwrap_or_default(),
                    symbol,
                    text,
                });
            }
            start = chunk_end;
        }
    }
    chunks
}

fn parent_for_chunk(path: &Path, language: &str, text: &str) -> (Option<String>, String) {
    let first = text.lines().next().unwrap_or_default().trim();
    let symbol = match language {
        "markdown" => first
            .strip_prefix('#')
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        "rust" | "typescript" | "javascript" => {
            let tokens = first
                .split(|c: char| c.is_whitespace() || "({:<=".contains(c))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            tokens
                .iter()
                .position(|value| {
                    matches!(
                        *value,
                        "fn" | "struct"
                            | "enum"
                            | "impl"
                            | "mod"
                            | "trait"
                            | "function"
                            | "class"
                            | "interface"
                            | "const"
                    )
                })
                .and_then(|index| tokens.get(index + 1).copied())
        }
        _ => first
            .split([':', '='])
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    }
    .map(str::to_string);
    let parent = symbol
        .as_ref()
        .map(|symbol| format!("{} > {symbol}", path.to_string_lossy().replace('\\', "/")))
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"));
    (symbol, parent)
}

fn byte_to_line(text: &str, byte: usize) -> u64 {
    text.as_bytes()[..byte.min(text.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as u64
        + 1
}

fn source_byte_offset(file: &DecodedFile, decoded_offset: usize) -> usize {
    match file.encoding {
        "utf-16le" | "utf-16be" => {
            let boundary = previous_char_boundary(&file.text, decoded_offset.min(file.text.len()));
            2 + file.text[..boundary].encode_utf16().count() * 2
        }
        _ => decoded_offset,
    }
}

/// Builds a private generation and atomically publishes it. The caller owns
/// cancellation and progress policy; neither callback can alter the workspace
/// root or any indexed content.
pub fn index_workspace(
    connection: &mut Connection,
    workspace_root: &Path,
    config: &IndexConfig,
    rebuild: bool,
    cancelled: impl Fn() -> bool,
    mut progress: impl FnMut(IndexProgress),
) -> Result<IndexSummary, RagError> {
    config.validate()?;
    let started = Instant::now();
    let canonical_root = workspace_root
        .canonicalize()
        .map_err(|error| RagError::InvalidWorkspace(error.to_string()))?;
    if !canonical_root.is_dir() {
        return Err(RagError::InvalidWorkspace(
            "workspace is not a directory".into(),
        ));
    }
    let key = workspace_key(&canonical_root)?;
    let now = now_ms();
    connection.execute(
        "UPDATE workspace_index_runs
         SET status = 'failed', finished_at = ?2, error_summary = '[\"core_restarted\"]'
         WHERE workspace_key = ?1 AND status = 'running'",
        params![key, now],
    )?;
    let generation: i64 = connection.query_row(
        "SELECT COALESCE(MAX(generation), 0) + 1 FROM workspace_index_runs WHERE workspace_key = ?1",
        [&key],
        |row| row.get(0),
    )?;
    let run_id = format!(
        "index-{}-{}",
        generation,
        &sha256_hex(format!("{key}:{now}").as_bytes())[..16]
    );
    connection.execute(
        "INSERT INTO workspace_index_runs
         (run_id, workspace_key, generation, status, started_at, scanner_version,
          chunker_version, tokenizer_version, dirty)
         VALUES (?1, ?2, ?3, 'running', ?4, ?5, ?6, ?7, 1)",
        params![
            run_id,
            key,
            generation,
            now,
            SCANNER_VERSION,
            CHUNKER_VERSION,
            TOKENIZER_VERSION
        ],
    )?;

    let outcome = (|| -> Result<IndexSummary, RagError> {
        let (files, initial_excluded) = collect_files(&canonical_root, config)?;
        let active_generation = active_generation(connection, &key)?;
        let mut indexed = 0usize;
        let mut reused = 0usize;
        let mut chunks = 0usize;
        let mut excluded = initial_excluded;
        let mut errors = Vec::new();
        let mut last_progress = Instant::now()
            .checked_sub(Duration::from_millis(config.progress_interval_ms))
            .unwrap_or_else(Instant::now);

        for (scanned, path) in files.iter().enumerate() {
            if cancelled() {
                return Err(RagError::Cancelled);
            }
            if started.elapsed() > Duration::from_millis(config.run_timeout_ms) {
                return Err(RagError::Timeout);
            }
            let relative = path
                .strip_prefix(&canonical_root)
                .map_err(|_| RagError::Sandbox("indexed path escaped workspace".into()))?
                .to_string_lossy()
                .replace('\\', "/");
            let Some((language, mime)) = language_for(path) else {
                excluded += 1;
                continue;
            };
            let snapshot = match stable_read(&canonical_root, path, config) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => {
                    excluded += 1;
                    continue;
                }
                Err(error) => return Err(error),
            };
            if snapshot.decode_status == "lossy"
                && matches!(
                    language,
                    "rust" | "typescript" | "javascript" | "json" | "toml" | "yaml"
                )
            {
                errors.push(format!("{relative}:invalid_structured_encoding"));
                excluded += 1;
                continue;
            }
            let file_hash = sha256_hex(&snapshot.bytes);
            let reused_document = if !rebuild {
                active_generation
                    .and_then(|active| {
                        copy_unchanged_document(
                            connection,
                            &key,
                            active,
                            generation,
                            &relative,
                            &file_hash,
                            snapshot.modified_ms,
                        )
                        .transpose()
                    })
                    .transpose()?
            } else {
                None
            };
            if let Some(reused_chunks) = reused_document {
                indexed += 1;
                reused += 1;
                chunks += reused_chunks;
            } else {
                let pending = PendingDocument {
                    relative_path: relative.clone(),
                    language: language.into(),
                    mime: mime.into(),
                    file_hash,
                    size_bytes: snapshot.bytes.len() as u64,
                    encoding: snapshot.encoding.into(),
                    decode_status: snapshot.decode_status.into(),
                    last_modified: snapshot.modified_ms,
                    chunks: chunk_document(path, language, &snapshot, config),
                };
                if chunks + pending.chunks.len() > config.max_chunks_per_run {
                    return Err(RagError::InvalidConfig(
                        "index run chunk budget exceeded".into(),
                    ));
                }
                chunks += pending.chunks.len();
                insert_document(connection, &key, generation, &pending)?;
                indexed += 1;
            }
            if last_progress.elapsed() >= Duration::from_millis(config.progress_interval_ms) {
                progress(IndexProgress {
                    run_id: run_id.clone(),
                    phase: "indexing".into(),
                    scanned_files: scanned + 1,
                    indexed_files: indexed,
                    chunks,
                    excluded,
                });
                last_progress = Instant::now();
            }
        }

        assert_generation_consistent(connection, &key, generation)?;
        publish_generation(
            connection, &run_id, &key, generation, indexed, chunks, excluded, &errors,
        )?;
        gc_superseded_generations(connection, &key)?;
        progress(IndexProgress {
            run_id: run_id.clone(),
            phase: "published".into(),
            scanned_files: files.len(),
            indexed_files: indexed,
            chunks,
            excluded,
        });
        Ok(IndexSummary {
            run_id: run_id.clone(),
            workspace_key: key.clone(),
            generation,
            status: "published".into(),
            indexed_files: indexed,
            reused_files: reused,
            chunks,
            excluded,
            errors,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    })();

    if let Err(error) = &outcome {
        let status = if matches!(error, RagError::Cancelled) {
            "cancelled"
        } else {
            "failed"
        };
        let safe = bounded_error("run", &error.to_string());
        connection.execute(
            "UPDATE workspace_index_runs SET status = ?2, finished_at = ?3,
             error_count = 1, error_summary = json_array(?4), dirty = 1
             WHERE run_id = ?1 AND status = 'running'",
            params![run_id, status, now_ms(), safe],
        )?;
        cleanup_generation(connection, &key, generation)?;
        progress(IndexProgress {
            run_id: run_id.clone(),
            phase: status.into(),
            scanned_files: 0,
            indexed_files: 0,
            chunks: 0,
            excluded: 0,
        });
    }
    outcome
}

fn active_generation(
    connection: &Connection,
    workspace_key: &str,
) -> Result<Option<i64>, RagError> {
    Ok(connection
        .query_row(
            "SELECT generation FROM workspace_index_runs
             WHERE workspace_key = ?1 AND status = 'published'",
            [workspace_key],
            |row| row.get(0),
        )
        .optional()?)
}

fn copy_unchanged_document(
    connection: &Connection,
    workspace_key: &str,
    old_generation: i64,
    new_generation: i64,
    path: &str,
    file_hash: &str,
    modified_ms: i64,
) -> Result<Option<usize>, RagError> {
    let document = connection
        .query_row(
            "SELECT document_id, language, mime, size_bytes, encoding, decode_status,
                    last_modified, redaction_status, is_secret_path
             FROM workspace_documents
             WHERE workspace_key = ?1 AND generation = ?2 AND path = ?3
               AND file_hash = ?4 AND status = 'active'",
            params![workspace_key, old_generation, path, file_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()?;
    let Some((
        old_id,
        language,
        mime,
        size,
        encoding,
        decode_status,
        _old_modified,
        redaction,
        secret,
    )) = document
    else {
        return Ok(None);
    };
    let new_id = stable_id(workspace_key, new_generation, path, "document");
    connection.execute(
        "INSERT INTO workspace_documents
         (document_id, workspace_key, path, generation, language, mime, file_hash,
          size_bytes, encoding, decode_status, last_modified, indexed_at, status,
          redaction_status, is_secret_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                 'active', ?13, ?14)",
        params![
            new_id,
            workspace_key,
            path,
            new_generation,
            language,
            mime,
            file_hash,
            size,
            encoding,
            decode_status,
            modified_ms,
            now_ms(),
            redaction,
            secret
        ],
    )?;
    let mut statement = connection.prepare(
        "SELECT ordinal, chunk_hash, byte_start, byte_end, line_start, line_end,
                parent_context, chunk_text, symbol, symbol_normalized, token_count,
                byte_count, strategy_version
         FROM document_chunks WHERE document_id = ?1 ORDER BY ordinal",
    )?;
    let rows = statement
        .query_map([old_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, String>(12)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for row in &rows {
        let chunk_id = stable_id(
            workspace_key,
            new_generation,
            &format!("{path}:{}", row.0),
            "chunk",
        );
        connection.execute(
            "INSERT INTO document_chunks
             (chunk_id, document_id, workspace_key, generation, ordinal, chunk_hash,
              byte_start, byte_end, line_start, line_end, parent_context, chunk_text,
              symbol, symbol_normalized, token_count, byte_count, strategy_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                     ?13, ?14, ?15, ?16, ?17)",
            params![
                chunk_id,
                new_id,
                workspace_key,
                new_generation,
                row.0,
                row.1,
                row.2,
                row.3,
                row.4,
                row.5,
                row.6,
                row.7,
                row.8,
                row.9,
                row.10,
                row.11,
                row.12
            ],
        )?;
        insert_fts(
            connection,
            &chunk_id,
            workspace_key,
            new_generation,
            &row.7,
            &row.9,
            path,
            &row.6,
        )?;
    }
    Ok(Some(rows.len()))
}

fn insert_document(
    connection: &Connection,
    workspace_key: &str,
    generation: i64,
    document: &PendingDocument,
) -> Result<(), RagError> {
    let document_id = stable_id(
        workspace_key,
        generation,
        &document.relative_path,
        "document",
    );
    connection.execute(
        "INSERT INTO workspace_documents
         (document_id, workspace_key, path, generation, language, mime, file_hash,
          size_bytes, encoding, decode_status, last_modified, indexed_at, status,
          redaction_status, is_secret_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                 'active', 'none', 0)",
        params![
            document_id,
            workspace_key,
            document.relative_path,
            generation,
            document.language,
            document.mime,
            document.file_hash,
            document.size_bytes as i64,
            document.encoding,
            document.decode_status,
            document.last_modified,
            now_ms()
        ],
    )?;
    for chunk in &document.chunks {
        let chunk_id = stable_id(
            workspace_key,
            generation,
            &format!("{}:{}", document.relative_path, chunk.ordinal),
            "chunk",
        );
        connection.execute(
            "INSERT INTO document_chunks
             (chunk_id, document_id, workspace_key, generation, ordinal, chunk_hash,
              byte_start, byte_end, line_start, line_end, parent_context, chunk_text,
              symbol, symbol_normalized, token_count, byte_count, strategy_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                     ?13, ?14, ?15, ?16, ?17)",
            params![
                chunk_id,
                document_id,
                workspace_key,
                generation,
                chunk.ordinal as i64,
                chunk.chunk_hash,
                chunk.byte_start as i64,
                chunk.byte_end as i64,
                chunk.line_start as i64,
                chunk.line_end as i64,
                chunk.parent_context,
                chunk.text,
                chunk.symbol,
                chunk.symbol_normalized,
                estimate_tokens(&chunk.text) as i64,
                chunk.text.len() as i64,
                CHUNKER_VERSION
            ],
        )?;
        insert_fts(
            connection,
            &chunk_id,
            workspace_key,
            generation,
            &chunk.text,
            &chunk.symbol_normalized,
            &document.relative_path,
            &chunk.parent_context,
        )?;
    }
    Ok(())
}

// Аргументы повторяют колонки строки FTS-индекса.
#[allow(clippy::too_many_arguments)]
fn insert_fts(
    connection: &Connection,
    chunk_id: &str,
    workspace_key: &str,
    generation: i64,
    text: &str,
    symbol: &str,
    path: &str,
    parent: &str,
) -> Result<(), RagError> {
    connection.execute(
        "INSERT INTO workspace_chunks_fts
         (chunk_text, symbol_normalized, path, parent_context, chunk_id, workspace_key, generation)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            text,
            symbol,
            path,
            parent,
            chunk_id,
            workspace_key,
            generation
        ],
    )?;
    Ok(())
}

fn assert_generation_consistent(
    connection: &Connection,
    workspace_key: &str,
    generation: i64,
) -> Result<(), RagError> {
    let chunks: i64 = connection.query_row(
        "SELECT COUNT(*) FROM document_chunks WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
        |row| row.get(0),
    )?;
    let fts: i64 = connection.query_row(
        "SELECT COUNT(*) FROM workspace_chunks_fts WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
        |row| row.get(0),
    )?;
    let orphans: i64 = connection.query_row(
        "SELECT COUNT(*) FROM document_chunks c
         LEFT JOIN workspace_documents d ON d.document_id = c.document_id
         WHERE c.workspace_key = ?1 AND c.generation = ?2 AND d.document_id IS NULL",
        params![workspace_key, generation],
        |row| row.get(0),
    )?;
    if chunks != fts || orphans != 0 {
        return Err(RagError::InvalidConfig(format!(
            "index consistency failed: chunks={chunks}, fts={fts}, orphans={orphans}"
        )));
    }
    Ok(())
}

// Аргументы повторяют колонки публикуемого поколения индекса.
#[allow(clippy::too_many_arguments)]
fn publish_generation(
    connection: &mut Connection,
    run_id: &str,
    workspace_key: &str,
    generation: i64,
    files: usize,
    chunks: usize,
    excluded: usize,
    errors: &[String],
) -> Result<(), RagError> {
    let transaction = connection.transaction()?;
    let current: String = transaction.query_row(
        "SELECT status FROM workspace_index_runs WHERE run_id = ?1 AND generation = ?2",
        params![run_id, generation],
        |row| row.get(0),
    )?;
    if current != "running" {
        return Err(RagError::Cancelled);
    }
    let newer: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM workspace_index_runs
         WHERE workspace_key = ?1 AND generation > ?2 AND status IN ('running','published')",
        params![workspace_key, generation],
        |row| row.get(0),
    )?;
    if newer != 0 {
        return Err(RagError::Cancelled);
    }
    transaction.execute(
        "UPDATE workspace_index_runs SET status = 'superseded'
         WHERE workspace_key = ?1 AND status = 'published'",
        [workspace_key],
    )?;
    transaction.execute(
        "UPDATE workspace_index_runs
         SET status = 'published', finished_at = ?2, published_at = ?2,
             file_count = ?3, chunk_count = ?4, excluded_count = ?5,
             error_count = ?6, error_summary = ?7, dirty = 0
         WHERE run_id = ?1 AND status = 'running'",
        params![
            run_id,
            now_ms(),
            files as i64,
            chunks as i64,
            excluded as i64,
            errors.len() as i64,
            serde_json::to_string(errors).unwrap_or_else(|_| "[]".into())
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

fn cleanup_generation(
    connection: &Connection,
    workspace_key: &str,
    generation: i64,
) -> Result<(), RagError> {
    connection.execute(
        "DELETE FROM workspace_chunks_fts WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
    )?;
    connection.execute(
        "DELETE FROM document_chunks WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
    )?;
    connection.execute(
        "DELETE FROM workspace_documents WHERE workspace_key = ?1 AND generation = ?2",
        params![workspace_key, generation],
    )?;
    Ok(())
}

fn gc_superseded_generations(connection: &Connection, workspace_key: &str) -> Result<(), RagError> {
    let keep: Option<i64> = connection.query_row(
        "SELECT MAX(generation) FROM workspace_index_runs
             WHERE workspace_key = ?1 AND status = 'superseded'",
        [workspace_key],
        |row| row.get(0),
    )?;
    let mut statement = connection.prepare(
        "SELECT generation FROM workspace_index_runs
         WHERE workspace_key = ?1 AND status IN ('superseded','failed','cancelled')
           AND generation != COALESCE(?2, -1)",
    )?;
    let generations = statement
        .query_map(params![workspace_key, keep], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for generation in generations {
        cleanup_generation(connection, workspace_key, generation)?;
    }
    // Vector retention mirrors the index policy: one published and at most
    // one deprecated generation. Failed/cancelled staging rows have no blobs.
    let keep_vector: Option<String> = connection
        .query_row(
            "SELECT index_id FROM workspace_vector_indexes
             WHERE workspace_key = ?1 AND build_status = 'deprecated'
             ORDER BY published_at DESC LIMIT 1",
            [workspace_key],
            |row| row.get(0),
        )
        .optional()?;
    connection.execute(
        "DELETE FROM workspace_vector_indexes
         WHERE workspace_key = ?1 AND build_status IN ('deprecated','failed','cancelled')
           AND index_id != COALESCE(?2, '')",
        params![workspace_key, keep_vector],
    )?;
    Ok(())
}

pub fn get_index_status(
    connection: &Connection,
    workspace_root: &Path,
) -> Result<IndexStatus, RagError> {
    let key = workspace_key(workspace_root)?;
    let row = connection
        .query_row(
            "SELECT generation, status, file_count, chunk_count, excluded_count, dirty, published_at
             FROM workspace_index_runs WHERE workspace_key = ?1
             ORDER BY CASE status WHEN 'running' THEN 0 WHEN 'published' THEN 1 ELSE 2 END,
                      generation DESC LIMIT 1",
            [&key],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?, row.get::<_, i64>(4)?, row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            },
        )
        .optional()?;
    let vector = connection
        .query_row(
            "SELECT index_id FROM workspace_vector_indexes
             WHERE workspace_key = ?1 AND build_status = 'published'",
            [&key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(match row {
        Some((generation, status, files, chunks, excluded, dirty, published_at)) => IndexStatus {
            workspace_key: key,
            generation: Some(generation),
            status,
            indexed_files: files as usize,
            chunks: chunks as usize,
            excluded: excluded as usize,
            dirty: dirty != 0,
            published_at,
            vector_mode: if vector.is_some() {
                "hybrid".into()
            } else {
                "fts5".into()
            },
            vector_index_id: vector,
        },
        None => IndexStatus {
            workspace_key: key,
            generation: None,
            status: "not_indexed".into(),
            indexed_files: 0,
            chunks: 0,
            excluded: 0,
            dirty: true,
            published_at: None,
            vector_mode: "fts5".into(),
            vector_index_id: None,
        },
    })
}

fn stable_id(workspace_key: &str, generation: i64, value: &str, kind: &str) -> String {
    format!(
        "{kind}-{}",
        &sha256_hex(format!("{workspace_key}:{generation}:{kind}:{value}").as_bytes())[..32]
    )
}

fn bounded_error(path: &str, error: &str) -> String {
    let code = if error.to_lowercase().contains("permission") {
        "permission_denied"
    } else if error.to_lowercase().contains("not found") {
        "file_not_found"
    } else if error.to_lowercase().contains("timeout") {
        "timeout"
    } else {
        "io_error"
    };
    format!("{}:{code}", path.chars().take(256).collect::<String>())
}

fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

pub fn search_workspace(
    connection: &Connection,
    workspace_root: &Path,
    query: &str,
    filters: QueryFilters,
    limits: &RetrievalLimits,
    hybrid: &HybridConfig,
) -> Result<SearchResult, RagError> {
    search_workspace_with_config(
        connection,
        workspace_root,
        query,
        filters,
        limits,
        hybrid,
        &LoopConfig::default(),
    )
}

pub fn search_workspace_with_config(
    connection: &Connection,
    workspace_root: &Path,
    query: &str,
    filters: QueryFilters,
    limits: &RetrievalLimits,
    hybrid: &HybridConfig,
    loop_config: &LoopConfig,
) -> Result<SearchResult, RagError> {
    search_workspace_with_progress(
        connection,
        workspace_root,
        query,
        filters,
        limits,
        hybrid,
        loop_config,
        |_| {},
    )
}

// Аргументы — параметры одного поиска: запрос, лимиты, фильтры и канал прогресса.
#[allow(clippy::too_many_arguments)]
pub fn search_workspace_with_progress(
    connection: &Connection,
    workspace_root: &Path,
    query: &str,
    filters: QueryFilters,
    limits: &RetrievalLimits,
    hybrid: &HybridConfig,
    loop_config: &LoopConfig,
    mut progress: impl FnMut(RetrievalProgress),
) -> Result<SearchResult, RagError> {
    limits.validate()?;
    validate_filters(&filters)?;
    if loop_config.max_iterations > 2
        || loop_config.wall_clock_timeout_ms > 30_000
        || loop_config.token_budget > 128_000
    {
        return Err(RagError::InvalidConfig(
            "agentic loop limits exceed hard bounds".into(),
        ));
    }
    let started = Instant::now();
    let key = workspace_key(workspace_root)?;
    let generation = active_generation(connection, &key)?.ok_or(RagError::IndexUnavailable)?;
    let mut plan = plan_query(query, filters)?;
    let query_id = format!(
        "query-{}",
        &sha256_hex(format!("{}:{}:{}", key, generation, query).as_bytes())[..24]
    );
    let planner_event = RetrievalProgress {
        event_type: "planner.started".into(),
        iteration: 0,
        strategy: plan.strategy.clone(),
        result_count: 0,
        coverage_millis: 0,
        reason_code: "deterministic_precheck".into(),
    };
    progress(planner_event.clone());
    let mut events = vec![planner_event];
    if loop_config.max_iterations == 0
        || loop_config.wall_clock_timeout_ms == 0
        || loop_config.token_budget == 0
    {
        let mut reached_limits: Vec<String> = Vec::new();
        if loop_config.max_iterations == 0 {
            reached_limits.push("iteration_limit".into());
        }
        if loop_config.wall_clock_timeout_ms == 0 {
            reached_limits.push("timeout".into());
        }
        if loop_config.token_budget == 0 {
            reached_limits.push("token_budget".into());
        }
        let stop_reason = reached_limits[0].clone();
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "loop.stopped".into(),
                iteration: 0,
                strategy: plan.strategy.clone(),
                result_count: 0,
                coverage_millis: 0,
                reason_code: stop_reason.clone(),
            },
        );
        let expansion_request = expansion_request(&plan.filters, "bounded_limit");
        return Ok(SearchResult {
            query_id,
            plan,
            evidence: Vec::new(),
            diagnostics: SearchDiagnostics {
                mode: "fts5".into(),
                fallback_reason: None,
                metrics_version: EVIDENCE_METRICS_VERSION.into(),
                iterations: 0,
                coverage: 0.0,
                stop_reason,
                result_count: 0,
                duration_ms: started.elapsed().as_millis() as u64,
                query_hash: sha256_hex(query.as_bytes()),
                conflict_flag: false,
                reached_limits,
                events,
            },
            uncertainty: Some("Данных workspace недостаточно: bounded loop не запущен".into()),
            expansion_request: Some(expansion_request),
        });
    }
    if !plan.need_search {
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "loop.stopped".into(),
                iteration: 0,
                strategy: plan.strategy.clone(),
                result_count: 0,
                coverage_millis: 0,
                reason_code: "search_not_needed".into(),
            },
        );
        return Ok(SearchResult {
            query_id,
            plan,
            evidence: Vec::new(),
            diagnostics: SearchDiagnostics {
                mode: "fts5".into(),
                fallback_reason: None,
                metrics_version: EVIDENCE_METRICS_VERSION.into(),
                iterations: 0,
                coverage: 0.0,
                stop_reason: "search_not_needed".into(),
                result_count: 0,
                duration_ms: started.elapsed().as_millis() as u64,
                query_hash: sha256_hex(query.as_bytes()),
                conflict_flag: false,
                reached_limits: Vec::new(),
                events,
            },
            uncertainty: None,
            expansion_request: None,
        });
    }

    let mut seen = HashSet::new();
    let mut evidence = Vec::new();
    let mut iterations = 0usize;
    let mut stop_reason = "evidence_sufficient".to_string();
    let mut coverage = 0.0;
    let retrieval_token_budget = loop_config.token_budget.saturating_mul(60) / 100;
    let mut consumed_tokens = 0usize;
    let mut reached_limits = Vec::new();
    for _ in 0..loop_config.max_iterations {
        if started.elapsed() >= Duration::from_millis(loop_config.wall_clock_timeout_ms) {
            stop_reason = "timeout".into();
            reached_limits.push("timeout".into());
            break;
        }
        if consumed_tokens >= retrieval_token_budget {
            stop_reason = "token_budget".into();
            reached_limits.push("token_budget".into());
            break;
        }
        let fingerprint = format!("{:?}:{}:{:?}", plan.strategy, plan.query, plan.filters);
        if !seen.insert(fingerprint) {
            stop_reason = "duplicate_rewrite".into();
            break;
        }
        iterations += 1;
        let remaining = Duration::from_millis(loop_config.wall_clock_timeout_ms)
            .saturating_sub(started.elapsed());
        evidence = match bounded_lexical_retrieval(
            connection,
            workspace_root,
            &key,
            generation,
            &plan,
            limits,
            remaining / 2,
        ) {
            Ok(evidence) => evidence,
            Err(RagError::Timeout)
                if started.elapsed() < Duration::from_millis(loop_config.wall_clock_timeout_ms) =>
            {
                let retry_remaining = Duration::from_millis(loop_config.wall_clock_timeout_ms)
                    .saturating_sub(started.elapsed());
                match bounded_lexical_retrieval(
                    connection,
                    workspace_root,
                    &key,
                    generation,
                    &plan,
                    limits,
                    retry_remaining,
                ) {
                    Ok(evidence) => evidence,
                    Err(_) => {
                        return Ok(retrieval_failure_result(
                            query_id,
                            plan,
                            query,
                            started,
                            events,
                            iterations,
                            "retrieval_error",
                            &mut progress,
                        ));
                    }
                }
            }
            Err(RagError::Sandbox(_)) => {
                return Ok(retrieval_failure_result(
                    query_id,
                    plan,
                    query,
                    started,
                    events,
                    iterations,
                    "security_rejected",
                    &mut progress,
                ));
            }
            Err(_) => {
                return Ok(retrieval_failure_result(
                    query_id,
                    plan,
                    query,
                    started,
                    events,
                    iterations,
                    "retrieval_error",
                    &mut progress,
                ));
            }
        };
        consumed_tokens = consumed_tokens.saturating_add(
            evidence
                .iter()
                .map(|item| estimate_tokens(item.content.as_deref().unwrap_or_default()))
                .sum::<usize>(),
        );
        coverage = checker_coverage(&plan, &evidence);
        for chunk in &mut evidence {
            chunk.checker_confidence = checker_confidence(&plan, chunk, coverage);
        }
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "retrieval.updated".into(),
                iteration: iterations,
                strategy: plan.strategy.clone(),
                result_count: evidence.len(),
                coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
                reason_code: if evidence.is_empty() {
                    "empty_result"
                } else {
                    "retrieved"
                }
                .into(),
            },
        );
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "checker.updated".into(),
                iteration: iterations,
                strategy: plan.strategy.clone(),
                result_count: evidence.len(),
                coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
                reason_code: if coverage >= 0.8 {
                    "sufficient"
                } else {
                    "low_coverage"
                }
                .into(),
            },
        );
        if evidence.is_empty() {
            stop_reason = "empty_result".into();
        } else if coverage >= 0.8 {
            break;
        } else {
            stop_reason = "low_coverage".into();
        }
        if iterations < loop_config.max_iterations {
            push_progress(
                &mut events,
                &mut progress,
                RetrievalProgress {
                    event_type: "rewrite.started".into(),
                    iteration: iterations + 1,
                    strategy: plan.strategy.clone(),
                    result_count: evidence.len(),
                    coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
                    reason_code: stop_reason.clone(),
                },
            );
            plan = rewrite_plan(&plan);
        }
    }

    let mut mode = "fts5".to_string();
    let mut fallback_reason = None;
    if hybrid.enabled && hybrid_allowed(&plan, hybrid) {
        match hybrid_retrieval(
            connection,
            workspace_root,
            &key,
            generation,
            query,
            &plan.filters,
            &evidence,
            limits,
        ) {
            Ok(Some(fused)) => {
                evidence = fused;
                mode = "hybrid".into();
            }
            Ok(None) => {
                mode = "fallback_fts5".into();
                fallback_reason = Some("vector_index_unavailable".into());
            }
            Err(_) => {
                mode = "fallback_fts5".into();
                fallback_reason = Some("vector_index_incompatible".into());
            }
        }
    }
    evidence.truncate(limits.max_evidence_chunks);
    let conflict_flag = unresolved_conflict(&evidence);
    if iterations >= loop_config.max_iterations && coverage < 0.8 {
        stop_reason = "iteration_limit".into();
        reached_limits.insert(0, "iteration_limit".into());
    }
    let uncertainty = if conflict_flag {
        stop_reason = "conflict_unresolved".into();
        Some("Источники workspace противоречат друг другу; конфликт нельзя разрешить молча".into())
    } else if evidence.is_empty() || coverage < 0.8 {
        Some("Данных workspace недостаточно для подтверждённого ответа".into())
    } else {
        None
    };
    let result_count = evidence.len();
    let expansion_request = uncertainty
        .as_ref()
        .map(|_| expansion_request(&plan.filters, &stop_reason));
    if expansion_request.is_some() {
        push_progress(
            &mut events,
            &mut progress,
            RetrievalProgress {
                event_type: "expansion.requested".into(),
                iteration: iterations,
                strategy: plan.strategy.clone(),
                result_count,
                coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
                reason_code: stop_reason.clone(),
            },
        );
    }
    push_progress(
        &mut events,
        &mut progress,
        RetrievalProgress {
            event_type: "loop.stopped".into(),
            iteration: iterations,
            strategy: plan.strategy.clone(),
            result_count,
            coverage_millis: (coverage.clamp(0.0, 1.0) * 1000.0).round() as u16,
            reason_code: stop_reason.clone(),
        },
    );
    Ok(SearchResult {
        query_id,
        plan,
        evidence,
        diagnostics: SearchDiagnostics {
            mode,
            fallback_reason,
            metrics_version: EVIDENCE_METRICS_VERSION.into(),
            iterations,
            coverage,
            stop_reason,
            result_count,
            duration_ms: started.elapsed().as_millis() as u64,
            query_hash: sha256_hex(query.as_bytes()),
            conflict_flag,
            reached_limits,
            events,
        },
        uncertainty,
        expansion_request,
    })
}

fn expansion_request(filters: &QueryFilters, reason: &str) -> ExpansionRequest {
    ExpansionRequest {
        request_type: "request_expansion".into(),
        suggested_path: filters.path.clone().unwrap_or_else(|| "src".into()),
        languages: filters.language.clone().into_iter().collect(),
        reason: reason.chars().take(64).collect(),
        estimated_iterations: 1,
        estimated_tokens: 1200,
        estimated_seconds: 5,
        requires_approval: true,
    }
}

fn push_progress(
    events: &mut Vec<RetrievalProgress>,
    progress: &mut impl FnMut(RetrievalProgress),
    event: RetrievalProgress,
) {
    progress(event.clone());
    events.push(event);
}

// Аргументы — поля bounded-результата отказа: причина, лимиты и диагностика.
#[allow(clippy::too_many_arguments)]
fn retrieval_failure_result(
    query_id: String,
    plan: QueryPlan,
    query: &str,
    started: Instant,
    mut events: Vec<RetrievalProgress>,
    iterations: usize,
    reason: &str,
    progress: &mut impl FnMut(RetrievalProgress),
) -> SearchResult {
    push_progress(
        &mut events,
        progress,
        RetrievalProgress {
            event_type: "loop.stopped".into(),
            iteration: iterations,
            strategy: plan.strategy.clone(),
            result_count: 0,
            coverage_millis: 0,
            reason_code: reason.into(),
        },
    );
    let expansion = expansion_request(&plan.filters, reason);
    SearchResult {
        query_id,
        plan,
        evidence: Vec::new(),
        diagnostics: SearchDiagnostics {
            mode: "fts5".into(),
            fallback_reason: Some(reason.into()),
            metrics_version: EVIDENCE_METRICS_VERSION.into(),
            iterations,
            coverage: 0.0,
            stop_reason: reason.into(),
            result_count: 0,
            duration_ms: started.elapsed().as_millis() as u64,
            query_hash: sha256_hex(query.as_bytes()),
            conflict_flag: false,
            reached_limits: Vec::new(),
            events,
        },
        uncertainty: Some("Данные workspace недоступны; утверждение не подтверждено".into()),
        expansion_request: Some(expansion),
    }
}

fn unresolved_conflict(evidence: &[RetrievedChunk]) -> bool {
    evidence.iter().enumerate().any(|(index, left)| {
        evidence.iter().skip(index + 1).any(|right| {
            left.source_id != right.source_id
                && left.symbol.is_some()
                && left.symbol == right.symbol
                && left.content_hash != right.content_hash
                && left.content.as_deref() != right.content.as_deref()
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn bounded_lexical_retrieval(
    connection: &Connection,
    workspace_root: &Path,
    workspace_key: &str,
    generation: i64,
    plan: &QueryPlan,
    limits: &RetrievalLimits,
    timeout: Duration,
) -> Result<Vec<RetrievedChunk>, RagError> {
    if timeout.is_zero() {
        return Err(RagError::Timeout);
    }
    let deadline = Instant::now() + timeout;
    connection.progress_handler(1_000, Some(move || Instant::now() >= deadline));
    let result = lexical_retrieval(
        connection,
        workspace_root,
        workspace_key,
        generation,
        plan,
        limits,
    );
    connection.progress_handler(0, None::<fn() -> bool>);
    match result {
        Err(RagError::Sqlite(error)) if error.to_string().to_lowercase().contains("interrupt") => {
            Err(RagError::Timeout)
        }
        other => other,
    }
}

fn lexical_retrieval(
    connection: &Connection,
    workspace_root: &Path,
    workspace_key: &str,
    generation: i64,
    plan: &QueryPlan,
    limits: &RetrievalLimits,
) -> Result<Vec<RetrievedChunk>, RagError> {
    let terms = bounded_terms(&plan.query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let match_expression = terms
        .iter()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");
    let path_filter = plan.filters.path.as_deref().unwrap_or("");
    let language_filter = plan.filters.language.as_deref().unwrap_or("");
    let column_query = match plan.strategy {
        QueryStrategy::ExactSymbol => format!("symbol_normalized : ({match_expression})"),
        QueryStrategy::Path => format!("path : ({match_expression})"),
        _ => match_expression,
    };
    let mut statement = connection.prepare(
        "SELECT c.chunk_id, c.document_id, d.path, d.language, c.byte_start,
                c.byte_end, c.chunk_hash, d.file_hash, c.chunk_text, c.symbol,
                c.parent_context, d.redaction_status, d.size_bytes,
                bm25(workspace_chunks_fts, 1.0, 2.0, 0.5, 0.25) AS rank_score,
                c.ordinal
         FROM workspace_chunks_fts
         JOIN document_chunks c ON c.chunk_id = workspace_chunks_fts.chunk_id
         JOIN workspace_documents d ON d.document_id = c.document_id
         WHERE workspace_chunks_fts MATCH ?1
           AND d.workspace_key = ?2 AND d.generation = ?3 AND d.status = 'active'
           AND d.is_secret_path = 0
           AND (?4 = '' OR d.path = ?4 OR d.path LIKE ?4 || '/%')
           AND (?5 = '' OR d.language = ?5)
         ORDER BY rank_score ASC, d.path COLLATE BINARY ASC, d.document_id ASC,
                  c.ordinal ASC, c.byte_start ASC
         LIMIT ?6",
    )?;
    let candidates = statement
        .query_map(
            params![
                column_query,
                workspace_key,
                generation,
                path_filter,
                language_filter,
                limits.max_retrieval_chunks as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, f64>(13)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let canonical_root = workspace_root.canonicalize()?;
    let mut output = Vec::new();
    for (rank, candidate) in candidates.into_iter().enumerate() {
        let (
            chunk_id,
            document_id,
            path,
            language,
            byte_start,
            byte_end,
            chunk_hash,
            file_hash,
            indexed_content,
            symbol,
            parent_context,
            redaction,
            indexed_size,
            bm25,
        ) = candidate;
        let mut matched_filters = vec![format!("workspace_key={workspace_key}")];
        if !path_filter.is_empty() {
            matched_filters.push(format!("path={path_filter}"));
        }
        if !language_filter.is_empty() {
            matched_filters.push(format!("language={language_filter}"));
        }
        let score = -bm25;
        let current_path = canonical_root.join(&path);
        let validation = validate_source(
            &canonical_root,
            &current_path,
            &file_hash,
            indexed_size as u64,
            byte_start as usize,
            byte_end as usize,
        );
        let (content, lines, stale) = match validation {
            Ok(bytes) if redaction != "full" => {
                match decode_source_range(&bytes, byte_start as usize, byte_end as usize) {
                    Ok((content, lines)) if content.trim() == indexed_content.trim() => {
                        (Some(content.trim().to_string()), Some(lines), false)
                    }
                    _ => (None, None, true),
                }
            }
            Ok(_) => (None, None, false),
            Err(_) => (None, None, true),
        };
        let term_frequencies = terms
            .iter()
            .map(|term| {
                let count = indexed_content
                    .to_lowercase()
                    .matches(&term.to_lowercase())
                    .count();
                (term.clone(), count)
            })
            .collect::<BTreeMap<_, _>>();
        output.push(RetrievedChunk {
            source_id: document_id,
            chunk_id,
            relative_path: path,
            language,
            byte_start: byte_start as u64,
            byte_end: byte_end as u64,
            lines,
            chunk_hash,
            content_hash: file_hash,
            content,
            symbol,
            parent_context,
            score,
            score_explanation: ScoreExplanation {
                algorithm: "bm25".into(),
                column_weights: BTreeMap::from([
                    ("content".into(), 1.0),
                    ("symbol_normalized".into(), 2.0),
                    ("canonical_path".into(), 0.5),
                ]),
                term_frequencies,
                document_length: indexed_content.len(),
                matched_filters,
                excluded_by: Vec::new(),
            },
            ranking_explanation: RankingExplanation {
                algorithm: "fts5".into(),
                lexical_rank: Some(rank + 1),
                vector_rank: None,
                rrf_rank: rank + 1,
                sources: vec!["lexical".into()],
            },
            stale,
            redaction_status: redaction,
            checker_confidence: 0.0,
        });
    }
    output.sort_by(deterministic_rank);
    output.truncate(limits.max_retrieval_chunks);
    Ok(output)
}

fn previous_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn validate_source(
    root: &Path,
    path: &Path,
    expected_hash: &str,
    expected_size: u64,
    byte_start: usize,
    byte_end: usize,
) -> Result<Vec<u8>, RagError> {
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(RagError::Sandbox("retrieval path escaped workspace".into()));
    }
    let bytes = fs::read(&canonical)?;
    if bytes.len() as u64 != expected_size
        || sha256_hex(&bytes) != expected_hash
        || byte_start > byte_end
        || byte_end > bytes.len()
    {
        return Err(RagError::InvalidWorkspace("stale source snapshot".into()));
    }
    Ok(bytes)
}

fn decode_source_range(
    bytes: &[u8],
    byte_start: usize,
    byte_end: usize,
) -> Result<(String, [u64; 2]), RagError> {
    if byte_start > byte_end || byte_end > bytes.len() {
        return Err(RagError::InvalidWorkspace(
            "invalid source byte range".into(),
        ));
    }
    let (content, prefix) = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff])
    {
        let little_endian = bytes.starts_with(&[0xff, 0xfe]);
        let decode = |slice: &[u8]| -> Result<String, RagError> {
            if !slice.len().is_multiple_of(2) {
                return Err(RagError::InvalidWorkspace(
                    "unaligned UTF-16 byte range".into(),
                ));
            }
            let units = slice
                .chunks_exact(2)
                .map(|pair| {
                    if little_endian {
                        u16::from_le_bytes([pair[0], pair[1]])
                    } else {
                        u16::from_be_bytes([pair[0], pair[1]])
                    }
                })
                .collect::<Vec<_>>();
            String::from_utf16(&units)
                .map_err(|_| RagError::InvalidWorkspace("invalid UTF-16 source range".into()))
        };
        let start = byte_start.max(2);
        let end = byte_end.max(2);
        (decode(&bytes[start..end])?, decode(&bytes[2..start])?)
    } else {
        (
            String::from_utf8_lossy(&bytes[byte_start..byte_end]).into_owned(),
            String::from_utf8_lossy(&bytes[..byte_start]).into_owned(),
        )
    };
    let start_line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u64 + 1;
    let end_line = start_line + content.bytes().filter(|byte| *byte == b'\n').count() as u64;
    Ok((content, [start_line, end_line]))
}

fn deterministic_rank(left: &RetrievedChunk, right: &RetrievedChunk) -> Ordering {
    let score = if (left.score - right.score).abs() <= 1e-9 {
        Ordering::Equal
    } else {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    };
    score
        .then_with(|| {
            left.relative_path
                .as_bytes()
                .cmp(right.relative_path.as_bytes())
        })
        .then_with(|| left.source_id.cmp(&right.source_id))
        .then_with(|| left.byte_start.cmp(&right.byte_start))
        .then_with(|| left.chunk_id.cmp(&right.chunk_id))
}

fn checker_coverage(plan: &QueryPlan, evidence: &[RetrievedChunk]) -> f64 {
    if evidence.is_empty() {
        return 0.0;
    }
    match plan.strategy {
        QueryStrategy::ExactSymbol => evidence.iter().any(|item| {
            item.symbol.as_ref().is_some_and(|symbol| {
                normalize_identifier(symbol, &item.language)
                    .contains(&normalize_identifier(&plan.query, &item.language))
            })
        }) as u8 as f64,
        QueryStrategy::Path => {
            let matching = evidence
                .iter()
                .filter(|item| {
                    item.relative_path
                        .to_lowercase()
                        .contains(&plan.query.to_lowercase())
                })
                .count();
            matching as f64 / evidence.len() as f64
        }
        QueryStrategy::Metadata => evidence.iter().any(|item| {
            plan.filters
                .language
                .as_ref()
                .is_none_or(|language| &item.language == language)
                && plan
                    .filters
                    .path
                    .as_ref()
                    .is_none_or(|path| item.relative_path.starts_with(path))
        }) as u8 as f64,
        QueryStrategy::Lexical => {
            let terms = bounded_terms(&plan.query);
            let combined = evidence
                .iter()
                .filter_map(|item| item.content.as_deref())
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            let matched = terms
                .iter()
                .filter(|term| combined.contains(&term.to_lowercase()))
                .count();
            matched as f64 / terms.len().max(1) as f64
        }
    }
}

fn checker_confidence(plan: &QueryPlan, chunk: &RetrievedChunk, coverage: f64) -> f64 {
    if chunk.stale || chunk.redaction_status == "full" || chunk.content.is_none() {
        return 0.0;
    }
    let score_gate = if chunk.score.is_finite() { 1.0 } else { 0.0 };
    let strategy_gate = match plan.strategy {
        QueryStrategy::Lexical => coverage,
        QueryStrategy::ExactSymbol => chunk.symbol.is_some() as u8 as f64,
        QueryStrategy::Path | QueryStrategy::Metadata => coverage,
    };
    (score_gate * 0.2 + strategy_gate * 0.8).clamp(0.0, 1.0)
}

fn rewrite_plan(plan: &QueryPlan) -> QueryPlan {
    let mut rewritten = plan.clone();
    rewritten.strategy = match plan.strategy {
        QueryStrategy::ExactSymbol => QueryStrategy::Lexical,
        QueryStrategy::Lexical => {
            if plan.filters.path.is_some() || plan.filters.language.is_some() {
                QueryStrategy::Metadata
            } else {
                QueryStrategy::Path
            }
        }
        QueryStrategy::Path | QueryStrategy::Metadata => QueryStrategy::Lexical,
    };
    rewritten.reason = "low_coverage_rewrite".into();
    rewritten.confidence = (rewritten.confidence - 0.2).max(0.0);
    rewritten
}

fn hybrid_allowed(plan: &QueryPlan, config: &HybridConfig) -> bool {
    let language = plan.filters.language.as_deref();
    let path = plan.filters.path.as_deref();
    (config.allowed_languages.is_empty()
        || language.is_some_and(|value| {
            config
                .allowed_languages
                .iter()
                .any(|allowed| allowed == value)
        }))
        && (config.allowed_path_prefixes.is_empty()
            || path.is_some_and(|value| {
                config
                    .allowed_path_prefixes
                    .iter()
                    .any(|allowed| value.starts_with(allowed))
            }))
}

pub fn build_vector_index(
    connection: &mut Connection,
    workspace_root: &Path,
    config: &HybridConfig,
    cancelled: impl Fn() -> bool,
) -> Result<Option<String>, RagError> {
    if !config.enabled {
        return Ok(None);
    }
    if !(1_000..=10 * 60 * 1000).contains(&config.build_timeout_ms)
        || !(1024 * 1024..=2 * 1024 * 1024 * 1024).contains(&config.max_build_bytes)
    {
        return Err(RagError::InvalidConfig(
            "hybrid resource limits are invalid".into(),
        ));
    }
    let started = Instant::now();
    let key = workspace_key(workspace_root)?;
    let generation = active_generation(connection, &key)?.ok_or(RagError::IndexUnavailable)?;
    let index_id = format!(
        "vector-{}-{}",
        generation,
        &sha256_hex(format!("{key}:{generation}:{LOCAL_EMBEDDING_VERSION}").as_bytes())[..16]
    );
    connection.execute(
        "INSERT OR REPLACE INTO workspace_vector_indexes
         (index_id, workspace_key, source_generation, embedding_model_id,
          embedding_model_version, vector_dimension, distance_metric, normalization,
          chunker_version, build_status, created_at, vector_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'cosine', 'l2', ?7, 'building', ?8, 0)",
        params![
            index_id,
            key,
            generation,
            LOCAL_EMBEDDING_MODEL,
            LOCAL_EMBEDDING_VERSION,
            VECTOR_DIMENSION as i64,
            CHUNKER_VERSION,
            now_ms()
        ],
    )?;
    connection.execute(
        "DELETE FROM workspace_chunk_vectors WHERE index_id = ?1",
        [&index_id],
    )?;
    let rows = {
        let mut statement = connection.prepare(
            "SELECT c.chunk_id, c.chunk_text, d.language, d.path
             FROM document_chunks c JOIN workspace_documents d ON d.document_id = c.document_id
             WHERE c.workspace_key = ?1 AND c.generation = ?2 AND d.status = 'active'
               AND d.redaction_status != 'full' AND d.is_secret_path = 0
             ORDER BY d.path, c.ordinal",
        )?;
        let collected = statement
            .query_map(params![key, generation], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        collected
    };
    let mut bytes = 0u64;
    for (chunk_id, text, language, path) in &rows {
        if cancelled() {
            fail_vector_build(connection, &index_id, "cancelled")?;
            return Err(RagError::Cancelled);
        }
        if started.elapsed() > Duration::from_millis(config.build_timeout_ms) {
            fail_vector_build(connection, &index_id, "failed")?;
            return Err(RagError::Timeout);
        }
        if !config.allowed_languages.is_empty() && !config.allowed_languages.contains(language) {
            continue;
        }
        if !config.allowed_path_prefixes.is_empty()
            && !config
                .allowed_path_prefixes
                .iter()
                .any(|prefix| path.starts_with(prefix))
        {
            continue;
        }
        let vector = embed_local(text);
        let blob = encode_vector(&vector);
        bytes += blob.len() as u64;
        if bytes > config.max_build_bytes {
            fail_vector_build(connection, &index_id, "failed")?;
            return Err(RagError::InvalidConfig(
                "vector build memory budget exceeded".into(),
            ));
        }
        connection.execute(
            "INSERT INTO workspace_chunk_vectors(index_id, chunk_id, vector) VALUES (?1, ?2, ?3)",
            params![index_id, chunk_id, blob],
        )?;
    }
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM workspace_chunk_vectors WHERE index_id = ?1",
        [&index_id],
        |row| row.get(0),
    )?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE workspace_vector_indexes SET build_status = 'ready', vector_count = ?2
         WHERE index_id = ?1 AND build_status = 'building'",
        params![index_id, count],
    )?;
    transaction.execute(
        "UPDATE workspace_vector_indexes SET build_status = 'deprecated'
         WHERE workspace_key = ?1 AND build_status = 'published'",
        [&key],
    )?;
    transaction.execute(
        "UPDATE workspace_vector_indexes SET build_status = 'published', published_at = ?2
         WHERE index_id = ?1 AND build_status = 'ready'",
        params![index_id, now_ms()],
    )?;
    transaction.commit()?;
    Ok(Some(index_id))
}

fn fail_vector_build(
    connection: &Connection,
    index_id: &str,
    status: &str,
) -> Result<(), RagError> {
    connection.execute(
        "DELETE FROM workspace_chunk_vectors WHERE index_id = ?1",
        [index_id],
    )?;
    connection.execute(
        "UPDATE workspace_vector_indexes SET build_status = ?2 WHERE index_id = ?1",
        params![index_id, status],
    )?;
    Ok(())
}

fn embed_local(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0f32; VECTOR_DIMENSION];
    for term in bounded_terms(text) {
        let hash = digest(&SHA256, term.to_lowercase().as_bytes());
        let bytes = hash.as_ref();
        let index = u16::from_le_bytes([bytes[0], bytes[1]]) as usize % VECTOR_DIMENSION;
        let sign = if bytes[2] & 1 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        vector.iter_mut().for_each(|value| *value /= norm);
    }
    vector
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn decode_vector(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() != VECTOR_DIMENSION * 4 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

// Аргументы — параметры гибридного поиска: запрос, веса, лимиты и диагностика.
#[allow(clippy::too_many_arguments)]
fn hybrid_retrieval(
    connection: &Connection,
    workspace_root: &Path,
    workspace_key: &str,
    generation: i64,
    query: &str,
    filters: &QueryFilters,
    lexical: &[RetrievedChunk],
    limits: &RetrievalLimits,
) -> Result<Option<Vec<RetrievedChunk>>, RagError> {
    let metadata = connection
        .query_row(
            "SELECT index_id, source_generation, embedding_model_id, embedding_model_version,
                    vector_dimension, distance_metric, normalization, chunker_version
             FROM workspace_vector_indexes
             WHERE workspace_key = ?1 AND build_status = 'published'",
            [workspace_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((
        index_id,
        source_generation,
        model,
        version,
        dimension,
        metric,
        normalization,
        chunker,
    )) = metadata
    else {
        return Ok(None);
    };
    if source_generation != generation
        || model != LOCAL_EMBEDDING_MODEL
        || version != LOCAL_EMBEDDING_VERSION
        || dimension != VECTOR_DIMENSION as i64
        || metric != "cosine"
        || normalization != "l2"
        || chunker != CHUNKER_VERSION
    {
        return Err(RagError::InvalidConfig("vector_index_incompatible".into()));
    }
    let query_vector = embed_local(query);
    let mut statement = connection.prepare(
        "SELECT v.chunk_id, v.vector FROM workspace_chunk_vectors v
         JOIN document_chunks c ON c.chunk_id = v.chunk_id
         JOIN workspace_documents d ON d.document_id = c.document_id
         WHERE v.index_id = ?1 AND c.workspace_key = ?2 AND c.generation = ?3
           AND d.status = 'active' AND d.redaction_status != 'full' AND d.is_secret_path = 0
           AND (?4 = '' OR d.path = ?4 OR d.path LIKE ?4 || '/%')
           AND (?5 = '' OR d.language = ?5)",
    )?;
    let path_filter = filters.path.as_deref().unwrap_or("");
    let language_filter = filters.language.as_deref().unwrap_or("");
    let mut vector_scores = statement
        .query_map(
            params![
                index_id,
                workspace_key,
                generation,
                path_filter,
                language_filter
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )?
        .filter_map(Result::ok)
        .filter_map(|(id, blob)| {
            decode_vector(&blob).map(|vector| (id, dot(&query_vector, &vector)))
        })
        .collect::<Vec<_>>();
    vector_scores.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    vector_scores.truncate(limits.max_retrieval_chunks);
    let vector_ranks = vector_scores
        .iter()
        .enumerate()
        .map(|(index, (id, _))| (id.clone(), index + 1))
        .collect::<HashMap<_, _>>();
    let lexical_ranks = lexical
        .iter()
        .enumerate()
        .map(|(index, item)| (item.chunk_id.clone(), index + 1))
        .collect::<HashMap<_, _>>();
    let mut fused = lexical.to_vec();
    let lexical_ids = lexical
        .iter()
        .map(|item| item.chunk_id.as_str())
        .collect::<HashSet<_>>();
    for (chunk_id, _) in &vector_scores {
        if !lexical_ids.contains(chunk_id.as_str()) {
            if let Some(candidate) = load_vector_candidate(
                connection,
                workspace_root,
                workspace_key,
                generation,
                chunk_id,
                filters,
            )? {
                fused.push(candidate);
            }
        }
    }
    for item in &mut fused {
        let lexical_rank = lexical_ranks.get(&item.chunk_id).copied();
        let vector_rank = vector_ranks.get(&item.chunk_id).copied();
        let rrf = lexical_rank
            .map(|rank| 1.0 / (RRF_K + rank as f64))
            .unwrap_or(0.0)
            + vector_rank
                .map(|rank| 1.0 / (RRF_K + rank as f64))
                .unwrap_or(0.0);
        item.score = rrf;
        item.ranking_explanation = RankingExplanation {
            algorithm: "rrf".into(),
            lexical_rank,
            vector_rank,
            rrf_rank: 0,
            sources: [
                lexical_rank.map(|_| "lexical".to_string()),
                vector_rank.map(|_| "vector".to_string()),
            ]
            .into_iter()
            .flatten()
            .collect(),
        };
    }
    fused.sort_by(deterministic_rank);
    for (index, item) in fused.iter_mut().enumerate() {
        item.ranking_explanation.rrf_rank = index + 1;
    }
    fused.truncate(limits.max_retrieval_chunks);
    Ok(Some(fused))
}

fn load_vector_candidate(
    connection: &Connection,
    workspace_root: &Path,
    workspace_key: &str,
    generation: i64,
    chunk_id: &str,
    filters: &QueryFilters,
) -> Result<Option<RetrievedChunk>, RagError> {
    let row = connection
        .query_row(
            "SELECT c.document_id, d.path, d.language, c.byte_start, c.byte_end,
                    c.chunk_hash, d.file_hash, c.chunk_text, c.symbol,
                    c.parent_context, d.redaction_status, d.size_bytes
             FROM document_chunks c
             JOIN workspace_documents d ON d.document_id = c.document_id
             WHERE c.workspace_key = ?1 AND c.generation = ?2 AND c.chunk_id = ?3
               AND d.status = 'active' AND d.redaction_status != 'full'
               AND d.is_secret_path = 0
               AND (?4 = '' OR d.path = ?4 OR d.path LIKE ?4 || '/%')
               AND (?5 = '' OR d.language = ?5)",
            params![
                workspace_key,
                generation,
                chunk_id,
                filters.path.as_deref().unwrap_or(""),
                filters.language.as_deref().unwrap_or("")
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            },
        )
        .optional()?;
    let Some((
        source_id,
        path,
        language,
        byte_start,
        byte_end,
        chunk_hash,
        file_hash,
        indexed_content,
        symbol,
        parent_context,
        redaction_status,
        indexed_size,
    )) = row
    else {
        return Ok(None);
    };
    let root = workspace_root.canonicalize()?;
    let validation = validate_source(
        &root,
        &root.join(&path),
        &file_hash,
        indexed_size as u64,
        byte_start as usize,
        byte_end as usize,
    );
    let (content, lines, stale) = match validation {
        Ok(bytes) => match decode_source_range(&bytes, byte_start as usize, byte_end as usize) {
            Ok((content, lines)) if content.trim() == indexed_content.trim() => {
                (Some(content.trim().to_string()), Some(lines), false)
            }
            _ => (None, None, true),
        },
        Err(_) => (None, None, true),
    };
    Ok(Some(RetrievedChunk {
        source_id,
        chunk_id: chunk_id.to_string(),
        relative_path: path,
        language,
        byte_start: byte_start as u64,
        byte_end: byte_end as u64,
        lines,
        chunk_hash,
        content_hash: file_hash,
        content,
        symbol,
        parent_context,
        score: 0.0,
        score_explanation: ScoreExplanation {
            algorithm: "vector".into(),
            column_weights: BTreeMap::new(),
            term_frequencies: BTreeMap::new(),
            document_length: indexed_content.len(),
            matched_filters: vec![format!("workspace_key={workspace_key}")],
            excluded_by: Vec::new(),
        },
        ranking_explanation: RankingExplanation {
            algorithm: "vector".into(),
            lexical_rank: None,
            vector_rank: None,
            rrf_rank: 0,
            sources: vec!["vector".into()],
        },
        stale,
        redaction_status,
        checker_confidence: 0.0,
    }))
}

fn dot(left: &[f32], right: &[f32]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(a, b)| (*a as f64) * (*b as f64))
        .sum()
}

pub fn build_evidence_context(
    connection: &Connection,
    workspace_root: &Path,
    search: &SearchResult,
    token_budget: usize,
    chunk_count_limit: usize,
    min_chunk_size_tokens: usize,
) -> Result<ContextBuildResult, RagError> {
    if token_budget == 0 || chunk_count_limit == 0 {
        return Ok(ContextBuildResult {
            ledger_id: format!(
                "rag-ledger-{}",
                &sha256_hex(search.query_id.as_bytes())[..20]
            ),
            model_context: String::new(),
            selected_block_ids: Vec::new(),
            citations: Vec::new(),
            rejected: vec!["empty_budget".into()],
            degraded: false,
            estimated_tokens: 0,
        });
    }
    if chunk_count_limit > 64 || token_budget > 128_000 || min_chunk_size_tokens > token_budget {
        return Err(RagError::InvalidConfig(
            "context budget is outside hard limits".into(),
        ));
    }
    let canonical_root = workspace_root.canonicalize()?;
    let ledger_id = format!(
        "rag-ledger-{}",
        &sha256_hex(format!("{}:{}", search.query_id, now_ms()).as_bytes())[..24]
    );
    let mut candidates = search.evidence.clone();
    candidates.sort_by(|left, right| {
        let left_score = left.score + left.checker_confidence;
        let right_score = right.score + right.checker_confidence;
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.chunk_id.cmp(&right.chunk_id))
    });
    let mut selected = Vec::new();
    let mut citations = Vec::new();
    let mut rejected = Vec::new();
    let mut context = String::new();
    let mut used_tokens = 0usize;
    let created_at = now_ms();
    for (rank, block) in candidates.iter().enumerate() {
        if selected.len() >= chunk_count_limit {
            rejected.push(format!("{}:chunk_count_limit", block.chunk_id));
            break;
        }
        if block.stale || block.redaction_status == "full" || block.content.is_none() {
            rejected.push(format!("{}:stale_or_redacted", block.chunk_id));
            continue;
        }
        let current_path = canonical_root.join(&block.relative_path);
        let initial = validate_source(
            &canonical_root,
            &current_path,
            &block.content_hash,
            fs::metadata(&current_path)
                .map(|metadata| metadata.len())
                .unwrap_or_default(),
            block.byte_start as usize,
            block.byte_end as usize,
        );
        if initial.is_err() {
            rejected.push(format!("{}:sandbox_or_stale", block.chunk_id));
            continue;
        }
        let snippet = with_parent_context(&current_path, block)?;
        let snippet_tokens = estimate_tokens(&snippet);
        let remaining = token_budget.saturating_sub(used_tokens);
        if remaining < min_chunk_size_tokens || snippet_tokens > remaining {
            rejected.push(format!("{}:budget_exhausted", block.chunk_id));
            break;
        }
        let citation = Citation {
            citation_format_version: CITATION_FORMAT_VERSION,
            id: block.chunk_id.clone(),
            path: block.relative_path.clone(),
            line_range: block.lines,
            chunk_hash: block.chunk_hash.clone(),
            status: CitationStatus::Valid,
            reason: "ranked_evidence".into(),
        };
        context.push_str(&citation.compact());
        context.push('\n');
        context.push_str(&snippet);
        context.push_str("\n\n");
        used_tokens += snippet_tokens + estimate_tokens(&citation.compact());
        selected.push(block.chunk_id.clone());
        citations.push(citation.clone());
        write_rag_ledger(
            connection,
            &ledger_id,
            &search.query_id,
            block,
            rank + 1,
            &snippet,
            &citation,
            "initial_valid",
            None,
            created_at,
        )?;
    }
    Ok(ContextBuildResult {
        ledger_id,
        model_context: context,
        selected_block_ids: selected,
        citations,
        rejected,
        degraded: false,
        estimated_tokens: used_tokens,
    })
}

fn with_parent_context(path: &Path, block: &RetrievedChunk) -> Result<String, RagError> {
    let bytes = fs::read(path)?;
    let (text, _, _) = decode_text(&bytes);
    let lines = text.lines().collect::<Vec<_>>();
    let range = block.lines.unwrap_or([1, 1]);
    let logical = block.symbol.is_some();
    let radius = if logical { 2 } else { 3 };
    let start = range[0].saturating_sub(1 + radius).min(lines.len() as u64) as usize;
    let end = (range[1] + radius).min(lines.len() as u64) as usize;
    let mut result = String::new();
    result.push_str(&format!(
        "<source path=\"{}\" parent=\"{}\">\n",
        block.relative_path, block.parent_context
    ));
    for (index, line) in lines[start..end].iter().enumerate() {
        result.push_str(&format!("{}: {}\n", start + index + 1, line));
    }
    result.push_str("</source>");
    Ok(result)
}

/// Final atomic re-read immediately before answer rendering. Updated text and
/// metadata are accepted together; stale evidence is removed from the model
/// context and cannot retain a `valid` citation.
pub fn finalize_citations(
    connection: &Connection,
    workspace_root: &Path,
    search: &SearchResult,
    mut context: ContextBuildResult,
) -> Result<ContextBuildResult, RagError> {
    let root = workspace_root.canonicalize()?;
    let selected = search
        .evidence
        .iter()
        .filter(|block| context.selected_block_ids.contains(&block.chunk_id))
        .map(|block| (block.chunk_id.clone(), block))
        .collect::<HashMap<_, _>>();
    let mut valid_context = String::new();
    let mut stale = 0usize;
    for citation in &mut context.citations {
        let Some(block) = selected.get(&citation.id).copied() else {
            citation.status = CitationStatus::Stale;
            citation.reason = "missing_selected_block".into();
            stale += 1;
            continue;
        };
        let path = root.join(&block.relative_path);
        let final_result = fs::read(&path).map_err(RagError::from).and_then(|bytes| {
            let hash = sha256_hex(&bytes);
            if hash == block.content_hash {
                Ok((
                    bytes,
                    CitationStatus::Valid,
                    block.chunk_hash.clone(),
                    block.lines,
                ))
            } else {
                relocate_nearby(&bytes, block)
                    .map(|(hash, lines)| (bytes, CitationStatus::Updated, hash, Some(lines)))
            }
        });
        match final_result {
            Ok((_bytes, status, chunk_hash, lines)) => {
                citation.status = status;
                citation.chunk_hash = chunk_hash;
                citation.line_range = lines;
                citation.reason = if citation.status == CitationStatus::Updated {
                    "reread_updated".into()
                } else {
                    "reread_valid".into()
                };
                let refreshed = if citation.status == CitationStatus::Updated {
                    let mut refreshed = block.clone();
                    refreshed.lines = lines;
                    with_parent_context(&path, &refreshed)?
                } else {
                    with_parent_context(&path, block)?
                };
                valid_context.push_str(&citation.compact());
                valid_context.push('\n');
                valid_context.push_str(&refreshed);
                valid_context.push_str("\n\n");
                update_rag_ledger(
                    connection,
                    &context.ledger_id,
                    &citation.id,
                    citation,
                    "reread_valid",
                    None,
                )?;
            }
            Err(error) => {
                citation.status = CitationStatus::Stale;
                citation.reason = "reread_failed".into();
                stale += 1;
                let code = bounded_error(&citation.path, &error.to_string());
                update_rag_ledger(
                    connection,
                    &context.ledger_id,
                    &citation.id,
                    citation,
                    "reread_stale",
                    Some(&code),
                )?;
            }
        }
    }
    context.model_context = valid_context;
    context.selected_block_ids = context
        .citations
        .iter()
        .filter(|citation| citation.status != CitationStatus::Stale)
        .map(|citation| citation.id.clone())
        .collect();
    context.estimated_tokens = estimate_tokens(&context.model_context);
    context.degraded = !context.citations.is_empty() && stale * 2 > context.citations.len();
    if context.degraded {
        context.rejected.push("stale_majority".into());
    }
    Ok(context)
}

fn relocate_nearby(bytes: &[u8], block: &RetrievedChunk) -> Result<(String, [u64; 2]), RagError> {
    let (text, _, _) = decode_text(bytes);
    let needle = block.content.as_deref().unwrap_or_default().trim();
    if needle.is_empty() {
        return Err(RagError::InvalidWorkspace("stale empty chunk".into()));
    }
    let Some(byte_start) = text.find(needle) else {
        return Err(RagError::InvalidWorkspace("stale chunk not found".into()));
    };
    let new_line = byte_to_line(&text, byte_start);
    let old_line = block.lines.map(|range| range[0]).unwrap_or(new_line);
    if new_line.abs_diff(old_line) > 5 {
        return Err(RagError::InvalidWorkspace(
            "chunk moved beyond reread window".into(),
        ));
    }
    let payload = format!(
        "{CHUNKER_VERSION}\n{}\n{}\n{needle}",
        block.language, block.parent_context
    );
    Ok((
        sha256_hex(payload.as_bytes()),
        [new_line, byte_to_line(&text, byte_start + needle.len())],
    ))
}

#[allow(clippy::too_many_arguments)]
fn write_rag_ledger(
    connection: &Connection,
    ledger_id: &str,
    query_id: &str,
    block: &RetrievedChunk,
    rank: usize,
    snippet: &str,
    citation: &Citation,
    reread_result: &str,
    error_code: Option<&str>,
    created_at: i64,
) -> Result<(), RagError> {
    connection.execute(
        "INSERT INTO rag_context_ledger
         (ledger_id, query_id, block_id, rank, retrieval_score, checker_confidence,
          chunk_hash, snippet_hash, path, line_start, line_end, citation_status,
          selection_reason, reread_result, error_code, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                 ?13, ?14, ?15, ?16)",
        params![
            ledger_id,
            query_id,
            block.chunk_id,
            rank as i64,
            block.score,
            block.checker_confidence,
            citation.chunk_hash,
            sha256_hex(snippet.as_bytes()),
            citation.path,
            citation.line_range.map(|range| range[0] as i64),
            citation.line_range.map(|range| range[1] as i64),
            citation.status.as_str(),
            citation.reason,
            reread_result,
            error_code,
            created_at
        ],
    )?;
    Ok(())
}

fn update_rag_ledger(
    connection: &Connection,
    ledger_id: &str,
    block_id: &str,
    citation: &Citation,
    reread_result: &str,
    error_code: Option<&str>,
) -> Result<(), RagError> {
    connection.execute(
        "UPDATE rag_context_ledger SET chunk_hash = ?3, line_start = ?4,
         line_end = ?5, citation_status = ?6, selection_reason = ?7,
         reread_result = ?8, error_code = ?9
         WHERE ledger_id = ?1 AND block_id = ?2",
        params![
            ledger_id,
            block_id,
            citation.chunk_hash,
            citation.line_range.map(|range| range[0] as i64),
            citation.line_range.map(|range| range[1] as i64),
            citation.status.as_str(),
            citation.reason,
            reread_result,
            error_code
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RagLedgerProjection {
    pub ledger_id: String,
    pub query_id: String,
    pub block_id: String,
    pub rank: usize,
    pub chunk_hash: String,
    pub snippet_hash: String,
    pub path: String,
    pub line_range: Option<[u64; 2]>,
    pub citation_status: String,
    pub selection_reason: String,
    pub reread_result: String,
    pub error_code: Option<String>,
}

pub fn rag_ledger_projection(
    connection: &Connection,
    query_id: &str,
    limit: usize,
) -> Result<Vec<RagLedgerProjection>, RagError> {
    if !(1..=100).contains(&limit) {
        return Err(RagError::InvalidConfig(
            "ledger projection limit must be 1..100".into(),
        ));
    }
    let mut statement = connection.prepare(
        "SELECT ledger_id, query_id, block_id, rank, chunk_hash, snippet_hash,
                path, line_start, line_end, citation_status, selection_reason,
                reread_result, error_code
         FROM rag_context_ledger WHERE query_id = ?1 ORDER BY rank LIMIT ?2",
    )?;
    let records = statement
        .query_map(params![query_id, limit as i64], |row| {
            let start = row.get::<_, Option<i64>>(7)?;
            let end = row.get::<_, Option<i64>>(8)?;
            Ok(RagLedgerProjection {
                ledger_id: row.get(0)?,
                query_id: row.get(1)?,
                block_id: row.get(2)?,
                rank: row.get::<_, i64>(3)? as usize,
                chunk_hash: row.get(4)?,
                snippet_hash: row.get(5)?,
                path: row.get(6)?,
                line_range: start.zip(end).map(|(a, b)| [a as u64, b as u64]),
                citation_status: row.get(9)?,
                selection_reason: row.get(10)?,
                reread_result: row.get(11)?,
                error_code: row.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(records)
}

/// Validates document provenance submitted by Memory Extraction against the
/// currently published RAG generation and a fresh filesystem read. This is
/// the only path that upgrades document evidence from `pending/unknown` to a
/// verified candidate; stale citations remain pending.
pub fn verify_document_provenance(
    connection: &Connection,
    workspace_root: &Path,
    relative_path: &str,
    chunk_hash: &str,
) -> Result<bool, RagError> {
    validate_filters(&QueryFilters {
        path: Some(relative_path.to_string()),
        language: None,
    })?;
    let key = workspace_key(workspace_root)?;
    let Some(generation) = active_generation(connection, &key)? else {
        return Ok(false);
    };
    let source = connection
        .query_row(
            "SELECT d.file_hash, d.size_bytes, c.byte_start, c.byte_end
             FROM document_chunks c
             JOIN workspace_documents d ON d.document_id = c.document_id
             WHERE d.workspace_key = ?1 AND d.generation = ?2 AND d.path = ?3
               AND c.chunk_hash = ?4 AND d.status = 'active'
               AND d.redaction_status != 'full' AND d.is_secret_path = 0",
            params![
                key,
                generation,
                relative_path.replace('\\', "/"),
                chunk_hash
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((file_hash, size, start, end)) = source else {
        return Ok(false);
    };
    let root = workspace_root.canonicalize()?;
    Ok(validate_source(
        &root,
        &root.join(relative_path),
        &file_hash,
        size as u64,
        start as usize,
        end as usize,
    )
    .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_local_storage::LocalDatabase;

    struct Fixture {
        root: PathBuf,
        database_path: PathBuf,
        database: LocalDatabase,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let suffix = format!("{}-{}", std::process::id(), now_ms());
            let root = std::env::temp_dir().join(format!("evohime-rag-{name}-{suffix}"));
            fs::create_dir_all(root.join("src")).unwrap();
            let database_path =
                std::env::temp_dir().join(format!("evohime-rag-{name}-{suffix}.db"));
            let database = LocalDatabase::open(&database_path).unwrap();
            Self {
                root,
                database_path,
                database,
            }
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, content).unwrap();
        }

        fn index(&mut self) -> IndexSummary {
            index_workspace(
                self.database.connection_mut(),
                &self.root,
                &IndexConfig::default(),
                false,
                || false,
                |_| {},
            )
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
            let _ = fs::remove_file(&self.database_path);
            let _ = fs::remove_file(self.database_path.with_extension("db-wal"));
            let _ = fs::remove_file(self.database_path.with_extension("db-shm"));
        }
    }

    #[test]
    fn planner_is_deterministic_and_rejects_scope_escape() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../schemas/workspace-query-plan.schema.json"))
                .unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["confidence"]["maximum"], 1.0);
        let plan = plan_query(
            "UserAuthManager.validateToken",
            QueryFilters {
                path: None,
                language: None,
            },
        )
        .unwrap();
        assert_eq!(plan.strategy, QueryStrategy::ExactSymbol);
        assert_eq!(
            normalize_identifier("MyClass::method()", "java"),
            "myclass::method"
        );
        assert!(plan_query(
            "needle",
            QueryFilters {
                path: Some("../secret".into()),
                language: None
            },
        )
        .is_err());
        assert!(plan_query(
            "needle",
            QueryFilters {
                path: Some("src%".into()),
                language: None
            },
        )
        .is_err());
        let invalid = QueryPlan {
            need_search: true,
            strategy: QueryStrategy::ExactSymbol,
            query: String::new(),
            filters: QueryFilters {
                path: None,
                language: None,
            },
            reason: String::new(),
            confidence: 2.0,
        };
        let fallback = validated_plan_or_fallback(
            invalid,
            "find authentication handler",
            QueryFilters {
                path: None,
                language: None,
            },
        )
        .unwrap();
        assert_eq!(fallback.strategy, QueryStrategy::Lexical);
        assert_eq!(fallback.reason, "validation_failed");
        assert_eq!(fallback.confidence, 0.0);
        assert!(serde_json::from_str::<QueryPlan>(
            r#"{"need_search":true,"strategy":"lexical","query":"needle","filters":{"path":null,"language":null},"reason":"test","confidence":1.0,"unknown":true}"#
        )
        .is_err());
    }

    #[test]
    fn loop_limits_use_fixed_priority_and_emit_a_terminal_event() {
        let mut fixture = Fixture::new("loop-limits");
        fixture.write("README.md", "bounded evidence");
        fixture.index();
        let result = search_workspace_with_config(
            fixture.database.connection(),
            &fixture.root,
            "bounded evidence",
            QueryFilters {
                path: None,
                language: None,
            },
            &RetrievalLimits::default(),
            &HybridConfig::default(),
            &LoopConfig {
                max_iterations: 0,
                wall_clock_timeout_ms: 0,
                token_budget: 0,
            },
        )
        .unwrap();
        assert_eq!(result.diagnostics.stop_reason, "iteration_limit");
        assert_eq!(
            result.diagnostics.reached_limits,
            vec!["iteration_limit", "timeout", "token_budget"]
        );
        assert_eq!(
            result
                .diagnostics
                .events
                .last()
                .map(|event| event.event_type.as_str()),
            Some("loop.stopped")
        );
        assert!(result
            .expansion_request
            .as_ref()
            .is_some_and(|request| request.requires_approval));
        let mut live_events = Vec::new();
        let _ = search_workspace_with_progress(
            fixture.database.connection(),
            &fixture.root,
            "bounded evidence",
            QueryFilters {
                path: None,
                language: None,
            },
            &RetrievalLimits::default(),
            &HybridConfig::default(),
            &LoopConfig::default(),
            |event| live_events.push(event.event_type),
        )
        .unwrap();
        assert_eq!(
            live_events.first().map(String::as_str),
            Some("planner.started")
        );
        assert_eq!(live_events.last().map(String::as_str), Some("loop.stopped"));
    }

    #[test]
    fn generation_publication_retrieval_and_incremental_reuse_work() {
        let mut fixture = Fixture::new("index-search");
        fixture.write(
            "src/auth.rs",
            "pub fn validate_token(value: &str) -> bool {\n    value == \"expected\"\n}\n",
        );
        fixture.write(
            "README.md",
            "# Authentication\nUse validate_token for checks.\n",
        );
        let first = fixture.index();
        assert_eq!(first.status, "published");
        assert_eq!(first.indexed_files, 2);
        let second = fixture.index();
        assert_eq!(second.reused_files, 2);
        let status = get_index_status(fixture.database.connection(), &fixture.root).unwrap();
        assert_eq!(status.generation, Some(2));
        let result = search_workspace(
            fixture.database.connection(),
            &fixture.root,
            "validate_token",
            QueryFilters {
                path: None,
                language: Some("rust".into()),
            },
            &RetrievalLimits::default(),
            &HybridConfig::default(),
        )
        .unwrap();
        assert!(!result.evidence.is_empty());
        assert!(result.evidence[0]
            .content
            .as_deref()
            .unwrap()
            .contains("validate_token"));
        assert_eq!(result.evidence[0].relative_path, "src/auth.rs");
        assert!(result.diagnostics.duration_ms < 500);

        let key = workspace_key(&fixture.root).unwrap();
        let generation = active_generation(fixture.database.connection(), &key)
            .unwrap()
            .unwrap();
        let details = fixture
            .database
            .connection()
            .prepare(
                "EXPLAIN QUERY PLAN SELECT document_id FROM workspace_documents
                 WHERE workspace_key = ?1 AND generation = ?2 AND language = 'rust' AND status = 'active'",
            )
            .unwrap()
            .query_map(params![key, generation], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join(" ");
        assert!(
            details.contains("INDEX"),
            "metadata scope must use an index: {details}"
        );
    }

    #[test]
    fn cancelled_run_keeps_old_published_generation() {
        let mut fixture = Fixture::new("cancel");
        fixture.write("README.md", "# One\nold evidence\n");
        fixture.index();
        fixture.write("README.md", "# Two\nnew evidence\n");
        let mut progress_events = Vec::new();
        let result = index_workspace(
            fixture.database.connection_mut(),
            &fixture.root,
            &IndexConfig::default(),
            true,
            || true,
            |event| progress_events.push(event),
        );
        assert!(matches!(result, Err(RagError::Cancelled)));
        let status = get_index_status(fixture.database.connection(), &fixture.root).unwrap();
        assert_eq!(status.generation, Some(1));
        assert_eq!(status.status, "published");
        assert_eq!(
            progress_events.last().map(|event| event.phase.as_str()),
            Some("cancelled")
        );
    }

    #[test]
    fn secret_binary_and_ragignore_paths_never_enter_index() {
        let mut fixture = Fixture::new("secrets");
        fixture.write("README.md", "public needle");
        fixture.write("notes.md", "client_secret = 'must-not-enter-index'");
        fixture.write(
            "tokens.txt",
            "ghp_0123456789012345678901234567890123456789\nAKIA0123456789ABCDEF",
        );
        fixture.write(".env", "API_KEY=super-secret");
        fixture.write("private.txt", "private marker");
        fixture.write(".ragignore", "private.txt\n");
        fs::write(fixture.root.join("binary.txt"), b"hello\0binary").unwrap();
        let summary = fixture.index();
        assert_eq!(summary.indexed_files, 1);
        let count: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM workspace_documents WHERE path IN ('.env','private.txt','binary.txt','notes.md','tokens.txt')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn vector_publication_is_atomic_and_hybrid_has_bounded_rrf_explanation() {
        let mut fixture = Fixture::new("hybrid");
        fixture.write(
            "src/search.rs",
            "pub fn search_workspace() { /* lexical vector fusion */ }",
        );
        fixture.write(
            "src/cache.rs",
            "pub fn cache_entries() { /* unrelated cache */ }",
        );
        fixture.write(
            "README.md",
            "# Guide\nUnrelated desktop setup instructions.",
        );
        fixture.index();
        let config = HybridConfig {
            enabled: true,
            ..HybridConfig::default()
        };
        let index_id = build_vector_index(
            fixture.database.connection_mut(),
            &fixture.root,
            &config,
            || false,
        )
        .unwrap();
        assert!(index_id.is_some());
        let result = search_workspace(
            fixture.database.connection(),
            &fixture.root,
            "search_workspace",
            QueryFilters {
                path: None,
                language: None,
            },
            &RetrievalLimits::default(),
            &config,
        )
        .unwrap();
        assert_eq!(result.diagnostics.mode, "hybrid");
        assert_eq!(result.evidence[0].ranking_explanation.algorithm, "rrf");
        assert!(result.evidence[0].ranking_explanation.sources.len() <= 2);
        let vector_only = search_workspace(
            fixture.database.connection(),
            &fixture.root,
            "concept_without_lexical_match",
            QueryFilters {
                path: None,
                language: None,
            },
            &RetrievalLimits::default(),
            &config,
        )
        .unwrap();
        assert_eq!(vector_only.diagnostics.mode, "hybrid");
        assert!(!vector_only.evidence.is_empty());
        assert!(vector_only.evidence.iter().any(|item| {
            item.ranking_explanation.lexical_rank.is_none()
                && item.ranking_explanation.vector_rank.is_some()
        }));
        for _ in 0..24 {
            let lexical = search_workspace(
                fixture.database.connection(),
                &fixture.root,
                "search_workspace",
                QueryFilters {
                    path: None,
                    language: None,
                },
                &RetrievalLimits::default(),
                &HybridConfig::default(),
            )
            .unwrap();
            let hybrid_result = search_workspace(
                fixture.database.connection(),
                &fixture.root,
                "search_workspace",
                QueryFilters {
                    path: None,
                    language: None,
                },
                &RetrievalLimits::default(),
                &config,
            )
            .unwrap();
            assert_eq!(lexical.evidence[0].relative_path, "src/search.rs");
            assert_eq!(hybrid_result.evidence[0].relative_path, "src/search.rs");
            let lexical_precision_at_3 = lexical
                .evidence
                .iter()
                .take(3)
                .filter(|item| item.relative_path == "src/search.rs")
                .count() as f64
                / 3.0;
            let hybrid_precision_at_3 = hybrid_result
                .evidence
                .iter()
                .take(3)
                .filter(|item| item.relative_path == "src/search.rs")
                .count() as f64
                / 3.0;
            let ndcg = |items: &[RetrievedChunk]| {
                items
                    .iter()
                    .take(3)
                    .position(|item| item.relative_path == "src/search.rs")
                    .map(|rank| 1.0 / ((rank + 2) as f64).log2())
                    .unwrap_or(0.0)
            };
            assert!(hybrid_precision_at_3 >= lexical_precision_at_3);
            assert!(ndcg(&hybrid_result.evidence) >= ndcg(&lexical.evidence));
        }
    }

    #[test]
    fn repeated_chunks_keep_their_persisted_byte_ranges() {
        let mut fixture = Fixture::new("repeated-range");
        fixture.write(
            "README.md",
            "# First\nrepeated evidence marker\n\n# Second\nrepeated evidence marker\n",
        );
        fixture.index();
        let result = search_workspace(
            fixture.database.connection(),
            &fixture.root,
            "repeated evidence marker",
            QueryFilters {
                path: None,
                language: None,
            },
            &RetrievalLimits::default(),
            &HybridConfig::default(),
        )
        .unwrap();
        let mut starts = result
            .evidence
            .iter()
            .filter_map(|item| item.lines.map(|lines| lines[0]))
            .collect::<Vec<_>>();
        starts.sort_unstable();
        starts.dedup();
        assert!(
            starts.len() >= 2,
            "duplicate text must retain distinct provenance"
        );
    }

    #[test]
    fn context_ledger_contains_only_metadata_and_race_makes_citation_stale() {
        let mut fixture = Fixture::new("citation-race");
        fixture.write("README.md", "# Stable fact\nThe answer is forty two.\n");
        fixture.index();
        let search = search_workspace(
            fixture.database.connection(),
            &fixture.root,
            "forty two",
            QueryFilters {
                path: None,
                language: None,
            },
            &RetrievalLimits::default(),
            &HybridConfig::default(),
        )
        .unwrap();
        let context = build_evidence_context(
            fixture.database.connection(),
            &fixture.root,
            &search,
            4096,
            8,
            16,
        )
        .unwrap();
        assert!(context.model_context.contains("[cite:"));
        assert!(verify_document_provenance(
            fixture.database.connection(),
            &fixture.root,
            &search.evidence[0].relative_path,
            &search.evidence[0].chunk_hash,
        )
        .unwrap());
        let stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT snippet_hash || ':' || chunk_hash || ':' || path FROM rag_context_ledger LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!stored.contains("forty two"));
        fixture.write(
            "README.md",
            "# Changed\nCompletely unrelated replacement.\n",
        );
        assert!(!verify_document_provenance(
            fixture.database.connection(),
            &fixture.root,
            &search.evidence[0].relative_path,
            &search.evidence[0].chunk_hash,
        )
        .unwrap());
        let final_context = finalize_citations(
            fixture.database.connection(),
            &fixture.root,
            &search,
            context,
        )
        .unwrap();
        assert!(final_context
            .citations
            .iter()
            .all(|citation| citation.status == CitationStatus::Stale));
        assert!(!final_context.model_context.contains("forty two"));
    }

    #[test]
    fn utf16_is_indexed_without_hidden_normalization_and_invalid_limits_fail_closed() {
        let mut fixture = Fixture::new("encoding");
        let mut bytes = vec![0xff, 0xfe];
        for word in "# UTF16\nneedle\n".encode_utf16() {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        fs::write(fixture.root.join("README.md"), bytes).unwrap();
        fixture.index();
        let encoding: String = fixture
            .database
            .connection()
            .query_row("SELECT encoding FROM workspace_documents", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(encoding, "utf-16le");
        let bad = RetrievalLimits {
            max_context_chunks: 20,
            max_evidence_chunks: 10,
            ..RetrievalLimits::default()
        };
        assert!(bad.validate().is_err());
    }
}
