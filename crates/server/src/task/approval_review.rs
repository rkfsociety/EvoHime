use evohime_protocol::ApprovalReview;
use serde_json::Value;

pub(crate) fn approval_review(tool_name: &str, input: &Value) -> Option<ApprovalReview> {
    if tool_name != "filesystem.patch" {
        return None;
    }

    let path = input.get("path")?.as_str()?;
    let diff = input.get("patch")?.as_str()?;
    Some(ApprovalReview::UnifiedDiff {
        path: path.to_string(),
        diff: diff.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_unified_diff_review_from_patch_input() {
        let input = json!({
            "path": "src/lib.rs",
            "patch": "@@ -1 +1 @@\n-old\n+new"
        });

        assert_eq!(
            approval_review("filesystem.patch", &input),
            Some(ApprovalReview::UnifiedDiff {
                path: "src/lib.rs".into(),
                diff: "@@ -1 +1 @@\n-old\n+new".into(),
            })
        );
    }

    #[test]
    fn ignores_non_patch_tools_and_malformed_input() {
        assert_eq!(
            approval_review(
                "filesystem.write",
                &json!({"path": "src/lib.rs", "content": "new"})
            ),
            None
        );
        assert_eq!(
            approval_review("filesystem.patch", &json!({"path": "src/lib.rs"})),
            None
        );
        assert_eq!(
            approval_review("filesystem.patch", &json!({"patch": "@@ -1 +1 @@"})),
            None
        );
    }
}
