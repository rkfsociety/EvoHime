//! Workspace text search for agent context (roadmap 6.1 / P2).
//!
//! Lexical search with:
//! - chunk merging (adjacent hits)
//! - separate path / symbol / content weights
//! - binary + noisy-file filtering
//!
//! Wave 4: Semantic search with embeddings:
//! - Chunk embeddings cached locally
//! - Hash-based deduplication
//! - Hybrid BM25 + cosine similarity scoring

pub mod embeddings;

use std::cmp::Reverse;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use embeddings::{EmbeddingCache, SemanticMatch};

const DEFAULT_MAX_RESULTS: usize = 5;
const DEFAULT_MAX_FILE_BYTES: u64 = 256 * 1024;
const CHUNK_GAP_LINES: usize = 2;
const CHUNK_MAX_LINES: usize = 12;
const CHUNK_SNIPPET_CHARS: usize = 480;

const WEIGHT_CONTENT: u32 = 10;
const WEIGHT_PATH: u32 = 25;
const WEIGHT_SYMBOL: u32 = 40;
const WEIGHT_SHORT_LINE: u32 = 1;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".evohime",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    ".next",
    ".turbo",
    ".cache",
];

const NOISY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "bmp", "svg", "pdf", "zip", "gz", "7z", "rar",
    "exe", "dll", "so", "dylib", "bin", "o", "a", "lib", "wasm", "map", "min.js", "min.css",
    "lock", "woff", "woff2", "ttf", "otf", "eot", "mp3", "mp4", "webm", "wav", "class", "jar",
    "parquet", "sqlite", "db", "pyc", "pyo",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Content,
    Path,
    Symbol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIndexMatch {
    pub path: PathBuf,
    /// 1-based start line of the chunk.
    pub line: usize,
    /// 1-based inclusive end line of the chunk.
    pub end_line: usize,
    pub snippet: String,
    pub score: u32,
    pub hit_kind: HitKind,
}

#[derive(Debug, Clone)]
pub struct ProjectIndex {
    workspace_root: PathBuf,
    max_results: usize,
    max_file_bytes: u64,
    /// Wave 4: Embedding cache for semantic search
    embeddings_cache: EmbeddingCache,
}

impl ProjectIndex {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            max_results: DEFAULT_MAX_RESULTS,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            embeddings_cache: EmbeddingCache::new(),
        }
    }

    pub fn with_limits(
        workspace_root: impl Into<PathBuf>,
        max_results: usize,
        max_file_bytes: u64,
    ) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            max_results: max_results.max(1),
            max_file_bytes: max_file_bytes.max(1),
            embeddings_cache: EmbeddingCache::new(),
        }
    }

    /// Wave 4: Get embedding cache (for testing & stats)
    pub fn embeddings_cache(&self) -> &EmbeddingCache {
        &self.embeddings_cache
    }

    pub fn search(&self, query: &str) -> Vec<ProjectIndexMatch> {
        self.search_with_limit(query, self.max_results)
    }

    pub fn search_with_limit(&self, query: &str, limit: usize) -> Vec<ProjectIndexMatch> {
        let query = normalize_query(query);
        if query.is_empty() || !self.workspace_root.exists() {
            return Vec::new();
        }

        let mut matches = Vec::new();
        let limit = limit.max(1);

        for entry in WalkDir::new(&self.workspace_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| !should_skip_directory(entry.path()))
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if entry.file_type().is_dir() {
                continue;
            }

            if should_skip_path(path) || is_noisy_file(path) {
                continue;
            }

            let metadata = match fs::metadata(path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.len() > self.max_file_bytes {
                continue;
            }

            let Ok(bytes) = fs::read(path) else {
                continue;
            };
            if is_probably_binary(&bytes) {
                continue;
            }
            let Ok(content) = String::from_utf8(bytes) else {
                continue;
            };

            let relative = relative_to_root(&self.workspace_root, path);
            let line_hits = collect_line_hits(&query, &content, &relative);
            matches.extend(merge_into_chunks(&relative, &content, line_hits));
        }

        matches.sort_by_key(|item| {
            (
                Reverse(item.score),
                hit_kind_rank(item.hit_kind),
                item.path.clone(),
                item.line,
                item.snippet.len(),
            )
        });
        matches.truncate(limit);
        matches
    }

    pub fn build_context(&self, query: &str, limit: usize) -> Option<String> {
        let matches = self.search_with_limit(query, limit);
        if matches.is_empty() {
            return None;
        }

        let mut output = String::from("Relevant project context:\n");
        for item in matches {
            let location = if item.end_line > item.line {
                format!("{}:{}-{}", item.path.display(), item.line, item.end_line)
            } else {
                format!("{}:{}", item.path.display(), item.line)
            };
            let kind = match item.hit_kind {
                HitKind::Symbol => "symbol",
                HitKind::Path => "path",
                HitKind::Content => "content",
            };
            let snippet = item.snippet.replace('\n', " / ");
            output.push_str(&format!(
                "- {location} [{kind}:{score}] {snippet}\n",
                score = item.score
            ));
        }

        Some(output.trim_end().to_string())
    }
}

#[derive(Debug, Clone)]
struct LineHit {
    line: usize,
    score: u32,
    kind: HitKind,
}

fn normalize_query(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_'))
        .filter(|token| token.len() > 1)
        .map(|token| token.to_lowercase())
        .collect()
}

fn collect_line_hits(query: &[String], content: &str, relative_path: &Path) -> Vec<LineHit> {
    let path_lower = relative_path.to_string_lossy().to_lowercase();
    let path_stem = relative_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let path_bonus = path_query_bonus(query, &path_lower, &path_stem);

    let mut hits = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        let (score, kind) = score_line_content(query, line);
        if score == 0 {
            continue;
        }
        hits.push(LineHit {
            line: line_index + 1,
            score,
            kind,
        });
    }

    if hits.is_empty() {
        if path_bonus > 0 {
            hits.push(LineHit {
                line: 1,
                score: path_bonus.saturating_add(WEIGHT_SHORT_LINE),
                kind: HitKind::Path,
            });
        }
    } else if path_bonus > 0 {
        if let Some(best) = hits.iter_mut().max_by_key(|hit| hit.score) {
            best.score = best.score.saturating_add(path_bonus);
            best.kind = stronger_kind(best.kind, HitKind::Path);
        }
    }

    hits
}

fn path_query_bonus(query: &[String], path_lower: &str, path_stem: &str) -> u32 {
    let mut bonus = 0u32;
    for token in query {
        if path_lower.contains(token) || path_stem == *token {
            bonus = bonus.saturating_add(WEIGHT_PATH);
        }
    }
    bonus
}

fn score_line_content(query: &[String], line: &str) -> (u32, HitKind) {
    let line_lower = line.to_lowercase();
    let mut content_score = 0u32;
    let mut symbol_score = 0u32;

    for token in query {
        if line_lower.contains(token) {
            content_score = content_score.saturating_add(WEIGHT_CONTENT);
            if looks_like_symbol_hit(line, token) {
                symbol_score = symbol_score.saturating_add(WEIGHT_SYMBOL);
            }
        }
    }

    let mut score = content_score.saturating_add(symbol_score);
    if score > 0 && line.len() < 120 {
        score = score.saturating_add(WEIGHT_SHORT_LINE);
    }

    let kind = if symbol_score > 0 {
        HitKind::Symbol
    } else {
        HitKind::Content
    };

    (score, kind)
}

fn looks_like_symbol_hit(line: &str, token: &str) -> bool {
    let lower = line.to_lowercase();
    let patterns = [
        format!("fn {token}"),
        format!("fn {token}<"),
        format!("fn {token}("),
        format!("struct {token}"),
        format!("enum {token}"),
        format!("class {token}"),
        format!("trait {token}"),
        format!("interface {token}"),
        format!("type {token}"),
        format!("def {token}"),
        format!("function {token}"),
        format!("const {token}"),
        format!("let {token}"),
        format!("pub fn {token}"),
        format!("async fn {token}"),
    ];
    patterns.iter().any(|pattern| lower.contains(pattern))
}

fn merge_into_chunks(
    relative_path: &Path,
    content: &str,
    hits: Vec<LineHit>,
) -> Vec<ProjectIndexMatch> {
    if hits.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = content.lines().collect();
    let mut chunks = Vec::new();
    let mut current_start = hits[0].line;
    let mut current_end = hits[0].line;
    let mut current_score = hits[0].score;
    let mut current_kind = hits[0].kind;

    for hit in hits.into_iter().skip(1) {
        if hit.line <= current_end + CHUNK_GAP_LINES
            && (current_end - current_start + 1) < CHUNK_MAX_LINES
        {
            current_end = hit.line;
            current_score = current_score.saturating_add(hit.score / 2);
            current_kind = stronger_kind(current_kind, hit.kind);
        } else {
            chunks.push(make_chunk(
                relative_path,
                &lines,
                current_start,
                current_end,
                current_score,
                current_kind,
            ));
            current_start = hit.line;
            current_end = hit.line;
            current_score = hit.score;
            current_kind = hit.kind;
        }
    }
    chunks.push(make_chunk(
        relative_path,
        &lines,
        current_start,
        current_end,
        current_score,
        current_kind,
    ));
    chunks
}

fn make_chunk(
    relative_path: &Path,
    lines: &[&str],
    start_line: usize,
    end_line: usize,
    score: u32,
    kind: HitKind,
) -> ProjectIndexMatch {
    let start_idx = start_line.saturating_sub(1);
    let end_idx = end_line.min(lines.len()).saturating_sub(1).max(start_idx);
    let snippet = if lines.is_empty() {
        String::new()
    } else {
        lines[start_idx..=end_idx]
            .iter()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let snippet = truncate_chars(&snippet, CHUNK_SNIPPET_CHARS);

    ProjectIndexMatch {
        path: relative_path.to_path_buf(),
        line: start_line,
        end_line,
        snippet,
        score,
        hit_kind: kind,
    }
}

fn stronger_kind(left: HitKind, right: HitKind) -> HitKind {
    match (left, right) {
        (HitKind::Symbol, _) | (_, HitKind::Symbol) => HitKind::Symbol,
        (HitKind::Path, _) | (_, HitKind::Path) => HitKind::Path,
        _ => HitKind::Content,
    }
}

fn hit_kind_rank(kind: HitKind) -> u8 {
    match kind {
        HitKind::Symbol => 0,
        HitKind::Path => 1,
        HitKind::Content => 2,
    }
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    input
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn should_skip_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| IGNORED_DIRS.contains(&name))
}

fn should_skip_path(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| IGNORED_DIRS.contains(&name))
    })
}

fn is_noisy_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_lowercase();
    if name.ends_with(".min.js") || name.ends_with(".min.css") || name.ends_with(".bundle.js") {
        return true;
    }
    if name == "package-lock.json" || name == "yarn.lock" || name == "pnpm-lock.yaml" {
        return true;
    }
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| NOISY_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_probably_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if bytes.contains(&0) {
        return true;
    }
    let sample = &bytes[..bytes.len().min(8_192)];
    let non_text = sample
        .iter()
        .filter(|&&b| b < 0x09 || (b > 0x0d && b < 0x20) || b == 0x7f)
        .count();
    (non_text as f64) / (sample.len() as f64) > 0.30
}

fn relative_to_root(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_relevant_snippets_and_limits_results() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("alpha.md"),
            "project index context\nmore text",
        )
        .expect("write");
        fs::write(temp.path().join("beta.md"), "project index context again").expect("write");
        fs::write(
            temp.path().join("gamma.md"),
            "project index context once more",
        )
        .expect("write");

        let index = ProjectIndex::with_limits(temp.path(), 2, DEFAULT_MAX_FILE_BYTES);
        let matches = index.search("project index context");

        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|item| item.snippet.contains("context")));
        assert!(matches.iter().all(|item| item.end_line >= item.line));
    }

    #[test]
    fn ignores_target_and_node_modules() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("target")).expect("dir");
        fs::create_dir_all(temp.path().join("node_modules")).expect("dir");
        fs::write(
            temp.path().join("target/hidden.md"),
            "project index context inside target",
        )
        .expect("write");
        fs::write(
            temp.path().join("node_modules/hidden.md"),
            "project index context inside node_modules",
        )
        .expect("write");
        fs::write(
            temp.path().join("visible.md"),
            "project index context visible",
        )
        .expect("write");

        let index = ProjectIndex::new(temp.path());
        let matches = index.search("project index context");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, PathBuf::from("visible.md"));
    }

    #[test]
    fn builds_context_block() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("notes.md"), "project index context").expect("write");

        let index = ProjectIndex::new(temp.path());
        let context = index.build_context("project index", 3).expect("context");

        assert!(context.contains("Relevant project context:"));
        assert!(context.contains("notes.md:1"));
        assert!(context.contains("project index context"));
    }

    #[test]
    fn prefers_symbol_hits_over_plain_content() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("lib.rs"),
            "fn retrieve_memory() {\n    // helper\n}\n\nlet x = retrieve_memory_cache;\n",
        )
        .expect("write");
        fs::write(
            temp.path().join("notes.md"),
            "please retrieve memory from disk later\n",
        )
        .expect("write");

        let index = ProjectIndex::new(temp.path());
        let matches = index.search("retrieve_memory");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].path, PathBuf::from("lib.rs"));
        assert_eq!(matches[0].hit_kind, HitKind::Symbol);
    }

    #[test]
    fn merges_adjacent_hits_into_chunks() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("chunk.rs"),
            "line zero\nchunk alpha\nchunk beta\nchunk gamma\nline tail\n",
        )
        .expect("write");

        let index = ProjectIndex::new(temp.path());
        let matches = index.search("chunk");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 2);
        assert_eq!(matches[0].end_line, 4);
        assert!(matches[0].snippet.contains("chunk alpha"));
        assert!(matches[0].snippet.contains("chunk gamma"));
    }

    #[test]
    fn skips_binary_and_noisy_extensions() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("ok.md"), "binary filter context").expect("write");
        fs::write(temp.path().join("noise.png"), [0u8, 1, 2, 3, 255]).expect("png");
        fs::write(
            temp.path().join("blob.bin"),
            b"binary filter context\0hidden",
        )
        .expect("bin");
        fs::write(temp.path().join("app.min.js"), "binary filter context").expect("minjs");

        let index = ProjectIndex::new(temp.path());
        let matches = index.search("binary filter context");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, PathBuf::from("ok.md"));
    }

    #[test]
    fn path_token_boosts_filename_matches() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("memory")).expect("dir");
        fs::write(temp.path().join("memory/service.rs"), "plain helper text\n").expect("write");
        fs::write(temp.path().join("other.rs"), "mentions memory once\n").expect("write");

        let index = ProjectIndex::new(temp.path());
        let matches = index.search("memory");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].path, PathBuf::from("memory/service.rs"));
        assert!(matches!(
            matches[0].hit_kind,
            HitKind::Path | HitKind::Content
        ));
    }

    #[test]
    fn path_match_in_empty_file_does_not_panic() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("smoke-final.txt"), "").expect("write");

        let index = ProjectIndex::new(temp.path());
        let matches = index.search("smoke-final");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, PathBuf::from("smoke-final.txt"));
        assert!(matches[0].snippet.is_empty());
    }
}
