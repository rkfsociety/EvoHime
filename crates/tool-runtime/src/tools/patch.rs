use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::fs;

pub const NAME: &str = "filesystem.patch";
pub const DESCRIPTION: &str = "Apply a unified diff to one workspace file";
pub const PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
struct Input {
    path: String,
    patch: String,
}

pub async fn execute(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|e| ToolError::InvalidInput {
        tool: NAME.into(),
        message: e.to_string(),
    })?;
    let path = ctx.sandbox()?.resolve_existing(&input.path)?;
    let original = fs::read_to_string(&path)
        .await
        .map_err(|e| ToolError::Execution(format!("read failed: {e}")))?;
    let mut lines: Vec<String> = original.lines().map(str::to_string).collect();
    if original.ends_with('\n') {
        lines.push(String::new());
    }
    let mut offset: isize = 0;
    let mut applied = 0;
    let mut hunk_lines = input.patch.lines().peekable();
    while let Some(line) = hunk_lines.next() {
        if !line.starts_with("@@") {
            continue;
        }
        let header = line
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| ToolError::InvalidInput {
                tool: NAME.into(),
                message: "malformed hunk".into(),
            })?;
        let start: usize = header
            .trim_start_matches('-')
            .split(',')
            .next()
            .unwrap()
            .parse()
            .map_err(|_| ToolError::InvalidInput {
                tool: NAME.into(),
                message: "malformed hunk range".into(),
            })?;
        let mut old_index = start.saturating_sub(1) as isize + offset;
        let mut replacements = Vec::new();
        while let Some(next) = hunk_lines.peek() {
            if next.starts_with("@@") || next.starts_with("---") || next.starts_with("+++") {
                break;
            }
            let item = hunk_lines.next().unwrap();
            if item.starts_with(' ') {
                if lines
                    .get(old_index as usize)
                    .map(|s| s != &item[1..])
                    .unwrap_or(true)
                {
                    return Err(ToolError::Execution("patch context mismatch".into()));
                }
                replacements.push(lines[old_index as usize].clone());
                old_index += 1;
            } else if item.starts_with('-') {
                if lines
                    .get(old_index as usize)
                    .map(|s| s != &item[1..])
                    .unwrap_or(true)
                {
                    return Err(ToolError::Execution("patch removal mismatch".into()));
                }
                old_index += 1;
            } else if let Some(stripped) = item.strip_prefix('+') {
                replacements.push(stripped.to_string());
            } else if !item.starts_with('\\') {
                return Err(ToolError::InvalidInput {
                    tool: NAME.into(),
                    message: "malformed diff line".into(),
                });
            }
        }
        let end = old_index as usize;
        let begin = (start.saturating_sub(1) as isize + offset) as usize;
        lines.splice(begin..end, replacements.clone());
        offset += replacements.len() as isize - (end - begin) as isize;
        applied += 1;
    }
    if applied == 0 {
        return Err(ToolError::InvalidInput {
            tool: NAME.into(),
            message: "no hunks found".into(),
        });
    }
    let mut result = lines.join("\n");
    if original.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    fs::write(&path, result.as_bytes())
        .await
        .map_err(|e| ToolError::Execution(format!("write failed: {e}")))?;
    Ok(ToolResult {
        output: format!("applied {applied} hunk(s)"),
        structured: json!({"path": input.path, "hunks_applied": applied, "bytes": result.len()}),
    })
}
