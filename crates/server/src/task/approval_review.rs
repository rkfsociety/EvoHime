use evohime_protocol::{ApprovalReview, FileChangeKind};
use evohime_tool_runtime::{classify_call_risk, WorkspaceSandbox};
use serde_json::Value;
use std::path::Path;

pub(crate) struct ApprovalPreview {
    pub risk_level: String,
    pub review: ApprovalReview,
}

pub(crate) async fn approval_review(
    tool_name: &str,
    input: &Value,
    workspace_root: &Path,
) -> ApprovalPreview {
    let risk_level = classify_call_risk(tool_name, input).as_str().to_string();
    let review = match tool_name {
        "filesystem.patch" => unified_diff_review(input),
        "filesystem.write" => file_write_review(input, workspace_root).await,
        _ => unavailable_review(tool_name),
    };
    ApprovalPreview { risk_level, review }
}

fn unified_diff_review(input: &Value) -> ApprovalReview {
    let path = input.get("path").and_then(Value::as_str);
    let diff = input.get("patch").and_then(Value::as_str);
    match (path, diff) {
        (Some(path), Some(diff)) => ApprovalReview::UnifiedDiff {
            path: path.to_string(),
            diff: diff.to_string(),
        },
        _ => unavailable_review("filesystem.patch"),
    }
}

async fn file_write_review(input: &Value, workspace_root: &Path) -> ApprovalReview {
    let (path, content) = match (
        input.get("path").and_then(Value::as_str),
        input.get("content").and_then(Value::as_str),
    ) {
        (Some(path), Some(content)) => (path, content),
        _ => return unavailable_review("filesystem.write"),
    };

    let sandbox = match WorkspaceSandbox::new(workspace_root) {
        Ok(sandbox) => sandbox,
        Err(_) => {
            return ApprovalReview::Unavailable {
                reason: "could not resolve workspace for preview".into(),
            }
        }
    };

    let new_bytes = content.len() as u64;
    match sandbox.resolve_existing(path) {
        Ok(resolved) => match tokio::fs::metadata(&resolved).await {
            Ok(metadata) => ApprovalReview::FileWrite {
                path: path.to_string(),
                change: FileChangeKind::Overwrite,
                old_bytes: Some(metadata.len()),
                new_bytes,
            },
            Err(_) => ApprovalReview::Unavailable {
                reason: "could not read current file state".into(),
            },
        },
        Err(_) => ApprovalReview::FileWrite {
            path: path.to_string(),
            change: FileChangeKind::Create,
            old_bytes: None,
            new_bytes,
        },
    }
}

fn unavailable_review(tool_name: &str) -> ApprovalReview {
    let reason = match tool_name {
        "shell.execute" => "shell command execution cannot be safely predicted".to_string(),
        "git.push" => "git push preview is not yet supported".to_string(),
        "git.commit" => "git commit preview is not yet supported".to_string(),
        "mcp.call" => "remote MCP method effects are opaque".to_string(),
        other => format!("no preview available for {other}"),
    };
    ApprovalReview::Unavailable { reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_protocol::ApprovalReview;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn builds_unified_diff_review_from_patch_input() {
        let dir = tempdir().unwrap();
        let input = json!({
            "path": "src/lib.rs",
            "patch": "@@ -1 +1 @@\n-old\n+new"
        });

        let preview = approval_review("filesystem.patch", &input, dir.path()).await;
        assert_eq!(preview.risk_level, "medium");
        assert_eq!(
            preview.review,
            ApprovalReview::UnifiedDiff {
                path: "src/lib.rs".into(),
                diff: "@@ -1 +1 @@\n-old\n+new".into(),
            }
        );
    }

    #[tokio::test]
    async fn malformed_patch_input_degrades_to_unavailable() {
        let dir = tempdir().unwrap();
        let preview = approval_review(
            "filesystem.patch",
            &json!({"path": "src/lib.rs"}),
            dir.path(),
        )
        .await;
        assert!(matches!(preview.review, ApprovalReview::Unavailable { .. }));
    }

    #[tokio::test]
    async fn filesystem_write_to_new_path_reports_create() {
        let dir = tempdir().unwrap();
        let input = json!({"path": "new.txt", "content": "hello"});

        let preview = approval_review("filesystem.write", &input, dir.path()).await;
        assert_eq!(preview.risk_level, "medium");
        assert_eq!(
            preview.review,
            ApprovalReview::FileWrite {
                path: "new.txt".into(),
                change: evohime_protocol::FileChangeKind::Create,
                old_bytes: None,
                new_bytes: 5,
            }
        );
    }

    #[tokio::test]
    async fn filesystem_write_to_existing_path_reports_overwrite_with_old_size() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("existing.txt"), "0123456789").unwrap();
        let input = json!({"path": "existing.txt", "content": "short"});

        let preview = approval_review("filesystem.write", &input, dir.path()).await;
        assert_eq!(
            preview.review,
            ApprovalReview::FileWrite {
                path: "existing.txt".into(),
                change: evohime_protocol::FileChangeKind::Overwrite,
                old_bytes: Some(10),
                new_bytes: 5,
            }
        );
    }

    #[tokio::test]
    async fn shell_execute_reports_unavailable_with_specific_reason() {
        let dir = tempdir().unwrap();
        let preview = approval_review("shell.execute", &json!({"command": "ls"}), dir.path()).await;
        assert_eq!(preview.risk_level, "high");
        assert_eq!(
            preview.review,
            ApprovalReview::Unavailable {
                reason: "shell command execution cannot be safely predicted".into(),
            }
        );
    }

    #[tokio::test]
    async fn git_push_reports_high_risk_and_unavailable() {
        let dir = tempdir().unwrap();
        let preview = approval_review("git.push", &json!({}), dir.path()).await;
        assert_eq!(preview.risk_level, "high");
        assert!(matches!(preview.review, ApprovalReview::Unavailable { .. }));
    }
}
