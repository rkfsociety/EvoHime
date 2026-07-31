use crate::reflection::{ReflectionEngine, ToolOutputContext};
use evohime_protocol::{ReflectionType, ReflectionAction, ReflectionAnalysis};
use chrono::Utc;
use uuid::Uuid;

pub struct ReflectionStageInput {
    pub task_id: Uuid,
    pub tool_call_id: Uuid,
    pub tool_name: String,
    pub tool_output: String,
    pub tool_error: Option<String>,
}

pub struct ReflectionStageOutput {
    pub task_id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub reflection_type: ReflectionType,
    pub tool_call_id: Option<Uuid>,
    pub analysis: ReflectionAnalysis,
    pub action: ReflectionAction,
    pub should_continue: bool,
    pub should_revise_plan: bool,
}

pub struct ReflectionStage;

impl ReflectionStage {
    pub async fn execute(input: ReflectionStageInput) -> ReflectionStageOutput {
        let context = ToolOutputContext {
            tool_name: input.tool_name,
            tool_input: serde_json::json!({}),
            tool_output: input.tool_output.clone(),
            tool_error: input.tool_error.clone(),
            expected_outcome: None,
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![]);

        let should_continue = matches!(action, ReflectionAction::Proceed);
        let should_revise_plan = matches!(action, ReflectionAction::RevisePlan);

        ReflectionStageOutput {
            task_id: input.task_id,
            timestamp: Utc::now(),
            reflection_type: ReflectionType::PostToolExecution,
            tool_call_id: Some(input.tool_call_id),
            analysis,
            action,
            should_continue,
            should_revise_plan,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reflection_stage_on_error() {
        let input = ReflectionStageInput {
            task_id: Uuid::new_v4(),
            tool_call_id: Uuid::new_v4(),
            tool_name: "filesystem.read".to_string(),
            tool_output: String::new(),
            tool_error: Some("Not found".to_string()),
        };

        let output = ReflectionStage::execute(input).await;
        assert!(!output.should_continue);
        assert_eq!(output.analysis.success_score, 0.0);
    }

    #[tokio::test]
    async fn test_reflection_stage_on_success() {
        let input = ReflectionStageInput {
            task_id: Uuid::new_v4(),
            tool_call_id: Uuid::new_v4(),
            tool_name: "filesystem.read".to_string(),
            tool_output: "success".to_string(),
            tool_error: None,
        };

        let output = ReflectionStage::execute(input).await;
        assert!(output.should_continue);
        assert!(output.analysis.success_score > 0.8);
    }
}
