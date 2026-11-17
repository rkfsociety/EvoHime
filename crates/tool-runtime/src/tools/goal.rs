use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde_json::{json, Value};
use std::time::Duration;

/// Registry identifier for Core goal criterion verification.
pub const NAME: &str = "goal_verify";
/// Registry identifier for Core goal creation by the agent.
pub const CREATE_NAME: &str = "goal_create";
/// Input and output summary exposed in tool catalogs.
pub const DESCRIPTION: &str = "Verify and mark a criterion of the active Core Goal as completed after checking its result with tools. Input: { goal_id, criterion_id, evidence_summary }.";
/// Input and output summary for `goal_create`.
pub const CREATE_DESCRIPTION: &str = "Create and pin a Core Goal for multi-step work in the current chat. The goal belongs only to that chat and is not a workspace-wide standing goal. Input: { objective, success_criteria?: string[], continuation_budget?: integer }.";
/// Permissions required before requesting goal verification (none; Core validates ownership).
pub const PERMISSIONS: &[Permission] = &[];
/// Permissions required before requesting goal creation (none; Core validates workspace).
pub const CREATE_PERMISSIONS: &[Permission] = &[];
/// Maximum duration advertised to the tool registry.
pub const TIMEOUT: Duration = Duration::from_secs(15);
/// Maximum duration advertised for `goal_create`.
pub const CREATE_TIMEOUT: Duration = Duration::from_secs(15);
/// Default criterion statement used when `goal_create` omits `success_criteria`.
pub const DEFAULT_CRITERION_STATEMENT: &str = "Цель полностью выполнена и проверена.";

/// Validated input for `goal_create`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalCreateInput {
    /// Human-readable goal objective.
    pub objective: String,
    /// Ordered list of verifiable criterion statements (always non-empty).
    pub success_criteria: Vec<String>,
    /// Optional continuation iteration budget (`1..=50`).
    pub continuation_budget: Option<u32>,
}

/// Validated input for `goal_verify`.
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
        "goal_verify is executed by the Core goal runtime".into(),
    ))
}

/// Registry placeholder; actual creation runs in `evohime-core` with journal access.
///
/// This function intentionally returns an execution error because `tool-runtime`
/// does not own the Core goal database.
pub async fn execute_create(_ctx: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
    Err(ToolError::Execution(
        "goal_create is executed by the Core goal runtime".into(),
    ))
}

/// Parses and validates `goal_create` arguments.
///
/// # Errors
///
/// Returns [`ToolError::InvalidInput`] when `objective` is missing or blank,
/// when `success_criteria` is not an array of non-empty strings (or exceeds 16 items),
/// or when `continuation_budget` is outside `1..=50`.
pub fn parse_create_input(input: &Value) -> Result<GoalCreateInput, ToolError> {
    let objective = input
        .get("objective")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ToolError::InvalidInput {
            tool: CREATE_NAME.to_string(),
            message: "objective is required".into(),
        })?;

    let mut success_criteria = match input.get("success_criteria") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => {
            if items.len() > 16 {
                return Err(ToolError::InvalidInput {
                    tool: CREATE_NAME.to_string(),
                    message: "success_criteria must contain at most 16 items".into(),
                });
            }
            let mut parsed = Vec::with_capacity(items.len());
            for item in items {
                let text = item
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| ToolError::InvalidInput {
                        tool: CREATE_NAME.to_string(),
                        message: "each success_criteria entry must be a non-empty string".into(),
                    })?;
                parsed.push(text.to_owned());
            }
            parsed
        }
        Some(_) => {
            return Err(ToolError::InvalidInput {
                tool: CREATE_NAME.to_string(),
                message: "success_criteria must be an array of strings".into(),
            });
        }
    };
    if success_criteria.is_empty() {
        success_criteria.push(DEFAULT_CRITERION_STATEMENT.to_owned());
    }

    let continuation_budget = match input.get("continuation_budget") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let raw = value.as_u64().ok_or_else(|| ToolError::InvalidInput {
                tool: CREATE_NAME.to_string(),
                message: "continuation_budget must be a positive integer".into(),
            })?;
            if !(1..=50).contains(&raw) {
                return Err(ToolError::InvalidInput {
                    tool: CREATE_NAME.to_string(),
                    message: "continuation_budget must be between 1 and 50".into(),
                });
            }
            Some(raw as u32)
        }
    };

    Ok(GoalCreateInput {
        objective,
        success_criteria,
        continuation_budget,
    })
}

/// Formats a Core goal creation result as readable text and structured JSON.
pub fn format_create_result(
    goal_id: &str,
    objective: &str,
    goal_status: &str,
    criteria: &[(String, String)],
) -> ToolResult {
    let criteria_summary = criteria
        .iter()
        .map(|(id, statement)| format!("{id}: {statement}"))
        .collect::<Vec<_>>()
        .join("; ");
    let output = format!(
        "goal_create: created goal '{goal_id}' (status: {goal_status}) for objective '{objective}'. Criteria: {criteria_summary}. Verify each criterion with goal_verify after checking the result."
    );
    ToolResult {
        output,
        structured: json!({
            "goal_id": goal_id,
            "objective": objective,
            "goal_status": goal_status,
            "criteria": criteria.iter().map(|(id, statement)| json!({
                "id": id,
                "statement": statement,
            })).collect::<Vec<_>>(),
        }),
    }
}

/// Parses and validates `goal_verify` arguments.
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
            "goal_verify: criterion '{criterion_id}' verified on goal '{goal_id}'. Goal status: {goal_status} ({completed_count}/{total_count} criteria completed). All criteria are complete."
        )
    } else {
        format!(
            "goal_verify: criterion '{criterion_id}' verified on goal '{goal_id}'. Goal status: {goal_status} ({completed_count}/{total_count} criteria completed). Remaining criteria: {}.",
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

    #[test]
    fn parse_create_input_defaults_criteria_and_validates_fields() {
        let minimal = parse_create_input(&json!({ "objective": "  Fix module tests  " }))
            .expect("valid minimal create input");
        assert_eq!(minimal.objective, "Fix module tests");
        assert_eq!(minimal.success_criteria, vec![DEFAULT_CRITERION_STATEMENT]);
        assert_eq!(minimal.continuation_budget, None);

        let full = parse_create_input(&json!({
            "objective": "Ship feature",
            "success_criteria": ["Unit tests pass", "Docs updated"],
            "continuation_budget": 10
        }))
        .expect("valid full create input");
        assert_eq!(
            full.success_criteria,
            vec!["Unit tests pass", "Docs updated"]
        );
        assert_eq!(full.continuation_budget, Some(10));

        assert!(parse_create_input(&json!({ "objective": "   " })).is_err());
        assert!(parse_create_input(&json!({
            "objective": "Ship",
            "success_criteria": [""]
        }))
        .is_err());
        assert!(parse_create_input(&json!({
            "objective": "Ship",
            "continuation_budget": 99
        }))
        .is_err());
    }

    #[test]
    fn format_create_result_includes_goal_id_and_criteria() {
        let created = format_create_result(
            "goal-42",
            "Ship feature",
            "active",
            &[("criterion-1".to_string(), "Tests pass".to_string())],
        );
        assert!(created.output.contains("goal-42"));
        assert!(created.output.contains("criterion-1: Tests pass"));
        assert_eq!(created.structured["goal_id"], "goal-42");
    }
}
