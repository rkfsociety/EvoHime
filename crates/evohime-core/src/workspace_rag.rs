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
    let mut compact = String::with_capacity(normalized.len());
    for (index, part) in normalized.split_whitespace().enumerate() {
        if index > 0 {
            compact.push(' ');
        }
        compact.push_str(part);
    }
    normalized = compact;
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
    let name = path.file_name()?.to_string_lossy();
    if name
        .get(.."readme".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("readme"))
    {
        return Some(("markdown", "text/markdown"));
    }
    let extension = path.extension()?.to_string_lossy();
    if extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown") {
        Some(("markdown", "text/markdown"))
    } else if extension.eq_ignore_ascii_case("rs") {
        Some(("rust", "text/x-rust"))
    } else if extension.eq_ignore_ascii_case("ts") || extension.eq_ignore_ascii_case("tsx") {
        Some(("typescript", "text/typescript"))
    } else if ["js", "jsx", "mjs", "cjs"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        Some(("javascript", "text/javascript"))
    } else if extension.eq_ignore_ascii_case("json") {
        Some(("json", "application/json"))
    } else if extension.eq_ignore_ascii_case("toml") {
        Some(("toml", "application/toml"))
    } else if extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml") {
        Some(("yaml", "application/yaml"))
    } else if ["txt", "log", "csv"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        Some(("text", "text/plain"))
    } else {
        None
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
    if pattern.is_empty() || pattern.starts_with('#') {
        return false;
    }
    if pattern.ends_with('/') {
        return path
            .split('/')
            .any(|part| part == pattern.trim_end_matches('/'));
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return path
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.ends_with('.'));
    }
    if pattern.contains('*') {
        let mut offset = 0;
        for part in pattern.split('*').filter(|part| !part.is_empty()) {
            let Some(found) = path[offset..].find(part) else {
                return false;
            };
            offset += found + part.len();
        }
        return true;
    }
    path == pattern
        || path
            .strip_prefix(pattern)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || path
            .strip_suffix(pattern)
            .is_some_and(|prefix| prefix.ends_with('/'))
}

fn load_ragignore(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join(".ragignore"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(normalize_ignore_pattern)
        .filter(|pattern| !pattern.is_empty() && !pattern.starts_with('#'))
        .collect()
}

fn normalize_ignore_pattern(pattern: &str) -> String {
    pattern
        .trim()
        .trim_start_matches('/')
        .replace('\\', "/")
        .to_lowercase()
}

fn collect_files(root: &Path, config: &IndexConfig) -> Result<(Vec<PathBuf>, usize), RagError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut excluded = 0;
    let ragignore = load_ragignore(root);
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_cached_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
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
    files.sort_by_cached_key(|path| path.to_string_lossy().replace('\\', "/").to_lowercase());
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
        let mut jwt_parts = token.split('.');
        let looks_like_jwt = matches!(
            (
                jwt_parts.next(),
                jwt_parts.next(),
                jwt_parts.next(),
                jwt_parts.next()
            ),
            (Some(first), Some(second), Some(third), None)
                if first.starts_with("eyj")
                    && first.len() >= 8
                    && second.len() >= 8
                    && third.len() >= 8
        );
        (token.starts_with("ghp_") && token.len() >= 36)
            || (token.starts_with("github_pat_") && token.len() >= 30)
            || (token.starts_with("sk-") && token.len() >= 24)
            || (token.starts_with("akia")
                && token.len() == 20
                && token
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric()))
            || looks_like_jwt
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
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair));
        let text = char::decode_utf16(words)
            .map(|result| result.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect();
        return (text, "utf-16le", "valid");
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        let words = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair));
        let text = char::decode_utf16(words)
            .map(|result| result.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect();
        return (text, "utf-16be", "valid");
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

#[path = "workspace_rag_evidence.rs"]
mod evidence;
/// Builds a private generation and atomically publishes it. The caller owns
/// cancellation and progress policy; neither callback can alter the workspace
/// root or any indexed content.
#[path = "workspace_rag_index.rs"]
mod index;
#[path = "workspace_rag_retrieval.rs"]
mod retrieval;

#[allow(unused_imports)]
pub use evidence::{
    build_evidence_context, finalize_citations, rag_ledger_projection, verify_document_provenance,
};
pub(super) use index::active_generation;
pub use index::{get_index_status, index_workspace};
pub use retrieval::SearchWorkspaceInput;
pub(super) use retrieval::{
    bounded_error, estimate_tokens, previous_char_boundary, stable_id, validate_source,
};
#[allow(unused_imports)]
pub use retrieval::{
    build_vector_index, search_workspace, search_workspace_with_config,
    search_workspace_with_progress,
};

#[cfg(test)]
#[path = "workspace_rag_tests.rs"]
mod tests;
