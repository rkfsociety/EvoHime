use std::cmp::Reverse;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const DEFAULT_MAX_RESULTS: usize = 5;
const DEFAULT_MAX_FILE_BYTES: u64 = 256 * 1024;
const IGNORED_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIndexMatch {
    pub path: PathBuf,
    pub line: usize,
    pub snippet: String,
    pub score: u32,
}

#[derive(Debug, Clone)]
pub struct ProjectIndex {
    workspace_root: PathBuf,
    max_results: usize,
    max_file_bytes: u64,
}

impl ProjectIndex {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            max_results: DEFAULT_MAX_RESULTS,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
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
        }
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

            if should_skip_path(path) {
                continue;
            }

            let metadata = match fs::metadata(path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.len() > self.max_file_bytes {
                continue;
            }

            let Ok(content) = fs::read_to_string(path) else {
                continue;
            };

            for (line_index, line) in content.lines().enumerate() {
                let score = score_line(&query, line, path);
                if score == 0 {
                    continue;
                }

                matches.push(ProjectIndexMatch {
                    path: relative_to_root(&self.workspace_root, path),
                    line: line_index + 1,
                    snippet: line.trim().to_string(),
                    score,
                });
            }
        }

        matches.sort_by_key(|item| {
            (
                Reverse(item.score),
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
            output.push_str(&format!(
                "- {}:{} [{}] {}\n",
                item.path.display(),
                item.line,
                item.score,
                item.snippet
            ));
        }

        Some(output.trim_end().to_string())
    }
}

fn normalize_query(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric()))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn score_line(query: &[String], line: &str, path: &Path) -> u32 {
    let line_lower = line.to_lowercase();
    let path_lower = path.to_string_lossy().to_lowercase();

    let mut score = 0;
    for token in query {
        if line_lower.contains(token) {
            score += 10;
        }
        if path_lower.contains(token) {
            score += 3;
        }
    }

    if score > 0 && line.len() < 120 {
        score += 1;
    }

    score
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
}
