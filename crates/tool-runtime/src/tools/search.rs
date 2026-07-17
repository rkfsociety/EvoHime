use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::ErrorKind,
    path::Path,
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

pub const NAME: &str = "filesystem.search";
pub const DESCRIPTION: &str =
    "Search text in workspace files (ripgrep when available, built-in walk fallback otherwise)";
pub const PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const TIMEOUT: Duration = Duration::from_secs(15);

const SKIP_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".launcher-logs",
    "dist",
    "build",
    ".venv",
    "venv",
];

#[derive(Deserialize)]
struct Input {
    query: String,
    path: Option<String>,
    glob: Option<String>,
    limit: Option<usize>,
}

pub async fn execute(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|e| ToolError::InvalidInput {
        tool: NAME.into(),
        message: e.to_string(),
    })?;
    if input.query.is_empty() {
        return Err(ToolError::InvalidInput {
            tool: NAME.into(),
            message: "query must not be empty".into(),
        });
    }
    let sandbox = ctx.sandbox()?;
    let base = match input.path.as_deref() {
        Some(path) => sandbox.resolve_existing(path)?,
        None => sandbox.root().to_path_buf(),
    };
    if let Some(glob) = input.glob.as_deref() {
        if glob.starts_with('-') {
            return Err(ToolError::InvalidInput {
                tool: NAME.into(),
                message: "invalid glob".into(),
            });
        }
    }
    let limit = input.limit.unwrap_or(100).min(1000);
    let force_fallback = std::env::var("EVOHIME_SEARCH_FORCE_FALLBACK")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);

    if !force_fallback {
        match ripgrep_search(
            sandbox.root(),
            &base,
            &input.query,
            input.glob.as_deref(),
            limit,
        )
        .await
        {
            Ok(matches) => {
                return Ok(search_result(matches, "ripgrep"));
            }
            Err(RgFailure::Missing) => {
                // fall through to walk fallback
            }
            Err(RgFailure::Failed(message)) => {
                return Err(ToolError::Execution(message));
            }
        }
    }

    let query = input.query.clone();
    let glob = input.glob.clone();
    let root = sandbox.root().to_path_buf();
    let matches = tokio::task::spawn_blocking(move || {
        fallback_search(&root, &base, &query, glob.as_deref(), limit)
    })
    .await
    .map_err(|error| ToolError::Execution(format!("search worker failed: {error}")))?;

    Ok(search_result(matches, "fallback"))
}

enum RgFailure {
    Missing,
    Failed(String),
}

async fn ripgrep_search(
    workspace_root: &Path,
    base: &Path,
    query: &str,
    glob: Option<&str>,
    limit: usize,
) -> Result<Vec<Value>, RgFailure> {
    let mut command = Command::new("rg");
    command
        .args(["--json", "--no-heading", "--color", "never", query])
        .arg(base)
        .current_dir(workspace_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(glob) = glob {
        command.args(["--glob", glob]);
    }
    let output = match command.output().await {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(RgFailure::Missing),
        Err(error) => {
            return Err(RgFailure::Failed(format!("ripgrep failed: {error}")));
        }
    };
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(RgFailure::Failed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    let mut matches = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Ok(item) = serde_json::from_str::<Value>(line) {
            if item["type"] == "match" {
                for sub in item["data"]["lines"]["text"]
                    .as_str()
                    .unwrap_or("")
                    .lines()
                {
                    matches.push(json!({
                        "path": item["data"]["path"]["text"],
                        "line": item["data"]["line_number"],
                        "text": sub
                    }));
                    if matches.len() >= limit {
                        break;
                    }
                }
            }
        }
        if matches.len() >= limit {
            break;
        }
    }
    Ok(matches)
}

fn fallback_search(
    workspace_root: &Path,
    base: &Path,
    query: &str,
    glob: Option<&str>,
    limit: usize,
) -> Vec<Value> {
    let mut matches = Vec::new();
    let mut stack = vec![base.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if SKIP_DIR_NAMES.iter().any(|skip| *skip == name.as_str()) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if let Some(glob) = glob {
                if !path_matches_glob(&path, glob) {
                    continue;
                }
            }
            if let Some(file_matches) = scan_file(workspace_root, &path, query, limit - matches.len())
            {
                matches.extend(file_matches);
                if matches.len() >= limit {
                    return matches;
                }
            }
        }
    }
    matches
}

fn scan_file(
    workspace_root: &Path,
    path: &Path,
    query: &str,
    remaining: usize,
) -> Option<Vec<Value>> {
    if remaining == 0 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    let rel = path
        .strip_prefix(workspace_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.contains(query) {
            out.push(json!({
                "path": rel,
                "line": index + 1,
                "text": line
            }));
            if out.len() >= remaining {
                break;
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn path_matches_glob(path: &Path, pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return true;
    }
    let pattern = pattern.strip_prefix("**/").unwrap_or(pattern);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if let Some(ext) = pattern.strip_prefix("*.") {
        return path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(ext));
    }
    if pattern.contains('*') {
        return wildcard_match(file_name, pattern);
    }
    file_name == pattern
        || path
            .to_string_lossy()
            .replace('\\', "/")
            .ends_with(pattern)
}

fn wildcard_match(text: &str, pattern: &str) -> bool {
    let mut text_chars = text.chars();
    let mut pattern_chars = pattern.chars().peekable();
    while let Some(expected) = pattern_chars.next() {
        if expected == '*' {
            if pattern_chars.peek().is_none() {
                return true;
            }
            let rest: String = pattern_chars.collect();
            let remainder: String = text_chars.collect();
            for index in 0..=remainder.len() {
                if remainder.is_char_boundary(index)
                    && wildcard_match(&remainder[index..], &rest)
                {
                    return true;
                }
            }
            return false;
        }
        match text_chars.next() {
            Some(actual) if actual == expected => {}
            _ => return false,
        }
    }
    text_chars.next().is_none()
}

fn search_result(matches: Vec<Value>, engine: &str) -> ToolResult {
    ToolResult {
        output: serde_json::to_string(&matches).unwrap_or_default(),
        structured: json!({
            "matches": matches,
            "count": matches.len(),
            "engine": engine,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn ctx(dir: &tempfile::TempDir) -> ToolContext {
        ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
}
    }

    #[tokio::test]
    async fn searches_recursively_and_honors_limit() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("nested")).expect("nested");
        std::fs::write(dir.path().join("a.txt"), "needle one").expect("write a");
        std::fs::write(dir.path().join("nested").join("b.md"), "needle two").expect("write b");

        let result = execute(
            &ctx(&dir),
            json!({
                "query": "needle",
                "limit": 1
            }),
        )
        .await
        .expect("search succeeds");

        assert_eq!(result.structured["count"], 1);
        assert_eq!(result.structured["matches"].as_array().unwrap().len(), 1);
        assert!(
            result.structured["engine"] == "ripgrep"
                || result.structured["engine"] == "fallback"
        );
    }

    #[tokio::test]
    async fn search_can_filter_by_glob() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("nested")).expect("nested");
        std::fs::write(dir.path().join("a.txt"), "needle one").expect("write a");
        std::fs::write(dir.path().join("nested").join("b.md"), "needle two").expect("write b");

        let result = execute(
            &ctx(&dir),
            json!({
                "query": "needle",
                "glob": "*.md"
            }),
        )
        .await
        .expect("search succeeds");

        let matches = result.structured["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["path"].as_str().unwrap().ends_with("b.md"));
    }

    #[tokio::test]
    async fn search_rejects_path_traversal() {
        let dir = tempdir().expect("tempdir");
        let error = execute(
            &ctx(&dir),
            json!({
                "query": "needle",
                "path": ".."
            }),
        )
        .await
        .expect_err("traversal rejected");

        assert!(matches!(
            error,
            ToolError::PermissionDenied(Permission::FilesystemRead)
        ));
    }

    #[test]
    fn fallback_search_finds_matches_and_skips_binary() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("nested")).expect("nested");
        std::fs::write(dir.path().join("a.txt"), "needle one").expect("write a");
        std::fs::write(dir.path().join("nested").join("b.md"), "needle two").expect("write b");
        std::fs::write(dir.path().join("skip.bin"), b"needle\0binary").expect("binary");

        let matches = fallback_search(
            dir.path(),
            dir.path(),
            "needle",
            Some("*.md"),
            10,
        );
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["path"].as_str().unwrap().ends_with("b.md"));
        assert_eq!(matches[0]["line"], 1);
    }

    #[test]
    fn glob_and_wildcard_helpers() {
        assert!(path_matches_glob(Path::new("docs/a.md"), "*.md"));
        assert!(!path_matches_glob(Path::new("docs/a.txt"), "*.md"));
        assert!(path_matches_glob(Path::new("foo.rs"), "foo.*"));
        assert!(wildcard_match("readme.md", "read*.md"));
        assert!(!wildcard_match("readme.txt", "read*.md"));
    }
}
