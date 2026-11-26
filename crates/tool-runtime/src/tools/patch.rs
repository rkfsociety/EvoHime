use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

/// Registry identifier for unified-diff or exact-text file patching.
pub const NAME: &str = "filesystem_patch";
/// Short user-facing summary shown in tool catalogs.
pub const DESCRIPTION: &str =
    "Apply a unified diff or exact old_text/new_text replacement to one workspace file";
/// Permission required to modify workspace contents.
pub const PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
/// Maximum duration for one patch operation.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// Maximum UTF-8 byte length accepted for a patch payload.
pub const MAX_PATCH_BYTES: usize = 131_072;

#[derive(Deserialize)]
struct Input {
    path: String,
    #[serde(default)]
    patch: Option<String>,
    #[serde(default)]
    old_text: Option<String>,
    #[serde(default)]
    new_text: Option<String>,
    expected_hash: String,
}

enum PatchMode {
    UnifiedDiff(String),
    ExactReplace { old_text: String, new_text: String },
}

fn invalid_input(message: impl Into<String>) -> ToolError {
    ToolError::InvalidInput {
        tool: NAME.into(),
        message: message.into(),
    }
}

fn validate_payload_bytes(payload: &str, label: &str) -> Result<(), ToolError> {
    if payload.len() > MAX_PATCH_BYTES {
        return Err(invalid_input(format!(
            "{label} exceeds 131072-byte limit; split it into smaller patches"
        )));
    }
    Ok(())
}

fn parse_mode(input: &Input) -> Result<PatchMode, ToolError> {
    if let (Some(old_text), Some(new_text)) = (&input.old_text, &input.new_text) {
        if old_text.is_empty() {
            return Err(invalid_input("old_text must not be empty"));
        }
        validate_payload_bytes(old_text, "old_text")?;
        validate_payload_bytes(new_text, "new_text")?;
        return Ok(PatchMode::ExactReplace {
            old_text: old_text.clone(),
            new_text: new_text.clone(),
        });
    }
    if input.old_text.is_some() || input.new_text.is_some() {
        return Err(invalid_input(
            "both old_text and new_text must be provided together",
        ));
    }
    if let Some(patch) = &input.patch {
        validate_payload_bytes(patch, "patch")?;
        return Ok(PatchMode::UnifiedDiff(patch.clone()));
    }
    Err(invalid_input(
        "provide either `patch` (unified diff) or both `old_text` and `new_text`",
    ))
}

fn parse_input(value: Value) -> Result<(Input, PatchMode), ToolError> {
    let input: Input =
        serde_json::from_value(value).map_err(|error| invalid_input(error.to_string()))?;
    let mode = parse_mode(&input)?;
    Ok((input, mode))
}

pub(crate) fn validate_input(value: &Value) -> Result<(), ToolError> {
    let input: Input =
        serde_json::from_value(value.clone()).map_err(|error| invalid_input(error.to_string()))?;
    parse_mode(&input).map(|_| ())
}

fn strip_cr(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

fn split_lines_preserving_trailing(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if text.ends_with('\n') {
        lines.push(String::new());
    }
    lines
}

fn parse_hunk_start(header_line: &str) -> Result<Option<usize>, ToolError> {
    let mut parts = header_line.split_whitespace();
    let _marker = parts.next();
    let Some(range) = parts.next() else {
        return Ok(None);
    };
    if range == "@@" || !range.starts_with('-') {
        return Ok(None);
    }
    let start: usize = range
        .trim_start_matches('-')
        .split(',')
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| invalid_input("malformed hunk range"))?;
    Ok(Some(start))
}

struct ParsedHunk {
    hint_start_1based: Option<usize>,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
}

fn parse_hunks(patch: &str) -> Result<Vec<ParsedHunk>, ToolError> {
    let mut hunks = Vec::new();
    let mut hunk_lines = patch.lines().peekable();
    while let Some(raw_line) = hunk_lines.next() {
        let line = strip_cr(raw_line);
        if !line.starts_with("@@") {
            continue;
        }
        let hint_start_1based = parse_hunk_start(line)?;
        let mut old_lines = Vec::new();
        let mut new_lines = Vec::new();
        while let Some(raw_next) = hunk_lines.peek() {
            let next = strip_cr(raw_next);
            if next.starts_with("@@") || next.starts_with("---") || next.starts_with("+++") {
                break;
            }
            let Some(raw_item) = hunk_lines.next() else {
                break;
            };
            let item = strip_cr(raw_item);
            if let Some(rest) = item.strip_prefix(' ') {
                old_lines.push(rest.to_string());
                new_lines.push(rest.to_string());
            } else if let Some(rest) = item.strip_prefix('-') {
                old_lines.push(rest.to_string());
            } else if let Some(rest) = item.strip_prefix('+') {
                new_lines.push(rest.to_string());
            } else if item.is_empty() {
                // Models sometimes omit the leading space on blank context lines.
                old_lines.push(String::new());
                new_lines.push(String::new());
            } else if !item.starts_with('\\') {
                return Err(invalid_input("malformed diff line"));
            }
        }
        hunks.push(ParsedHunk {
            hint_start_1based,
            old_lines,
            new_lines,
        });
    }
    if hunks.is_empty() {
        return Err(invalid_input("no hunks found"));
    }
    Ok(hunks)
}

fn find_hunk_position(
    lines: &[String],
    old_lines: &[String],
    hint_begin: Option<isize>,
) -> Result<usize, ToolError> {
    if old_lines.is_empty() {
        let Some(pos) = hint_begin else {
            return Err(ToolError::Execution(
                "patch context mismatch: insertion hunk has no line number and no context".into(),
            ));
        };
        let idx = usize::try_from(pos)
            .map_err(|_| ToolError::Execution("patch start is outside file".into()))?;
        if idx > lines.len() {
            return Err(ToolError::Execution("patch range is out of bounds".into()));
        }
        return Ok(idx);
    }

    if let Some(pos) = hint_begin {
        if let Ok(idx) = usize::try_from(pos) {
            if idx + old_lines.len() <= lines.len()
                && lines[idx..idx + old_lines.len()] == *old_lines
            {
                return Ok(idx);
            }
        }
    }

    let mut matches = Vec::new();
    if old_lines.len() <= lines.len() {
        for idx in 0..=lines.len() - old_lines.len() {
            if lines[idx..idx + old_lines.len()] == *old_lines {
                matches.push(idx);
            }
        }
    }

    match matches.len() {
        0 => {
            let expected = old_lines.first().map(String::as_str).unwrap_or("");
            let hint_info = match hint_begin.and_then(|pos| usize::try_from(pos).ok()) {
                Some(idx) => match lines.get(idx) {
                    Some(actual) => format!(
                        " at line {}: expected {:?}, found {:?}",
                        idx + 1,
                        expected,
                        actual
                    ),
                    None => format!(" at line {} (file has only {} lines)", idx + 1, lines.len()),
                },
                None => String::new(),
            };
            Err(ToolError::Execution(format!(
                "patch context mismatch{hint_info}; verify exact lines with filesystem_read or use old_text/new_text"
            )))
        }
        1 => Ok(matches[0]),
        _ => {
            if let Some(hint) = hint_begin {
                let mut best_idx = matches[0];
                let mut best_dist = (matches[0] as isize - hint).unsigned_abs();
                let mut tied = false;
                for &idx in &matches[1..] {
                    let dist = (idx as isize - hint).unsigned_abs();
                    if dist < best_dist {
                        best_dist = dist;
                        best_idx = idx;
                        tied = false;
                    } else if dist == best_dist {
                        tied = true;
                    }
                }
                if !tied {
                    return Ok(best_idx);
                }
            }
            let line_list = matches
                .iter()
                .map(|idx| (idx + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Err(ToolError::Execution(format!(
                "patch context mismatch: hunk matched multiple places (lines {line_list}); include more context lines"
            )))
        }
    }
}

fn apply_unified_diff(
    original: &str,
    patch: &str,
    line_ending: &str,
) -> Result<(String, usize), ToolError> {
    let hunks = parse_hunks(patch)?;
    let mut lines = split_lines_preserving_trailing(original);
    let mut offset: isize = 0;
    let mut applied = 0;

    for hunk in hunks {
        let hint_begin = hunk
            .hint_start_1based
            .map(|start| start.saturating_sub(1) as isize + offset);
        let begin = find_hunk_position(&lines, &hunk.old_lines, hint_begin)?;
        let end = begin + hunk.old_lines.len();
        let new_len = hunk.new_lines.len();
        lines.splice(begin..end, hunk.new_lines);
        offset += new_len as isize - (end - begin) as isize;
        applied += 1;
    }

    let mut result = lines.join(line_ending);
    if original.ends_with('\n') && !result.ends_with('\n') {
        result.push_str(line_ending);
    }
    Ok((result, applied))
}

fn apply_exact_replace(
    original: &str,
    old_text: &str,
    new_text: &str,
    line_ending: &str,
) -> Result<String, ToolError> {
    let matches: Vec<usize> = original
        .match_indices(old_text)
        .map(|(idx, _)| idx)
        .collect();
    if matches.len() == 1 {
        let idx = matches[0];
        let normalized_new = if line_ending == "\r\n" && !new_text.contains("\r\n") {
            new_text.replace('\n', "\r\n")
        } else {
            new_text.to_string()
        };
        let mut result = String::with_capacity(original.len() + normalized_new.len());
        result.push_str(&original[..idx]);
        result.push_str(&normalized_new);
        result.push_str(&original[idx + old_text.len()..]);
        return Ok(result);
    }
    if matches.len() > 1 {
        return Err(ToolError::Execution(format!(
            "patch context mismatch: old_text matched {} places; include more surrounding lines to make it unique",
            matches.len()
        )));
    }

    // Fallback: compare with normalized LF line endings so Windows CRLF files
    // still match LF snippets generated by models.
    let norm_original = original.replace("\r\n", "\n");
    let norm_old = old_text.replace("\r\n", "\n");
    let norm_matches: Vec<usize> = norm_original
        .match_indices(&norm_old)
        .map(|(idx, _)| idx)
        .collect();
    match norm_matches.len() {
        0 => Err(ToolError::Execution(
            "patch context mismatch: old_text was not found in file; verify exact text with filesystem_read"
                .into(),
        )),
        1 => {
            let idx = norm_matches[0];
            let norm_new = new_text.replace("\r\n", "\n");
            let mut replaced = String::with_capacity(norm_original.len() + norm_new.len());
            replaced.push_str(&norm_original[..idx]);
            replaced.push_str(&norm_new);
            replaced.push_str(&norm_original[idx + norm_old.len()..]);
            if line_ending == "\r\n" {
                Ok(replaced.replace('\n', "\r\n"))
            } else {
                Ok(replaced)
            }
        }
        count => Err(ToolError::Execution(format!(
            "patch context mismatch: old_text matched {count} places; include more surrounding lines to make it unique"
        ))),
    }
}

/// Applies a unified diff or exact text replacement only when the target's
/// current revision matches the supplied expected hash (when provided).
///
/// The file is read and written through the revision-safe workspace boundary;
/// invalid hunks, oversized patches, missing files, and stale hashes are
/// rejected without applying a partial patch.
///
/// # Errors
///
/// Returns [`ToolError`] for invalid input, denied access, missing files,
/// mismatched revisions, malformed hunks, or write failures.
pub async fn execute(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let (input, mode) = parse_input(value)?;
    let (_, original) = crate::revision_safe_workspace_files::read(ctx, &input.path)
        .await
        .map_err(|error| {
            if matches!(error, crate::revision_safe_workspace_files::RevisionError::Io(ref io) if io.kind() == std::io::ErrorKind::NotFound) {
                return ToolError::NotFound {
                    tool: NAME.to_string(),
                    path: input.path.clone(),
                    hint: String::new(),
                };
            }
            crate::revision_safe_workspace_files::permission(
                error,
                NAME,
                Permission::FilesystemWrite,
            )
        })?;

    let line_ending = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    let (result, hunks_applied) = match mode {
        PatchMode::UnifiedDiff(patch) => apply_unified_diff(&original, &patch, line_ending)?,
        PatchMode::ExactReplace { old_text, new_text } => (
            apply_exact_replace(&original, &old_text, &new_text, line_ending)?,
            1,
        ),
    };

    let file_ref = crate::revision_safe_workspace_files::write(
        ctx,
        &input.path,
        result.as_bytes(),
        Some(&input.expected_hash),
    )
    .await
    .map_err(|error| {
        crate::revision_safe_workspace_files::permission(error, NAME, Permission::FilesystemWrite)
    })?;
    Ok(ToolResult {
        output: format!("applied {hunks_applied} hunk(s)"),
        structured: json!({"path": input.path, "hunks_applied": hunks_applied, "bytes": result.len(), "content_hash": file_ref.content_hash, "revision": file_ref.revision, "namespace": file_ref.namespace.as_str(), "change_set": {"status": "observed", "path": file_ref.path}}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::tempdir;

    fn test_ctx(root: &std::path::Path) -> ToolContext {
        ToolContext {
            workspace_root: root.to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
            eva_workspace_root: None,
            self_development_profile: None,
            agent_depth: 1,
            self_development_test_gate: std::sync::Arc::new(tokio::sync::Mutex::new(
                crate::tools::self_development::TestGate::default(),
            )),
        }
    }

    #[test]
    fn accepts_patch_at_byte_limit() {
        let value = json!({"path": "src/lib.rs", "patch": "a".repeat(MAX_PATCH_BYTES), "expected_hash": "hash"});

        assert!(validate_input(&value).is_ok());
    }

    #[test]
    fn rejects_patch_above_byte_limit() {
        let value = json!({"path": "src/lib.rs", "patch": "a".repeat(MAX_PATCH_BYTES + 1), "expected_hash": "hash"});

        let error = validate_input(&value).expect_err("oversized patch must fail");

        assert!(matches!(
            error,
            ToolError::InvalidInput { message, .. }
                if message == "patch exceeds 131072-byte limit; split it into smaller patches"
        ));
    }

    #[test]
    fn counts_utf8_bytes_not_characters() {
        let value = json!({"path": "src/lib.rs", "patch": "я".repeat((MAX_PATCH_BYTES / 2) + 1), "expected_hash": "hash"});

        assert!(validate_input(&value).is_err());
    }

    #[test]
    fn accepts_old_text_and_new_text_mode() {
        let value = json!({
            "path": "README.md",
            "old_text": "hello",
            "new_text": "world",
            "expected_hash": "hash"
        });
        assert!(validate_input(&value).is_ok());
    }

    #[test]
    fn rejects_incomplete_replace_fields() {
        let missing_new =
            json!({"path": "README.md", "old_text": "hello", "expected_hash": "hash"});
        assert!(validate_input(&missing_new).is_err());

        let empty_old =
            json!({"path": "README.md", "old_text": "", "new_text": "x", "expected_hash": "hash"});
        assert!(validate_input(&empty_old).is_err());
    }

    #[tokio::test]
    async fn unified_diff_recovers_when_hunk_line_number_is_wrong() {
        let dir = tempdir().expect("tempdir");
        let content = (1..=30)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std_fs::write(dir.path().join("README.md"), &content).expect("write");

        let ctx = test_ctx(dir.path());
        let read_res = crate::tools::filesystem::execute(&ctx, json!({"path": "README.md"}))
            .await
            .expect("read succeeds");
        let hash = read_res.structured["content_hash"]
            .as_str()
            .expect("hash")
            .to_string();

        let patch = "@@ -15,3 +15,3 @@\n line 23\n-line 24\n+line 24 updated\n line 25\n";
        let result = execute(
            &ctx,
            json!({"path": "README.md", "patch": patch, "expected_hash": hash}),
        )
        .await
        .expect("shifted hunk should succeed");
        assert_eq!(result.structured["hunks_applied"], 1);

        let updated = std_fs::read_to_string(dir.path().join("README.md")).expect("read");
        assert!(updated.contains("line 23\nline 24 updated\nline 25"));
    }

    #[tokio::test]
    async fn exact_replace_replaces_unique_snippet_and_preserves_crlf() {
        let dir = tempdir().expect("tempdir");
        std_fs::write(dir.path().join("win.txt"), "alpha\r\nbeta\r\ngamma\r\n").expect("write");

        let ctx = test_ctx(dir.path());
        let read_res = crate::tools::filesystem::execute(&ctx, json!({"path": "win.txt"}))
            .await
            .expect("read succeeds");
        let hash = read_res.structured["content_hash"]
            .as_str()
            .expect("hash")
            .to_string();

        let result = execute(
            &ctx,
            json!({
                "path": "win.txt",
                "old_text": "alpha\nbeta",
                "new_text": "alpha\nbeta2",
                "expected_hash": hash
            }),
        )
        .await
        .expect("exact replace succeeds");
        assert_eq!(result.structured["hunks_applied"], 1);

        let updated = std_fs::read_to_string(dir.path().join("win.txt")).expect("read");
        assert_eq!(updated, "alpha\r\nbeta2\r\ngamma\r\n");
    }

    #[tokio::test]
    async fn exact_replace_rejects_ambiguous_snippet() {
        let dir = tempdir().expect("tempdir");
        std_fs::write(dir.path().join("dup.txt"), "dup\nmid\ndup\n").expect("write");

        let ctx = test_ctx(dir.path());
        let err = execute(
            &ctx,
            json!({"path": "dup.txt", "old_text": "dup", "new_text": "one", "expected_hash": "hash"}),
        )
        .await
        .expect_err("ambiguous snippet must fail");
        assert!(err.to_string().contains("matched 2 places"));
    }
}
