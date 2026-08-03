use crate::planning_graph::{validate_and_sort_graph, GraphError};
use evohime_protocol::{PlanGenerationResponse, PlanStep};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum PlanGenerationError {
    #[error("plan generation failed: {0}")]
    GenerationFailed(String),
    #[error("invalid plan graph: {0}")]
    InvalidGraph(String),
}

/// Build an executable plan from LLM response, with BLOCKER #1 fix:
/// Materialize implicit sequential dependencies for empty depends_on fields.
/// This handles both legacy linear plans and cyclic fallback scenarios.
pub async fn build_executable_plan(
    response: PlanGenerationResponse,
) -> Result<Vec<PlanStep>, PlanGenerationError> {
    // Check plan size limit (max 50 steps)
    if response.steps.len() > 50 {
        return Err(PlanGenerationError::GenerationFailed(format!(
            "plan exceeds maximum 50 steps (got {})",
            response.steps.len()
        )));
    }

    if response.steps.is_empty() {
        return Ok(vec![]);
    }

    // Validate graph structure
    let mut dep_map = HashMap::new();
    for step in &response.steps {
        dep_map.insert(step.id.clone(), step.depends_on.clone());
    }

    match validate_and_sort_graph(&dep_map) {
        Ok(graph) => {
            // Check if this is a legacy linear plan (all steps have no dependencies)
            let is_legacy_linear = response.steps.iter().all(|s| s.depends_on.is_empty());

            let mut ordered_steps = Vec::new();
            for (idx, step_id) in graph.topological_order.iter().enumerate() {
                let mut step = response.steps.iter().find(|s| &s.id == step_id)
                    .cloned()
                    .ok_or_else(|| PlanGenerationError::InvalidGraph(
                        format!("step {} in topological order not found", step_id)
                    ))?;

                // BLOCKER #1 FIX: Materialize implicit sequential deps ONLY for legacy linear plans
                // If plan has explicit dependencies, keep them as-is.
                // Only materialize for backward compat when ALL steps have empty depends_on.
                if is_legacy_linear && step.depends_on.is_empty() && idx > 0 {
                    step.depends_on = vec![graph.topological_order[idx - 1].clone()];
                }

                ordered_steps.push(step);
            }
            Ok(ordered_steps)
        }
        Err(GraphError::CycleDetected) => {
            tracing::warn!("LLM generated cyclic dependencies; falling back to linear plan");
            // Fallback: clear all depends_on, then materialize sequential deps
            let mut steps = response.steps.clone();
            for step in &mut steps {
                step.depends_on = vec![];
            }
            // Now materialize sequential dependencies
            for idx in 1..steps.len() {
                steps[idx].depends_on = vec![steps[idx - 1].id.clone()];
            }
            Ok(steps)
        }
        Err(GraphError::MissingDependency(ref_id)) => {
            Err(PlanGenerationError::InvalidGraph(format!(
                "plan references non-existent step: {}",
                ref_id
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_materialized_sequential_deps() {
        let response = PlanGenerationResponse {
            plan_title: "Linear plan test".to_string(),
            reasoning: "Test".to_string(),
            steps: vec![
                PlanStep {
                    id: "step-1".to_string(),
                    tool_name: "filesystem.read".to_string(),
                    description: "Read".to_string(),
                    depends_on: vec![],
                },
                PlanStep {
                    id: "step-2".to_string(),
                    tool_name: "filesystem.patch".to_string(),
                    description: "Patch".to_string(),
                    depends_on: vec![],
                },
                PlanStep {
                    id: "step-3".to_string(),
                    tool_name: "shell.execute".to_string(),
                    description: "Test".to_string(),
                    depends_on: vec![],
                },
            ],
        };

        let plan = build_executable_plan(response).await.expect("build plan");

        // Verify materialization: empty depends_on became sequential deps
        assert_eq!(plan[0].depends_on, Vec::<String>::new()); // first step has no deps
        assert_eq!(plan[1].depends_on, vec!["step-1".to_string()]); // depends on previous
        assert_eq!(plan[2].depends_on, vec!["step-2".to_string()]); // depends on previous
    }

    #[tokio::test]
    async fn test_explicit_dependencies_preserved() {
        let response = PlanGenerationResponse {
            plan_title: "Graph plan test".to_string(),
            reasoning: "Test".to_string(),
            steps: vec![
                PlanStep {
                    id: "a".to_string(),
                    tool_name: "sh".to_string(),
                    description: "A".to_string(),
                    depends_on: vec![],
                },
                PlanStep {
                    id: "b".to_string(),
                    tool_name: "sh".to_string(),
                    description: "B".to_string(),
                    depends_on: vec!["a".to_string()],
                },
                PlanStep {
                    id: "c".to_string(),
                    tool_name: "sh".to_string(),
                    description: "C".to_string(),
                    depends_on: vec!["a".to_string()],
                },
                PlanStep {
                    id: "d".to_string(),
                    tool_name: "sh".to_string(),
                    description: "D".to_string(),
                    depends_on: vec!["b".to_string(), "c".to_string()],
                },
            ],
        };

        let plan = build_executable_plan(response).await.expect("build plan");

        // Verify explicit dependencies are preserved
        assert_eq!(plan.iter().find(|s| s.id == "b").unwrap().depends_on, vec!["a"]);
        assert_eq!(plan.iter().find(|s| s.id == "c").unwrap().depends_on, vec!["a"]);
        assert_eq!(
            plan.iter().find(|s| s.id == "d").unwrap().depends_on,
            vec!["b", "c"]
        );
    }

    #[tokio::test]
    async fn test_plan_size_limit() {
        let mut steps = vec![];
        for i in 0..51 {
            steps.push(PlanStep {
                id: format!("step-{}", i),
                tool_name: "sh".to_string(),
                description: "X".to_string(),
                depends_on: vec![],
            });
        }

        let response = PlanGenerationResponse {
            plan_title: "Too big".to_string(),
            reasoning: "Test".to_string(),
            steps,
        };

        let result = build_executable_plan(response).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum 50 steps"));
    }

    #[tokio::test]
    async fn test_cyclic_graph_fallback() {
        let response = PlanGenerationResponse {
            plan_title: "Cycle test".to_string(),
            reasoning: "Test".to_string(),
            steps: vec![
                PlanStep {
                    id: "x".to_string(),
                    tool_name: "sh".to_string(),
                    description: "X".to_string(),
                    depends_on: vec!["y".to_string()], // cycle: x depends on y
                },
                PlanStep {
                    id: "y".to_string(),
                    tool_name: "sh".to_string(),
                    description: "Y".to_string(),
                    depends_on: vec!["x".to_string()], // cycle: y depends on x
                },
            ],
        };

        let plan = build_executable_plan(response).await.expect("fallback to linear");

        // After fallback, all steps should have materialized sequential deps
        assert_eq!(plan[0].depends_on, Vec::<String>::new()); // first step no deps
        assert_eq!(plan[1].depends_on, vec!["x".to_string()]); // second depends on first
    }

    #[tokio::test]
    async fn test_empty_plan() {
        let response = PlanGenerationResponse {
            plan_title: "Empty".to_string(),
            reasoning: "Test".to_string(),
            steps: vec![],
        };

        let plan = build_executable_plan(response).await.expect("empty plan");
        assert_eq!(plan.len(), 0);
    }
}
