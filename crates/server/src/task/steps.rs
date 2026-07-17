//! Task plan / checkpoint / step status persistence.
use crate::app::AppState;
use crate::task::helpers::{emit_event, find_session_for_task};
use crate::ApiError;
use evohime_agent_runtime::AgentResumeContext;
use evohime_protocol::{PlanStep, ServerEvent};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

pub(crate) fn build_agent_resume_context(
    checkpoint: Option<&evohime_storage::TaskCheckpointRow>,
    task_steps: &[evohime_storage::TaskStepRow],
) -> AgentResumeContext {
    let state = checkpoint.map(|row| &row.state_json);
    let workspace_context = state
        .and_then(|value| value.get("workspace_context"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let plan = state
        .and_then(|value| value.get("plan"))
        .and_then(|value| serde_json::from_value::<Vec<PlanStep>>(value.clone()).ok());
    let pause_reason = state
        .and_then(|value| value.get("pause_reason"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut completed_step_ids = Vec::new();
    let mut tool_results = Vec::new();
    for step in task_steps {
        let plan_step_id = step
            .input_json
            .get("plan_step_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if step.status == "completed" {
            if let Some(id) = &plan_step_id {
                completed_step_ids.push(id.clone());
            }
        }
        if let (Some(id), Some(output)) = (plan_step_id, step.output.as_ref()) {
            if !output.trim().is_empty() {
                tool_results.push(format!("{id} ({}):\n{output}", step.tool_name));
            }
        }
    }
    AgentResumeContext {
        workspace_context,
        plan,
        completed_step_ids,
        tool_results,
        pause_reason,
    }
}

pub(crate) async fn persist_task_plan(
    state: &Arc<AppState>,
    task_id: Uuid,
    plan: &[PlanStep],
) -> Result<(), ApiError> {
    if !evohime_storage::list_task_steps(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .is_empty()
    {
        return Ok(());
    }

    let existing_checkpoint = evohime_storage::load_checkpoint(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let workspace_context = existing_checkpoint
        .as_ref()
        .and_then(|checkpoint| {
            checkpoint
                .state_json
                .get("workspace_context")
                .and_then(|value| value.as_str())
        })
        .map(|value| value.to_string());

    let mut step_ids = HashMap::new();
    for (index, step) in plan.iter().enumerate() {
        let depends_on = step
            .depends_on
            .iter()
            .filter_map(|dependency| step_ids.get(dependency).copied())
            .collect::<Vec<_>>();
        let input = json!({
            "plan_step_id": step.id,
            "description": step.description,
            "tool_name": step.tool_name,
            "depends_on": step.depends_on,
        });
        let row = evohime_storage::create_task_step(
            &state.pool,
            task_id,
            index as i32,
            &step.tool_name,
            &input,
            &depends_on,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
        step_ids.insert(step.id.clone(), row.id);
        emit_task_step_changed(state, task_id, row.id, "pending", step.tool_name.as_str()).await?;
    }

    let checkpoint_state = match workspace_context {
        Some(workspace_context) => json!({
            "plan": plan,
            "workspace_context": workspace_context,
            "pause_reason": Value::Null,
            "approval_wait": Value::Null,
        }),
        None => json!({
            "plan": plan,
            "pause_reason": Value::Null,
            "approval_wait": Value::Null,
        }),
    };
    evohime_storage::merge_checkpoint(&state.pool, task_id, Some(0), &checkpoint_state)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    if let Some(first_step) = plan.first() {
        if first_step.tool_name == "filesystem.read" {
            if let Some(output) = checkpoint_state
                .get("workspace_context")
                .and_then(|value| value.as_str())
            {
                if let Some(step_id) = step_ids.get(&first_step.id).copied() {
                    evohime_storage::set_step_status(
                        &state.pool,
                        step_id,
                        "completed",
                        Some(output),
                        None,
                    )
                    .await
                    .map_err(|error| ApiError::Internal(error.to_string()))?;
                    emit_task_step_changed(
                        state,
                        task_id,
                        step_id,
                        "completed",
                        first_step.tool_name.as_str(),
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}

pub(crate) async fn update_task_step_status(
    state: &Arc<AppState>,
    task_id: Uuid,
    tool_name: &str,
    status: &str,
    output: Option<&str>,
    error: Option<&str>,
) -> Result<(), ApiError> {
    let steps = evohime_storage::list_task_steps(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let step = steps
        .iter()
        .find(|step| {
            step.tool_name == tool_name
                && match status {
                    "running" => step.status == "pending",
                    "completed" | "failed" | "cancelled" => {
                        step.status == "running" || step.status == "pending"
                    }
                    _ => true,
                }
        })
        .or_else(|| {
            steps.iter().find(|step| {
                step.tool_name == tool_name
                    && match status {
                        "running" => step.status == "running",
                        "completed" | "failed" | "cancelled" => true,
                        _ => true,
                    }
            })
        });

    if let Some(step) = step {
        evohime_storage::set_step_status(&state.pool, step.id, status, output, error)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        emit_task_step_changed(state, task_id, step.id, status, tool_name).await?;
    }

    Ok(())
}

pub(crate) async fn finalize_open_task_steps(
    state: &Arc<AppState>,
    task_id: Uuid,
    status: &str,
) -> Result<(), ApiError> {
    let steps = evohime_storage::list_task_steps(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    for step in steps
        .into_iter()
        .filter(|step| step.status == "pending" || step.status == "running")
    {
        evohime_storage::set_step_status(&state.pool, step.id, status, None, None)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        emit_task_step_changed(state, task_id, step.id, status, step.tool_name.as_str()).await?;
    }

    Ok(())
}

pub(crate) async fn emit_task_step_changed(
    state: &Arc<AppState>,
    task_id: Uuid,
    step_id: Uuid,
    status: &str,
    tool_name: &str,
) -> Result<(), ApiError> {
    let session_id = find_session_for_task(state, task_id).await?;
    emit_event(
        state,
        session_id,
        Some(task_id),
        ServerEvent::TaskStepChanged {
            task_id,
            step_id,
            status: status.to_string(),
            tool_name: tool_name.to_string(),
        },
    )
    .await
    .map_err(|(_, error)| error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn merge_checkpoint_preserves_plan_when_patching_workspace() {
        let existing = json!({
            "plan": [{"id":"step-1","tool_name":"assistant.reply","description":"hi","depends_on":[]}],
            "workspace_context": "old",
        });
        let merged = evohime_storage::merge_checkpoint_state(
            &existing,
            &json!({ "workspace_context": "new" }),
        );
        assert_eq!(merged["workspace_context"], "new");
        assert!(merged.get("plan").is_some());
    }

    #[test]
    fn builds_resume_context_from_checkpoint_and_steps() {
        let checkpoint = evohime_storage::TaskCheckpointRow {
            task_id: Uuid::nil(),
            next_step: 1,
            state_json: json!({
                "workspace_context": "ctx",
                "plan": [{"id":"step-1","tool_name":"filesystem.read","description":"read","depends_on":[]}],
                "pause_reason": "approval_required",
            }),
            updated_at: chrono::Utc::now(),
        };
        let steps = vec![evohime_storage::TaskStepRow {
            id: Uuid::nil(),
            task_id: Uuid::nil(),
            step_index: 0,
            tool_name: "filesystem.read".into(),
            input_json: json!({"plan_step_id":"step-1"}),
            depends_on: vec![],
            status: "completed".into(),
            output: Some("file body".into()),
            error: None,
        }];
        let resume = build_agent_resume_context(Some(&checkpoint), &steps);
        assert_eq!(resume.workspace_context.as_deref(), Some("ctx"));
        assert_eq!(resume.completed_step_ids, vec!["step-1".to_string()]);
        assert_eq!(resume.pause_reason.as_deref(), Some("approval_required"));
        assert_eq!(resume.plan.as_ref().map(|p| p.len()), Some(1));
        assert!(resume.tool_results[0].contains("file body"));
    }
}
