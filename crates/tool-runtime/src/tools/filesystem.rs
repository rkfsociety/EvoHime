use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

/// Registry identifier for the workspace text-file reader.
pub const NAME: &str = "filesystem.read";
/// Short user-facing summary shown in tool catalogs.
pub const DESCRIPTION: &str = "Read a UTF-8 text file from the workspace";
/// Permission required before the tool can read workspace files.
pub const PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
/// Default execution deadline for one read request.
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct Input {
    path: String,
    /// 1-based first line to return.
    #[serde(default)]
    offset: Option<usize>,
    /// Maximum number of lines to return.
    #[serde(default)]
    limit: Option<usize>,
}

const DEFAULT_LINE_LIMIT: usize = 200;
const MAX_LINE_LIMIT: usize = 2_000;

/// Reads a UTF-8 file through the revision-safe workspace boundary.
///
/// The returned structured value includes byte length, content hash, revision,
/// and a display preview. The preview is a window of `limit` lines (default
/// 200, at most 2000) starting at the 1-based `offset`; `content_hash` always
/// describes the whole file. Reads outside the authorized workspace are
/// rejected.
///
/// # Errors
///
/// Returns [`ToolError`] for invalid input, missing files, denied access, or
/// filesystem failures.
pub async fn execute(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(input).map_err(|error| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: error.to_string(),
    })?;

    let (file_ref, content) = crate::revision_safe_workspace_files::read(ctx, &input.path)
        .await
        .map_err(|error| {
            if matches!(error, crate::revision_safe_workspace_files::RevisionError::Io(ref io) if io.kind() == std::io::ErrorKind::NotFound) {
                return ToolError::NotFound { tool: NAME.into(), path: input.path.clone(), hint: String::new() };
            }
            crate::revision_safe_workspace_files::permission(
                error,
                NAME,
                Permission::FilesystemRead,
            )
        })?;

    let offset = input.offset.unwrap_or(1);
    if offset == 0 {
        return Err(ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: "offset must be at least 1".to_string(),
        });
    }
    let limit = input
        .limit
        .unwrap_or(DEFAULT_LINE_LIMIT)
        .clamp(1, MAX_LINE_LIMIT);
    let window = line_window(&content, offset, limit);
    let display = window.text.clone();
    Ok(ToolResult {
        output: display.clone(),
        structured: json!({
            "path": input.path,
            "bytes": content.len(),
            "content_hash": file_ref.content_hash,
            "revision": file_ref.revision,
            "namespace": file_ref.namespace.as_str(),
            "preview": display,
            "start_line": window.start_line,
            "end_line": window.end_line,
            "total_lines": window.total_lines,
            "truncated": window.truncated,
        }),
    })
}

struct LineWindow {
    text: String,
    start_line: usize,
    end_line: usize,
    total_lines: usize,
    truncated: bool,
}

fn line_window(content: &str, offset: usize, limit: usize) -> LineWindow {
    let total_lines = content.lines().count();
    let mut text = String::new();
    let mut end_line = 0;
    for (index, line) in content.lines().enumerate().skip(offset - 1).take(limit) {
        if end_line != 0 {
            text.push('\n');
        }
        text.push_str(line);
        end_line = index + 1;
    }
    let truncated = end_line != 0 && end_line < total_lines;
    if truncated {
        text.push_str(&format!(
            "\n... [lines {offset}-{end_line} of {total_lines}; continue with offset={}]",
            end_line + 1
        ));
    }
    LineWindow {
        text,
        start_line: if end_line == 0 { 0 } else { offset },
        end_line,
        total_lines,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn reads_file_inside_workspace() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("demo.txt");
        std_fs::write(&file_path, "hello\nworld").expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let result = execute(&ctx, json!({ "path": "demo.txt" }))
            .await
            .expect("read succeeds");

        assert!(result.output.contains("hello"));
        assert_eq!(result.structured["bytes"], 11);
    }

    #[test]
    fn line_window_pages_through_content() {
        let content = (1..=5)
            .map(|n| format!("l{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let first = line_window(&content, 1, 2);
        assert!(first.truncated);
        assert_eq!(
            (first.start_line, first.end_line, first.total_lines),
            (1, 2, 5)
        );
        assert!(first.text.starts_with("l1\nl2\n..."));
        let last = line_window(&content, 4, 10);
        assert!(!last.truncated);
        assert_eq!(last.text, "l4\nl5");
        let beyond = line_window(&content, 9, 3);
        assert_eq!(
            (beyond.start_line, beyond.end_line, beyond.truncated),
            (0, 0, false)
        );
    }

    #[tokio::test]
    async fn rejects_path_outside_workspace() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let error = execute(&ctx, json!({ "path": ".." }))
            .await
            .expect_err("path traversal blocked");

        match error {
            ToolError::WorkspaceBoundaryDenied(Permission::FilesystemRead) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn classifies_missing_file_as_not_found() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let error = execute(&ctx, json!({ "path": "missing.txt" }))
            .await
            .expect_err("missing file must fail");
        assert!(matches!(
            error,
            ToolError::NotFound { ref tool, ref path, .. }
                if tool == NAME && path == "missing.txt"
        ));
    }
}
