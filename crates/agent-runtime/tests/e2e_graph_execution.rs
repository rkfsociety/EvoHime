use evohime_agent_runtime::plan_generation::build_executable_plan;
use evohime_agent_runtime::planning_graph::validate_and_sort_graph;
use evohime_agent_runtime::compute_execution_batches;
use evohime_protocol::{PlanGenerationResponse, PlanStep};
use std::collections::HashMap;

#[tokio::test]
async fn e2e_linear_plan_materialization() {
    // Simulate legacy linear plan from Stage 8.1
    let response = PlanGenerationResponse {
        plan_title: "Linear refactor".to_string(),
        reasoning: "Sequential execution".to_string(),
        steps: vec![
            PlanStep {
                id: "step-1".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read auth.rs".to_string(),
                depends_on: vec![],
            },
            PlanStep {
                id: "step-2".to_string(),
                tool_name: "filesystem.patch".to_string(),
                description: "Extract class".to_string(),
                depends_on: vec![],
            },
            PlanStep {
                id: "step-3".to_string(),
                tool_name: "shell.execute".to_string(),
                description: "Run tests".to_string(),
                depends_on: vec![],
            },
        ],
    };

    // Build plan (materializes sequential deps)
    let plan = build_executable_plan(response).await.expect("build plan");

    // Verify materialization: empty depends_on became sequential
    assert_eq!(plan[0].depends_on, Vec::<String>::new());
    assert_eq!(plan[1].depends_on, vec!["step-1".to_string()]);
    assert_eq!(plan[2].depends_on, vec!["step-2".to_string()]);

    // Validate graph
    let mut dep_map = HashMap::new();
    for step in &plan {
        dep_map.insert(step.id.clone(), step.depends_on.clone());
    }

    let graph = validate_and_sort_graph(&dep_map).expect("validate");
    assert_eq!(graph.topological_order, vec!["step-1", "step-2", "step-3"]);

    // Compute batches
    let batches = compute_execution_batches(&plan).expect("compute batches");
    assert_eq!(batches.batches.len(), 3); // Linear: 3 batches
    assert_eq!(batches.batches[0], vec!["step-1"]);
    assert_eq!(batches.batches[1], vec!["step-2"]);
    assert_eq!(batches.batches[2], vec!["step-3"]);
}

#[tokio::test]
async fn e2e_diamond_graph_execution() {
    // Diamond pattern: A → [B, C] → D
    let response = PlanGenerationResponse {
        plan_title: "Diamond refactor".to_string(),
        reasoning: "Parallel branches".to_string(),
        steps: vec![
            PlanStep {
                id: "a".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read file".to_string(),
                depends_on: vec![],
            },
            PlanStep {
                id: "b".to_string(),
                tool_name: "filesystem.write".to_string(),
                description: "Extract class".to_string(),
                depends_on: vec!["a".to_string()],
            },
            PlanStep {
                id: "c".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read tests".to_string(),
                depends_on: vec!["a".to_string()],
            },
            PlanStep {
                id: "d".to_string(),
                tool_name: "shell.execute".to_string(),
                description: "Run all tests".to_string(),
                depends_on: vec!["b".to_string(), "c".to_string()],
            },
        ],
    };

    // Build plan
    let plan = build_executable_plan(response).await.expect("build plan");

    // Compute batches
    let batches = compute_execution_batches(&plan).expect("compute batches");

    // Verify diamond structure:
    // Batch 0: [a]
    // Batch 1: [b, c] (independent)
    // Batch 2: [d] (depends on both)
    assert_eq!(batches.batches.len(), 3);
    assert_eq!(batches.batches[0], vec!["a"]);
    assert_eq!(batches.batches[1].len(), 2);
    assert!(batches.batches[1].contains(&"b".to_string()));
    assert!(batches.batches[1].contains(&"c".to_string()));
    assert_eq!(batches.batches[2], vec!["d"]);
}

#[tokio::test]
async fn e2e_cyclic_fallback_to_linear() {
    // Simulate LLM generating cyclic plan
    let response = PlanGenerationResponse {
        plan_title: "Cyclic plan".to_string(),
        reasoning: "Has cycle".to_string(),
        steps: vec![
            PlanStep {
                id: "x".to_string(),
                tool_name: "sh".to_string(),
                description: "X".to_string(),
                depends_on: vec!["y".to_string()], // Cycle: x→y→x
            },
            PlanStep {
                id: "y".to_string(),
                tool_name: "sh".to_string(),
                description: "Y".to_string(),
                depends_on: vec!["x".to_string()],
            },
        ],
    };

    // Build plan (should fallback to linear due to cycle)
    let plan = build_executable_plan(response).await.expect("fallback to linear");

    // After fallback, all steps have materialized sequential deps
    assert_eq!(plan[0].depends_on, Vec::<String>::new()); // First step no deps
    assert_eq!(plan[1].depends_on, vec!["x".to_string()]); // Second depends on first

    // Compute batches
    let batches = compute_execution_batches(&plan).expect("compute batches");
    assert_eq!(batches.batches.len(), 2);
    assert_eq!(batches.batches[0], vec!["x"]);
    assert_eq!(batches.batches[1], vec!["y"]);
}

#[tokio::test]
async fn e2e_plan_size_limit() {
    // Create plan exceeding max size (50 steps)
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

    // Should reject plan
    let result = build_executable_plan(response).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("exceeds maximum 50 steps"));
}

#[tokio::test]
async fn e2e_explicit_dependencies_preserved() {
    // Plan with explicit dependencies (not materialized from empty)
    let response = PlanGenerationResponse {
        plan_title: "Graph plan".to_string(),
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
        ],
    };

    let plan = build_executable_plan(response).await.expect("build plan");

    // Verify explicit deps preserved (not overwritten by materialization)
    assert_eq!(plan.iter().find(|s| s.id == "b").unwrap().depends_on, vec!["a"]);
    assert_eq!(plan.iter().find(|s| s.id == "c").unwrap().depends_on, vec!["a"]);
}

#[tokio::test]
async fn e2e_empty_plan() {
    let response = PlanGenerationResponse {
        plan_title: "Empty".to_string(),
        reasoning: "Test".to_string(),
        steps: vec![],
    };

    let plan = build_executable_plan(response).await.expect("empty plan");
    assert_eq!(plan.len(), 0);

    let batches = compute_execution_batches(&plan).expect("compute batches");
    assert_eq!(batches.batches.len(), 0);
}

#[tokio::test]
async fn e2e_complex_pipeline() {
    // Complex scenario: multiple independent and dependent branches
    let response = PlanGenerationResponse {
        plan_title: "Complex refactor".to_string(),
        reasoning: "Multiple branches".to_string(),
        steps: vec![
            // Setup phase
            PlanStep {
                id: "setup-1".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read config".to_string(),
                depends_on: vec![],
            },
            PlanStep {
                id: "setup-2".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read schema".to_string(),
                depends_on: vec![],
            },
            // Parallel branches
            PlanStep {
                id: "branch-a1".to_string(),
                tool_name: "filesystem.patch".to_string(),
                description: "Patch A".to_string(),
                depends_on: vec!["setup-1".to_string()],
            },
            PlanStep {
                id: "branch-b1".to_string(),
                tool_name: "filesystem.patch".to_string(),
                description: "Patch B".to_string(),
                depends_on: vec!["setup-2".to_string()],
            },
            // Merge phase
            PlanStep {
                id: "merge".to_string(),
                tool_name: "shell.execute".to_string(),
                description: "Run tests".to_string(),
                depends_on: vec!["branch-a1".to_string(), "branch-b1".to_string()],
            },
        ],
    };

    let plan = build_executable_plan(response).await.expect("build plan");
    let batches = compute_execution_batches(&plan).expect("compute batches");

    // Verify structure
    assert_eq!(batches.batches.len(), 3);

    // Batch 0: Setup (independent)
    assert_eq!(batches.batches[0].len(), 2);

    // Batch 1: Parallel branches
    assert_eq!(batches.batches[1].len(), 2);

    // Batch 2: Merge
    assert_eq!(batches.batches[2], vec!["merge"]);
}
