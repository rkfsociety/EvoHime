//! Post-tool-execution reflection stage (roadmap 8.2).
//!
//! Runs after every ReAct tool observation: pulls remembered failure lessons from
//! experience memory (6.21), scores the observation, persists the verdict and
//! returns the action the loop must take.

use crate::reflection::{ReflectionEngine, ToolOutputContext};
use chrono::Utc;
use evohime_protocol::{ReflectionAction, ReflectionAnalysis, ReflectionType};
use evohime_storage::{
    list_memory_items_for_operator, MemoryKind, MemoryScope, MemoryStatus, ReflectionEventDAO,
    LOCAL_OPERATOR_SCOPE_KEY,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// Upper bound on remembered lessons fed into one reflection: matching is linear
/// and the loop runs after every tool call.
const MAX_FAILURE_PATTERNS: i64 = 24;

/// Consecutive failing steps after which the loop is told to revise the plan
/// instead of retrying the same approach.
const REVISE_AFTER_CONSECUTIVE_FAILURES: usize = 3;

pub struct ReflectionStageInput {
    pub task_id: Uuid,
    pub operator_id: Uuid,
    /// Native tool-call id as issued by the model (not a UUID).
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_output: String,
    pub tool_error: Option<String>,
    /// Failing tool observations seen in a row before this one.
    pub consecutive_failures: usize,
}

pub struct ReflectionStageOutput {
    pub task_id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub reflection_type: ReflectionType,
    pub tool_call_id: Option<Uuid>,
    pub analysis: ReflectionAnalysis,
    pub action: ReflectionAction,
    pub recommendation: Option<String>,
    pub should_continue: bool,
    pub should_revise_plan: bool,
}

pub struct ReflectionStage;

impl ReflectionStage {
    /// `pool` is optional: without a database the stage still scores the step, it
    /// just has no remembered patterns and nothing to persist.
    pub async fn execute(
        pool: Option<&PgPool>,
        input: ReflectionStageInput,
    ) -> ReflectionStageOutput {
        let patterns = match pool {
            Some(pool) => load_failure_patterns(pool, input.operator_id).await,
            None => Vec::new(),
        };

        let context = ToolOutputContext {
            tool_name: input.tool_name.clone(),
            tool_input: input.tool_input,
            tool_output: input.tool_output,
            tool_error: input.tool_error,
            expected_outcome: None,
        };

        let (analysis, mut action) = ReflectionEngine::analyze_tool_output(&context, patterns);
        let failing = !matches!(action, ReflectionAction::Proceed);
        let consecutive_failures = if failing {
            input.consecutive_failures + 1
        } else {
            0
        };
        if failing && consecutive_failures >= REVISE_AFTER_CONSECUTIVE_FAILURES {
            action = ReflectionAction::RevisePlan;
        }

        let recommendation = ReflectionEngine::recommendation(&action, &analysis, &input.tool_name);
        let should_continue = matches!(action, ReflectionAction::Proceed);
        let should_revise_plan =
            ReflectionEngine::should_revise_plan(&action, input.consecutive_failures);
        let tool_call_id = Some(reflection_call_uuid(&input.tool_call_id));

        let output = ReflectionStageOutput {
            task_id: input.task_id,
            timestamp: Utc::now(),
            reflection_type: ReflectionType::PostToolExecution,
            tool_call_id,
            analysis,
            action,
            recommendation,
            should_continue,
            should_revise_plan,
        };

        if let Some(pool) = pool {
            persist(pool, &output).await;
        }
        output
    }
}

/// Stable UUID for a model-issued tool-call id, so a reflection row can be joined
/// back to the call it judged without changing the protocol to strings.
pub fn reflection_call_uuid(tool_call_id: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, tool_call_id.as_bytes())
}

async fn load_failure_patterns(pool: &PgPool, operator_id: Uuid) -> Vec<(String, String, f64)> {
    let rows = match list_memory_items_for_operator(
        pool,
        operator_id,
        MemoryScope::Experience,
        LOCAL_OPERATOR_SCOPE_KEY,
        &[MemoryStatus::Active, MemoryStatus::Candidate],
        MAX_FAILURE_PATTERNS,
    )
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "reflection: failed to load experience patterns");
            return Vec::new();
        }
    };

    rows.into_iter()
        .filter(|row| {
            matches!(
                MemoryKind::parse(&row.kind),
                Some(MemoryKind::FailurePattern) | Some(MemoryKind::VerificationRule)
            )
        })
        .map(|row| (row.id.to_string(), row.content, row.confidence))
        .collect()
}

async fn persist(pool: &PgPool, output: &ReflectionStageOutput) {
    let error_patterns = serde_json::to_value(&output.analysis.error_patterns)
        .unwrap_or_else(|_| Value::Array(Vec::new()));
    let dao = ReflectionEventDAO::new(pool.clone());
    if let Err(error) = dao
        .insert_reflection_event(
            Uuid::new_v4(),
            output.task_id,
            output.tool_call_id,
            reflection_type_str(&output.reflection_type),
            reflection_action_str(&output.action),
            output.analysis.success_score,
            &error_patterns,
            output.analysis.confidence,
            &output.analysis.reasoning,
            output.recommendation.as_deref(),
        )
        .await
    {
        tracing::warn!(%error, "reflection: failed to persist reflection event");
    }
}

fn reflection_type_str(value: &ReflectionType) -> &'static str {
    match value {
        ReflectionType::PostToolExecution => "post_tool_execution",
        ReflectionType::PlanRevision => "plan_revision",
        ReflectionType::ErrorRecovery => "error_recovery",
    }
}

fn reflection_action_str(value: &ReflectionAction) -> &'static str {
    match value {
        ReflectionAction::Proceed => "proceed",
        ReflectionAction::AskUser => "ask_user",
        ReflectionAction::RetryTool => "retry_tool",
        ReflectionAction::RevisePlan => "revise_plan",
        ReflectionAction::Escalate => "escalate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        tool_error: Option<&str>,
        output: &str,
        consecutive_failures: usize,
    ) -> ReflectionStageInput {
        ReflectionStageInput {
            task_id: Uuid::new_v4(),
            operator_id: Uuid::new_v4(),
            tool_call_id: "call_1".to_string(),
            tool_name: "filesystem.read".to_string(),
            tool_input: serde_json::json!({"path": "docs/readme.md"}),
            tool_output: output.to_string(),
            tool_error: tool_error.map(str::to_string),
            consecutive_failures,
        }
    }

    #[tokio::test]
    async fn test_reflection_stage_on_error() {
        let output = ReflectionStage::execute(None, input(Some("Not found"), "", 0)).await;

        assert!(!output.should_continue);
        assert_eq!(output.analysis.success_score, 0.0);
        assert_eq!(output.action, ReflectionAction::RetryTool);
        assert!(output.recommendation.is_some());
    }

    #[tokio::test]
    async fn test_reflection_stage_on_success() {
        let output = ReflectionStage::execute(None, input(None, "file contents", 0)).await;

        assert!(output.should_continue);
        assert!(output.analysis.success_score > 0.8);
        assert!(output.recommendation.is_none());
    }

    #[tokio::test]
    async fn repeated_failures_switch_to_plan_revision() {
        let output = ReflectionStage::execute(None, input(Some("Not found"), "", 2)).await;

        assert_eq!(output.action, ReflectionAction::RevisePlan);
        assert!(output.should_revise_plan);
    }

    #[test]
    fn call_uuid_is_stable_per_tool_call_id() {
        assert_eq!(
            reflection_call_uuid("call_1"),
            reflection_call_uuid("call_1")
        );
        assert_ne!(
            reflection_call_uuid("call_1"),
            reflection_call_uuid("call_2")
        );
    }
}
