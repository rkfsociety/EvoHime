use evohime_protocol::PlanStep;
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use crate::planning_graph::validate_and_sort_graph;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("invalid graph: {0}")]
    InvalidGraph(String),
    #[error("step failed: {0}")]
    StepFailed(String),
}

pub struct ExecutionBatches {
    pub batches: Vec<Vec<String>>, // Each batch is a list of step IDs that can run in order
    pub failed_steps: HashSet<String>, // Steps that failed (don't execute descendants)
}

impl ExecutionBatches {
    pub fn is_step_ready(&self, step_id: &str, steps: &[PlanStep], completed: &HashSet<String>) -> bool {
        let step = steps.iter().find(|s| s.id == step_id).expect("step exists");

        // Ready if: all dependencies completed AND no dependency failed
        step.depends_on.iter().all(|dep| {
            completed.contains(dep) && !self.failed_steps.contains(dep)
        })
    }
}

/// Compute execution batches from plan steps
///
/// Returns batches where:
/// - Each batch contains steps that can run deterministically (in step_id order)
/// - Batch N+1 starts only after Batch N completes
/// - NOTE: Stage 8.3 runs batches sequentially, not in parallel
pub fn compute_execution_batches(steps: &[PlanStep]) -> Result<ExecutionBatches, ExecutorError> {
    if steps.is_empty() {
        return Ok(ExecutionBatches {
            batches: vec![],
            failed_steps: HashSet::new(),
        });
    }

    // Build dependency map
    let mut dep_map: HashMap<String, Vec<String>> = HashMap::new();
    for step in steps {
        dep_map.insert(step.id.clone(), step.depends_on.clone());
    }

    // Validate (cycle detection, missing deps)
    let graph = validate_and_sort_graph(&dep_map)
        .map_err(|e| ExecutorError::InvalidGraph(format!("{:?}", e)))?;

    // Partition into batches by depth
    // Batch index = max batch index of dependencies + 1
    let mut batches: Vec<Vec<String>> = vec![];
    let mut step_to_batch: HashMap<String, usize> = HashMap::new();

    for step_id in &graph.topological_order {
        let deps = &dep_map[step_id];

        // Batch index = max batch index of dependencies + 1
        let batch_idx = deps.iter()
            .filter_map(|d| step_to_batch.get(d))
            .max()
            .map(|i| i + 1)
            .unwrap_or(0);

        // Ensure batch exists
        while batches.len() <= batch_idx {
            batches.push(vec![]);
        }

        batches[batch_idx].push(step_id.clone());
        step_to_batch.insert(step_id.clone(), batch_idx);
    }

    Ok(ExecutionBatches {
        batches,
        failed_steps: HashSet::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_step_execution() {
        let steps = vec![PlanStep {
            id: "step-1".to_string(),
            tool_name: "shell.execute".to_string(),
            description: "echo hello".to_string(),
            depends_on: vec![],
        }];

        let batches = compute_execution_batches(&steps).expect("compute");
        assert_eq!(batches.batches.len(), 1);
        assert_eq!(batches.batches[0], vec!["step-1"]);
    }

    #[test]
    fn test_sequential_dependent_steps() {
        let steps = vec![
            PlanStep {
                id: "step-1".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read file".to_string(),
                depends_on: vec![],
            },
            PlanStep {
                id: "step-2".to_string(),
                tool_name: "filesystem.patch".to_string(),
                description: "Patch file".to_string(),
                depends_on: vec!["step-1".to_string()],
            },
        ];

        let batches = compute_execution_batches(&steps).expect("compute");
        assert_eq!(batches.batches.len(), 2);
        assert_eq!(batches.batches[0], vec!["step-1"]);
        assert_eq!(batches.batches[1], vec!["step-2"]);
    }

    #[test]
    fn test_parallel_independent_steps() {
        let steps = vec![
            PlanStep {
                id: "step-a".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read a.rs".to_string(),
                depends_on: vec![],
            },
            PlanStep {
                id: "step-b".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read b.rs".to_string(),
                depends_on: vec![],
            },
        ];

        let batches = compute_execution_batches(&steps).expect("compute");
        assert_eq!(batches.batches.len(), 1);
        assert_eq!(batches.batches[0].len(), 2);
        assert!(batches.batches[0].contains(&"step-a".to_string()));
        assert!(batches.batches[0].contains(&"step-b".to_string()));
    }

    #[test]
    fn test_diamond_dependency_pattern() {
        // A → [B, C] → D
        let steps = vec![
            PlanStep {
                id: "a".to_string(),
                tool_name: "shell.execute".to_string(),
                description: "Root".to_string(),
                depends_on: vec![],
            },
            PlanStep {
                id: "b".to_string(),
                tool_name: "shell.execute".to_string(),
                description: "Branch B".to_string(),
                depends_on: vec!["a".to_string()],
            },
            PlanStep {
                id: "c".to_string(),
                tool_name: "shell.execute".to_string(),
                description: "Branch C".to_string(),
                depends_on: vec!["a".to_string()],
            },
            PlanStep {
                id: "d".to_string(),
                tool_name: "shell.execute".to_string(),
                description: "Merge D".to_string(),
                depends_on: vec!["b".to_string(), "c".to_string()],
            },
        ];

        let batches = compute_execution_batches(&steps).expect("compute");
        assert_eq!(batches.batches.len(), 3);
        assert_eq!(batches.batches[0], vec!["a"]);
        assert_eq!(batches.batches[1].len(), 2); // b and c
        assert!(batches.batches[1].contains(&"b".to_string()));
        assert!(batches.batches[1].contains(&"c".to_string()));
        assert_eq!(batches.batches[2], vec!["d"]);
    }

    #[test]
    fn test_empty_plan() {
        let steps = vec![];
        let batches = compute_execution_batches(&steps).expect("compute");
        assert_eq!(batches.batches.len(), 0);
    }

    #[test]
    fn test_is_step_ready() {
        let steps = vec![
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
        ];

        let mut execution = ExecutionBatches {
            batches: vec![],
            failed_steps: HashSet::new(),
        };

        let mut completed = HashSet::new();

        // Before a completes, b is not ready
        assert!(!execution.is_step_ready("b", &steps, &completed));

        // After a completes, b is ready
        completed.insert("a".to_string());
        assert!(execution.is_step_ready("b", &steps, &completed));

        // If a fails, b should not be ready
        completed.remove("a");
        execution.failed_steps.insert("a".to_string());
        assert!(!execution.is_step_ready("b", &steps, &completed));
    }
}
