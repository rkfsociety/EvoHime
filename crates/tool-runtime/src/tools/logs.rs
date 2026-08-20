use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::fs;

pub const TAIL_NAME: &str = "logs.tail";
pub const TAIL_DESCRIPTION: &str = "Read last N lines from a log file";
pub const TAIL_PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const TAIL_TIMEOUT: Duration = Duration::from_secs(10);

pub const GREP_NAME: &str = "logs.grep";
pub const GREP_DESCRIPTION: &str = "Search log files for a pattern";
pub const GREP_PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const GREP_TIMEOUT: Duration = Duration::from_secs(30);

// ============================================================================
// logs.tail: Read last N lines
// ============================================================================

#[derive(Debug, Deserialize)]
struct TailInput {
    path: String,
    #[serde(default = "default_lines")]
    lines: usize,
}

fn default_lines() -> usize {
    50
}

pub async fn tail(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: TailInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: TAIL_NAME.to_string(),
        message: e.to_string(),
    })?;

    let file_path = ctx.sandbox()?.resolve_existing(&opts.path)?;

    let content = fs::read_to_string(&file_path)
        .await
        .map_err(|e| ToolError::Execution(format!("failed to read log file: {e}")))?;

    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > opts.lines {
        lines.len() - opts.lines
    } else {
        0
    };

    let tail_content = lines[start..].join("\n");

    Ok(ToolResult {
        output: tail_content.clone(),
        structured: json!({
            "action": "tail",
            "path": opts.path,
            "lines_shown": lines.len() - start,
            "total_lines": lines.len()
        }),
    })
}

// ============================================================================
// logs.grep: Search in logs
// ============================================================================

#[derive(Debug, Deserialize)]
struct GrepInput {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    case_insensitive: bool,
    #[serde(default)]
    invert_match: bool,
    #[serde(default, rename = "context_lines")]
    _context_lines: usize,
}

pub async fn grep(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: GrepInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: GREP_NAME.to_string(),
        message: e.to_string(),
    })?;

    let search_path = if let Some(p) = opts.path {
        ctx.sandbox()?.resolve_existing(&p)?
    } else {
        ctx.workspace_root.clone()
    };

    let mut matches = Vec::new();

    if search_path.is_file() {
        let content = fs::read_to_string(&search_path)
            .await
            .map_err(|e| ToolError::Execution(format!("failed to read file: {e}")))?;

        matches.extend(search_file(
            &content,
            &opts.pattern,
            opts.case_insensitive,
            opts.invert_match,
        ));
    } else if search_path.is_dir() {
        // Recursively search all files in directory
        search_directory(
            &search_path,
            &opts.pattern,
            opts.case_insensitive,
            opts.invert_match,
            &mut matches,
        )
        .await
        .map_err(|e| ToolError::Execution(format!("directory search failed: {e}")))?;
    }

    let result_text = if matches.is_empty() {
        "No matches found".to_string()
    } else {
        matches
            .iter()
            .take(1000)
            .map(|m| m.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };

    Ok(ToolResult {
        output: result_text,
        structured: json!({
            "action": "grep",
            "pattern": opts.pattern,
            "matches_count": matches.len(),
            "case_insensitive": opts.case_insensitive,
            "invert_match": opts.invert_match
        }),
    })
}

fn search_file(
    content: &str,
    pattern: &str,
    case_insensitive: bool,
    invert_match: bool,
) -> Vec<String> {
    let pattern_lower = pattern.to_lowercase();
    content
        .lines()
        .enumerate()
        .filter_map(|(line_num, line)| {
            let line_matches = if case_insensitive {
                line.to_lowercase().contains(&pattern_lower)
            } else {
                line.contains(pattern)
            };

            let should_include = if invert_match {
                !line_matches
            } else {
                line_matches
            };

            if should_include {
                Some(format!("{}: {}", line_num + 1, line))
            } else {
                None
            }
        })
        .collect()
}

async fn search_directory(
    dir: &std::path::Path,
    pattern: &str,
    case_insensitive: bool,
    invert_match: bool,
    matches: &mut Vec<String>,
) -> std::io::Result<()> {
    let mut pending = vec![dir.to_path_buf()];

    while let Some(current_dir) = pending.pop() {
        let mut entries = fs::read_dir(current_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_file() {
                // Only search text files (log, txt, json, yaml, md, rs, ts, etc)
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if matches!(
                        ext_str.as_str(),
                        "log"
                            | "txt"
                            | "json"
                            | "yaml"
                            | "yml"
                            | "md"
                            | "rs"
                            | "ts"
                            | "tsx"
                            | "jsx"
                            | "js"
                            | "py"
                            | "go"
                            | "java"
                    ) {
                        if let Ok(content) = fs::read_to_string(&path).await {
                            let file_matches =
                                search_file(&content, pattern, case_insensitive, invert_match);
                            for m in file_matches {
                                matches.push(format!("{}:{}", path.display(), m));
                            }
                        }
                    }
                }
            } else if path.is_dir()
                && !path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                // Avoid hidden directories
                pending.push(path);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[tokio::test]
    async fn tail_works() {
        let dir = tempdir().expect("tempdir");
        let log_file = dir.path().join("test.log");

        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!("Line {}\n", i));
        }

        std_fs::write(&log_file, content).expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let result = tail(&ctx, json!({"path": "test.log", "lines": 10}))
            .await
            .expect("tail");

        assert!(result.output.contains("Line 99"));
        let line_count = result.output.lines().count();
        assert!(line_count <= 10);
    }

    #[tokio::test]
    async fn grep_works() {
        let dir = tempdir().expect("tempdir");
        let log_file = dir.path().join("test.log");
        std_fs::write(
            &log_file,
            "error: something failed\ninfo: all good\nerror: another failure\n",
        )
        .expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let result = grep(&ctx, json!({"pattern": "error", "path": "test.log"}))
            .await
            .expect("grep");

        assert!(result.output.contains("error"));
        let matches: serde_json::Value = result.structured;
        assert_eq!(matches["matches_count"], 2);
    }
}
