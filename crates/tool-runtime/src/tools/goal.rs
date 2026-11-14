use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde_json::{json, Value};
use std::time::Duration;

/// Registry identifier for Core goal criterion verification.
pub const NAME: &str = "goal.verify";
/// Input and output summary exposed in tool catalogs.
pub const DESCRIPTION: &str = "Verify and mark a criterion of the active Core Goal as completed after checking its result with tools. Input: { goal_id, criterion_id, evidence_summary }.";
/// Permissions required before requesting goal verification (none; Core validates ownership).
pub const PERMISSIONS: &[Permission] = &[];
/// Maximum duration advertised to the tool registry.
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// Validated input for `goal.verify`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalVerifyInput {
    /// Core Goal identifier.
    pub goal_id: String,
    /// Criterion identifier within the goal's `success_criteria`.
    pub criterion_id: String,
    /// Concise summary of observed verification evidence.
    pub evidence_summary: String,
}

/// Registry placeholder; actual verification runs in `evohime-core` with journal access.
///
/// This function intentionally returns an execution error because `tool-runtime`
/// does not own the Core goal database.
pub async fn execute(_ctx: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
    Err(ToolError::Execution(
        "goal.verify is executed by the Core goal runtime".into(),
    ))
}

/// Parses and validates `goal.verify` arguments.
///
/// # Errors
///
/// Returns [`ToolError::InvalidInput`] when `goal_id`, `criterion_id`, or
/// `evidence_summary` is missing or blank.
pub fn parse_input(input: &Value) -> Result<GoalVerifyInput, ToolError> {
    let required_string = |field: &str| -> Result<String, ToolError> {
        input
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| ToolError::InvalidInput {
                tool: NAME.to_string(),
                message: format!("{field} is required"),
            })
    };

    Ok(GoalVerifyInput {
        goal_id: required_string("goal_id")?,
        criterion_id: required_string("criterion_id")?,
        evidence_summary: required_string("evidence_summary")?,
    })
}

/// Formats a Core goal verification result as readable text and structured JSON.
pub fn format_result(
    goal_id: &str,
    criterion_id: &str,
    goal_status: &str,
    completed_count: usize,
    total_count: usize,
    remaining_ids: &[String],
) -> ToolResult {
    let output = if remaining_ids.is_empty() {
        format!(
            "goal.verify: criterion '{criterion_id}' verified on goal '{goal_id}'. Goal status: {goal_status} ({completed_count}/{total_count} criteria completed). All criteria are complete."
        )
    } else {
        format!(
            "goal.verify: criterion '{criterion_id}' verified on goal '{goal_id}'. Goal status: {goal_status} ({completed_count}/{total_count} criteria completed). Remaining criteria: {}.",
            remaining_ids.join(", ")
        )
    };
    ToolResult {
        output,
        structured: json!({
            "goal_id": goal_id,
            "criterion_id": criterion_id,
            "goal_status": goal_status,
            "completed_count": completed_count,
            "total_count": total_count,
            "remaining_criteria": remaining_ids,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_input_accepts_valid_payload() {
        let parsed = parse_input(&json!({
            "goal_id": " goal-1 ",
            "criterion_id": " criterion-1 ",
            "evidence_summary": "All targeted tests passed"
        }))
        .expect("valid input");
        assert_eq!(parsed.goal_id, "goal-1");
        assert_eq!(parsed.criterion_id, "criterion-1");
        assert_eq!(parsed.evidence_summary, "All targeted tests passed");
    }

    #[test]
    fn parse_input_rejects_missing_or_blank_fields() {
        assert!(parse_input(&json!({
            "goal_id": "",
            "criterion_id": "c1",
            "evidence_summary": "ok"
        }))
        .is_err());
        assert!(parse_input(&json!({
            "goal_id": "g1",
            "evidence_summary": "ok"
        }))
        .is_err());
        assert!(parse_input(&json!({
            "goal_id": "g1",
            "criterion_id": "c1",
            "evidence_summary": "   "
        }))
        .is_err());
    }

    #[test]
    fn format_result_lists_remaining_criteria_or_completion() {
        let partial = format_result("g1", "c1", "active", 1, 2, &["c2".to_string()]);
        assert!(partial.output.contains("Remaining criteria: c2"));
        let done = format_result("g1", "c2", "completed", 2, 2, &[]);
        assert!(done.output.contains("All criteria are complete."));
    }
}
