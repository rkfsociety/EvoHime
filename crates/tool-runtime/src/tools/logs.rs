use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncReadExt;

pub const TAIL_NAME: &str = "logs.tail";
pub const TAIL_DESCRIPTION: &str = "Read last N lines from a log file";
pub const TAIL_PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const TAIL_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_TAIL_LINES: usize = 1_000;
const MAX_TAIL_LINE_BYTES: usize = 64 * 1024;
const MAX_TAIL_OUTPUT_BYTES: usize = 1024 * 1024;

pub const GREP_NAME: &str = "logs.grep";
pub const GREP_DESCRIPTION: &str = "Search log files for a pattern";
pub const GREP_PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const GREP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_GREP_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_GREP_MATCHES: usize = 1_000;
const MAX_GREP_LINE_CHARS: usize = 16 * 1024;
const MAX_GREP_OUTPUT_BYTES: usize = 1024 * 1024;

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

    let mut file = fs::File::open(&file_path)
        .await
        .map_err(|e| ToolError::Execution(format!("failed to read log file: {e}")))?;
    let requested_lines = opts.lines.min(MAX_TAIL_LINES);
    let mut read_buffer = [0_u8; 16 * 1024];
    let mut current_line = Vec::new();
    let mut tail = VecDeque::new();
    let mut tail_bytes = 0;
    let mut total_lines: usize = 0;
    loop {
        let read = file
            .read(&mut read_buffer)
            .await
            .map_err(|e| ToolError::Execution(format!("failed to read log file: {e}")))?;
        if read == 0 {
            break;
        }
        for byte in &read_buffer[..read] {
            if *byte == b'\n' {
                total_lines = total_lines.saturating_add(1);
                append_tail_line(&mut tail, &mut tail_bytes, &current_line, requested_lines);
                current_line.clear();
            } else if current_line.len() < MAX_TAIL_LINE_BYTES {
                current_line.push(*byte);
            }
        }
    }
    if !current_line.is_empty() {
        total_lines = total_lines.saturating_add(1);
        append_tail_line(&mut tail, &mut tail_bytes, &current_line, requested_lines);
    }
    let lines_shown = tail.len();
    let tail_content = tail.into_iter().collect::<Vec<_>>().join("\n");

    Ok(ToolResult {
        output: tail_content.clone(),
        structured: json!({
            "action": "tail",
            "path": opts.path,
            "lines_shown": lines_shown,
            "total_lines": total_lines
        }),
    })
}

fn append_tail_line(
    tail: &mut VecDeque<String>,
    tail_bytes: &mut usize,
    line: &[u8],
    requested_lines: usize,
) {
    if requested_lines == 0 {
        return;
    }
    let value = String::from_utf8_lossy(line).into_owned();
    *tail_bytes = tail_bytes.saturating_add(value.len().saturating_add(1));
    tail.push_back(value);
    while tail.len() > requested_lines || *tail_bytes > MAX_TAIL_OUTPUT_BYTES {
        let Some(removed) = tail.pop_front() else {
            break;
        };
        *tail_bytes = tail_bytes.saturating_sub(removed.len().saturating_add(1));
    }
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
        let content = read_bounded_text(&search_path, MAX_GREP_FILE_BYTES)
            .await
            .map_err(|error| ToolError::Execution(format!("failed to read file: {error}")))?;

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

    let truncated = matches.len() >= MAX_GREP_MATCHES;
    let result_text = if matches.is_empty() {
        "No matches found".to_string()
    } else {
        let mut output = String::new();
        for (index, entry) in matches.iter().take(MAX_GREP_MATCHES).enumerate() {
            if index > 0 {
                if output.len() >= MAX_GREP_OUTPUT_BYTES {
                    break;
                }
                output.push('\n');
            }
            if output.len() >= MAX_GREP_OUTPUT_BYTES {
                break;
            }
            for character in entry.chars() {
                if character.len_utf8() > MAX_GREP_OUTPUT_BYTES.saturating_sub(output.len()) {
                    break;
                }
                output.push(character);
            }
        }
        output
    };

    Ok(ToolResult {
        output: result_text,
        structured: json!({
            "action": "grep",
            "pattern": opts.pattern,
            "matches_count": matches.len(),
            "truncated": truncated,
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
                Some(format!(
                    "{}: {}",
                    line_num + 1,
                    line.chars().take(MAX_GREP_LINE_CHARS).collect::<String>()
                ))
            } else {
                None
            }
        })
        .take(MAX_GREP_MATCHES)
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

        while matches.len() < MAX_GREP_MATCHES {
            let Some(entry) = entries.next_entry().await? else {
                break;
            };
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
                        if let Ok(content) = read_bounded_text(&path, MAX_GREP_FILE_BYTES).await {
                            let file_matches =
                                search_file(&content, pattern, case_insensitive, invert_match);
                            for m in file_matches {
                                if matches.len() >= MAX_GREP_MATCHES {
                                    break;
                                }
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

async fn read_bounded_text(path: &std::path::Path, limit: u64) -> io::Result<String> {
    if fs::metadata(path).await?.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "log file exceeds 4 MiB grep limit",
        ));
    }
    let mut file = fs::File::open(path).await?;
    let mut bytes = Vec::with_capacity((limit as usize).min(16 * 1024));
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = file.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        if bytes.len() as u64 > limit.saturating_sub(read as u64) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "log file exceeds 4 MiB grep limit",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
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
    async fn tail_does_not_load_the_whole_log_into_memory() {
        let dir = tempdir().expect("tempdir");
        let log_file = dir.path().join("large.log");
        std_fs::write(&log_file, vec![b'x'; MAX_TAIL_OUTPUT_BYTES * 2]).expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let result = tail(&ctx, json!({"path": "large.log", "lines": 50}))
            .await
            .expect("tail");
        assert!(result.output.len() <= MAX_TAIL_OUTPUT_BYTES);
        assert_eq!(result.structured["total_lines"], 1);
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

    #[tokio::test]
    async fn grep_rejects_oversized_direct_file() {
        let dir = tempdir().expect("tempdir");
        let log_file = dir.path().join("oversized.log");
        std_fs::write(&log_file, vec![b'x'; MAX_GREP_FILE_BYTES as usize + 1]).expect("write");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let error = grep(&ctx, json!({"pattern": "x", "path": "oversized.log"}))
            .await
            .expect_err("oversized grep input must be rejected");
        assert!(error.to_string().contains("exceeds 4 MiB"));
    }

    #[tokio::test]
    async fn grep_skips_oversized_directory_files() {
        let dir = tempdir().expect("tempdir");
        std_fs::write(
            dir.path().join("oversized.log"),
            vec![b'x'; MAX_GREP_FILE_BYTES as usize + 1],
        )
        .expect("write");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let result = grep(&ctx, json!({"pattern": "x"}))
            .await
            .expect("directory grep");
        assert_eq!(result.output, "No matches found");
    }
}
