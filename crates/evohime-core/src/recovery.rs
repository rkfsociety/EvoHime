use evohime_tool_runtime::{ToolError, ToolResult};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialSource {
    Policy,
    User,
    Escalation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolFailureKind {
    NotFound,
    InvalidInput,
    Denied(DenialSource),
    Timeout,
    NonZeroExit,
    Execution,
}

#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub ok: bool,
    pub kind: Option<ToolFailureKind>,
    pub output: String,
    pub structured: Value,
}

impl ToolOutcome {
    pub fn success(result: ToolResult) -> Self {
        let kind = semantic_failure(&result.structured);
        Self {
            ok: kind.is_none(),
            kind,
            output: result.output,
            structured: result.structured,
        }
    }

    pub fn from_result(result: Result<ToolResult, ToolError>) -> Self {
        match result {
            Ok(result) => Self::success(result),
            Err(error) => Self::from_error(error),
        }
    }

    pub fn from_error(error: ToolError) -> Self {
        let kind = match &error {
            ToolError::NotFound { .. } => ToolFailureKind::NotFound,
            ToolError::InvalidInput { .. } => ToolFailureKind::InvalidInput,
            ToolError::PermissionDenied(_)
            | ToolError::NeedsApproval { .. }
            | ToolError::ApprovalMismatch
            | ToolError::ApprovalDenied => {
                ToolFailureKind::Denied(DenialSource::Policy)
            }
            ToolError::TimedOut(_) => ToolFailureKind::Timeout,
            ToolError::Execution(_) | ToolError::UnknownTool(_) => ToolFailureKind::Execution,
        };
        Self {
            ok: false,
            kind: Some(kind),
            output: error.to_string(),
            structured: Value::Null,
        }
    }

    pub fn denied_by_user(output: impl Into<String>) -> Self {
        Self {
            ok: false,
            kind: Some(ToolFailureKind::Denied(DenialSource::User)),
            output: output.into(),
            structured: Value::Null,
        }
    }
}

fn semantic_failure(structured: &Value) -> Option<ToolFailureKind> {
    if structured.get("timed_out").and_then(Value::as_bool) == Some(true) {
        return Some(ToolFailureKind::Timeout);
    }
    if let Some(exit_code) = structured.get("exit_code") {
        if exit_code.as_i64() != Some(0) {
            return Some(ToolFailureKind::NonZeroExit);
        }
    }
    if structured.get("status").and_then(Value::as_str) == Some("nothing_to_commit") {
        return Some(ToolFailureKind::Execution);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn text_is_not_used_to_detect_success() {
        let outcome = ToolOutcome::success(ToolResult {
            output: "error: this is valid file content".into(),
            structured: json!({"path":"notes.txt"}),
        });
        assert!(outcome.ok);
        assert_eq!(outcome.kind, None);
    }

    #[test]
    fn shell_exit_and_timeout_are_typed() {
        assert_eq!(
            ToolOutcome::success(ToolResult {
                output: String::new(),
                structured: json!({"exit_code": 1, "timed_out": false}),
            })
            .kind,
            Some(ToolFailureKind::NonZeroExit)
        );
        assert_eq!(
            ToolOutcome::success(ToolResult {
                output: String::new(),
                structured: json!({"exit_code": null, "timed_out": true}),
            })
            .kind,
            Some(ToolFailureKind::Timeout)
        );
    }

    #[test]
    fn errors_keep_their_typed_category() {
        assert_eq!(
            ToolOutcome::from_error(ToolError::NotFound {
                tool: "filesystem.read".into(),
                path: "missing.txt".into(),
                hint: String::new(),
            })
            .kind,
            Some(ToolFailureKind::NotFound)
        );
        assert_eq!(
            ToolOutcome::from_error(ToolError::TimedOut(Duration::from_secs(1))).kind,
            Some(ToolFailureKind::Timeout)
        );
        assert_eq!(
            ToolOutcome::from_error(ToolError::ApprovalMismatch).kind,
            Some(ToolFailureKind::Denied(DenialSource::Policy))
        );
    }

    #[test]
    fn nothing_to_commit_is_not_a_successful_commit() {
        let outcome = ToolOutcome::success(ToolResult {
            output: "Изменений для коммита нет".into(),
            structured: json!({"status": "nothing_to_commit"}),
        });
        assert!(!outcome.ok);
        assert_eq!(outcome.kind, Some(ToolFailureKind::Execution));
    }
}
