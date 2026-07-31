//! End-to-end tests for the planning flow.
//! Verifies the entire planning pipeline: generate K candidates → score → prune → save history.

#![cfg(test)]

use evohime_agent_runtime::{
    generate_candidate_plans, prune_to_top_n, score_candidate_plans, ExperienceHandle, LlmClient,
    PlanningConfig, PlanningError, ScoringWeights,
};
use evohime_protocol::planning::{PlanCandidate, ScoreBreakdown};
use evohime_storage::{planning_history::*, test_db::connect_integration_pool};
use uuid::Uuid;

/// Mock LLM client that generates K stub candidates
struct MockLlmClientForE2E {
    num_to_generate: usize,
}

impl LlmClient for MockLlmClientForE2E {
    async fn generate_plans(
        &self,
        task_desc: &str,
        num_candidates: usize,
    ) -> Result<Vec<PlanCandidate>, PlanningError> {
        let mut plans = Vec::new();
        for i in 0..num_candidates.min(self.num_to_generate) {
            plans.push(PlanCandidate {
                id: format!("plan-{}", i),
                description: format!("Approach {}: {}", i + 1, task_desc),
                confidence: 0.5,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.0,
                    tool_success_rate: 0.0,
                    complexity_penalty: 0.0,
                    feedback_adjustment: 0.0,
                    final_score: 0.0,
                },
            });
        }
        Ok(plans)
    }
}

/// Mock experience handle that returns a neutral similarity score
struct MockExperienceHandleForE2E {
    similarity_score: f32,
}

impl MockExperienceHandleForE2E {
    fn new(similarity_score: f32) -> Self {
        Self { similarity_score }
    }
}

impl ExperienceHandle for MockExperienceHandleForE2E {
    async fn search_similar(&self, _task_desc: &str) -> Result<f32, PlanningError> {
        Ok(self.similarity_score)
    }
}

/// Helper to create a lazy-connected test pool for unit tests
/// Note: The pool won't actually connect until used
fn create_test_pool_lazy() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://test:test@localhost/test")
        .expect("failed to create lazy pool")
}

/// Test: Generate K candidates, verify they are created with valid fields
#[tokio::test]
async fn test_e2e_generate_candidates() {
    let config = PlanningConfig {
        num_candidates: 3,
        top_n: 1,
        weights: ScoringWeights {
            similarity: 0.3,
            tool_success: 0.3,
            complexity: 0.2,
            feedback: 0.2,
        },
    };

    let llm = MockLlmClientForE2E {
        num_to_generate: config.num_candidates,
    };
    let experience = MockExperienceHandleForE2E::new(0.7);

    let generated = generate_candidate_plans(
        &llm,
        "test task description",
        config.num_candidates,
        &experience,
    )
    .await
    .expect("should generate plans");

    // Verify K candidates were generated
    assert_eq!(
        generated.len(),
        config.num_candidates,
        "should generate {} candidates",
        config.num_candidates
    );

    // Verify all candidates have valid structure
    for (idx, candidate) in generated.iter().enumerate() {
        assert!(!candidate.id.is_empty(), "candidate {} should have id", idx);
        assert!(
            !candidate.description.is_empty(),
            "candidate {} should have description",
            idx
        );
        // After generation, all candidates should have confidence = 0.5
        assert_eq!(
            candidate.confidence, 0.5,
            "candidate {} should have initial confidence 0.5",
            idx
        );
    }
}

/// Test: Score candidates, verify all confidence values are in [0.0–1.0]
#[tokio::test]
async fn test_e2e_score_candidates() {
    let candidates = vec![
        PlanCandidate {
            id: "plan-0".to_string(),
            description: "Approach 1: test task".to_string(),
            confidence: 0.5,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.0,
                tool_success_rate: 0.0,
                complexity_penalty: 0.0,
                feedback_adjustment: 0.0,
                final_score: 0.0,
            },
        },
        PlanCandidate {
            id: "plan-1".to_string(),
            description: "Approach 2: test task".to_string(),
            confidence: 0.5,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.0,
                tool_success_rate: 0.0,
                complexity_penalty: 0.0,
                feedback_adjustment: 0.0,
                final_score: 0.0,
            },
        },
    ];

    let weights = ScoringWeights {
        similarity: 0.3,
        tool_success: 0.3,
        complexity: 0.2,
        feedback: 0.2,
    };

    let experience = MockExperienceHandleForE2E::new(0.7);
    let pool = create_test_pool_lazy();

    let scored = score_candidate_plans(&candidates, "test task", &weights, &pool, &experience)
        .await
        .expect("should score plans");

    // Verify all candidates were scored
    assert_eq!(
        scored.len(),
        candidates.len(),
        "should score all candidates"
    );

    // Verify all confidence values are valid [0.0–1.0]
    for (idx, (candidate, _breakdown)) in scored.iter().enumerate() {
        assert!(
            candidate.confidence >= 0.0 && candidate.confidence <= 1.0,
            "candidate {} confidence {} should be in [0.0, 1.0]",
            idx,
            candidate.confidence
        );
    }
}

/// Test: Prune to top N, verify chosen_plan_id matches first candidate
#[tokio::test]
async fn test_e2e_prune_to_top_n() {
    let candidates = vec![
        (
            PlanCandidate {
                id: "plan-2".to_string(),
                description: "Plan 2".to_string(),
                confidence: 0.6,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.6,
                    tool_success_rate: 0.5,
                    complexity_penalty: 0.3,
                    feedback_adjustment: 0.0,
                    final_score: 0.6,
                },
            },
            ScoreBreakdown {
                similarity_score: 0.6,
                tool_success_rate: 0.5,
                complexity_penalty: 0.3,
                feedback_adjustment: 0.0,
                final_score: 0.6,
            },
        ),
        (
            PlanCandidate {
                id: "plan-0".to_string(),
                description: "Plan 0".to_string(),
                confidence: 0.8,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.8,
                    tool_success_rate: 0.8,
                    complexity_penalty: 0.1,
                    feedback_adjustment: 0.0,
                    final_score: 0.8,
                },
            },
            ScoreBreakdown {
                similarity_score: 0.8,
                tool_success_rate: 0.8,
                complexity_penalty: 0.1,
                feedback_adjustment: 0.0,
                final_score: 0.8,
            },
        ),
        (
            PlanCandidate {
                id: "plan-1".to_string(),
                description: "Plan 1".to_string(),
                confidence: 0.7,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.7,
                    tool_success_rate: 0.7,
                    complexity_penalty: 0.2,
                    feedback_adjustment: 0.0,
                    final_score: 0.7,
                },
            },
            ScoreBreakdown {
                similarity_score: 0.7,
                tool_success_rate: 0.7,
                complexity_penalty: 0.2,
                feedback_adjustment: 0.0,
                final_score: 0.7,
            },
        ),
    ];

    let top_n = 1;
    let pruned = prune_to_top_n(candidates, top_n);

    // Verify only top N were kept
    assert_eq!(
        pruned.len(),
        top_n,
        "should prune to top {} candidates",
        top_n
    );

    // Verify chosen plan is first in pruned list (highest score)
    let chosen_plan_id = &pruned[0].0.id;
    assert_eq!(
        chosen_plan_id, "plan-0",
        "chosen plan should be plan-0 (highest score 0.8)"
    );

    // Verify score is highest
    assert_eq!(
        pruned[0].0.confidence, 0.8,
        "chosen plan should have highest confidence"
    );
}

/// Test: Full E2E flow with database persistence
#[tokio::test]
async fn test_e2e_full_planning_flow_with_history() {
    let pool = match connect_integration_pool().await {
        Some(p) => p,
        None => {
            eprintln!("skipping full E2E test: integration database unavailable");
            return;
        }
    };

    // Setup
    let task_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let config = PlanningConfig {
        num_candidates: 3,
        top_n: 1,
        weights: ScoringWeights {
            similarity: 0.3,
            tool_success: 0.3,
            complexity: 0.2,
            feedback: 0.2,
        },
    };

    // Step 1: Generate K candidates
    let llm = MockLlmClientForE2E {
        num_to_generate: config.num_candidates,
    };
    let experience = MockExperienceHandleForE2E::new(0.7);
    let task_desc = "Deploy production application safely";

    let generated = generate_candidate_plans(&llm, task_desc, config.num_candidates, &experience)
        .await
        .expect("generate candidates");

    assert_eq!(generated.len(), config.num_candidates);

    // Step 2: Score candidates
    let scored = score_candidate_plans(&generated, task_desc, &config.weights, &pool, &experience)
        .await
        .expect("score candidates");

    assert_eq!(scored.len(), config.num_candidates);

    // Verify all have valid confidence
    for (candidate, _) in &scored {
        assert!(
            candidate.confidence >= 0.0 && candidate.confidence <= 1.0,
            "confidence should be in [0.0, 1.0], got {}",
            candidate.confidence
        );
    }

    // Step 3: Prune to top N
    let pruned = prune_to_top_n(scored, config.top_n);
    assert_eq!(pruned.len(), config.top_n);

    let chosen_plan_id = pruned[0].0.id.clone();
    let reasoning = format!("Selected plan {} based on scoring", chosen_plan_id);

    // Verify chosen_plan_id is first in pruned
    assert_eq!(pruned[0].0.id, chosen_plan_id);

    // Step 4: Save planning_history
    let entry = NewPlanningHistory {
        task_id,
        session_id,
        candidates: generated,
        chosen_plan_id: Some(chosen_plan_id.clone()),
        reasoning: reasoning.clone(),
    };

    let inserted = insert_planning_history(&pool, entry)
        .await
        .expect("insert planning history");

    // Assertions on inserted row
    assert_eq!(inserted.task_id, task_id);
    assert_eq!(inserted.session_id, session_id);
    assert_eq!(inserted.chosen_plan_id, Some(chosen_plan_id.clone()));
    assert_eq!(inserted.reasoning, reasoning);
    assert_eq!(
        inserted.candidates.as_array().map(Vec::len),
        Some(config.num_candidates)
    );

    // Verify reasoning ≤ 512 chars
    assert!(
        inserted.reasoning.len() <= 512,
        "reasoning should be ≤ 512 chars"
    );

    // Step 5: Retrieve and verify planning_history
    let retrieved_entries = list_planning_by_task(&pool, task_id)
        .await
        .expect("list planning history");

    assert!(
        !retrieved_entries.is_empty(),
        "should have at least one history entry"
    );

    let first_entry = &retrieved_entries[0];
    assert_eq!(first_entry.task_id, task_id);
    assert_eq!(first_entry.session_id, session_id);
    assert_eq!(first_entry.chosen_plan_id, Some(chosen_plan_id.clone()));
    assert_eq!(
        first_entry.candidates.as_array().map(Vec::len),
        Some(config.num_candidates)
    );
}

/// Test: Reasoning truncation when longer than 512 chars
#[tokio::test]
async fn test_e2e_reasoning_truncation() {
    let pool = match connect_integration_pool().await {
        Some(p) => p,
        None => {
            eprintln!("skipping reasoning truncation test: database unavailable");
            return;
        }
    };

    let task_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();

    // Create reasoning longer than 512 chars
    let long_reasoning = "a".repeat(600);

    let candidate = PlanCandidate {
        id: "plan-0".to_string(),
        description: "Test plan".to_string(),
        confidence: 0.8,
        score_breakdown: ScoreBreakdown {
            similarity_score: 0.8,
            tool_success_rate: 0.8,
            complexity_penalty: 0.1,
            feedback_adjustment: 0.0,
            final_score: 0.8,
        },
    };

    let entry = NewPlanningHistory {
        task_id,
        session_id,
        candidates: vec![candidate],
        chosen_plan_id: Some("plan-0".to_string()),
        reasoning: long_reasoning,
    };

    let inserted = insert_planning_history(&pool, entry)
        .await
        .expect("insert planning history");

    // Verify reasoning was truncated
    assert_eq!(
        inserted.reasoning.len(),
        512,
        "reasoning should be truncated to 512 chars"
    );
}

/// Test: All candidates have valid confidence in full flow
#[tokio::test]
async fn test_e2e_all_candidates_valid_confidence() {
    let candidates = vec![
        PlanCandidate {
            id: "plan-0".to_string(),
            description: "Plan 0".to_string(),
            confidence: 0.5,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.0,
                tool_success_rate: 0.0,
                complexity_penalty: 0.0,
                feedback_adjustment: 0.0,
                final_score: 0.0,
            },
        },
        PlanCandidate {
            id: "plan-1".to_string(),
            description: "Plan 1".to_string(),
            confidence: 0.5,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.0,
                tool_success_rate: 0.0,
                complexity_penalty: 0.0,
                feedback_adjustment: 0.0,
                final_score: 0.0,
            },
        },
        PlanCandidate {
            id: "plan-2".to_string(),
            description: "Plan 2".to_string(),
            confidence: 0.5,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.0,
                tool_success_rate: 0.0,
                complexity_penalty: 0.0,
                feedback_adjustment: 0.0,
                final_score: 0.0,
            },
        },
    ];

    let weights = ScoringWeights {
        similarity: 0.3,
        tool_success: 0.3,
        complexity: 0.2,
        feedback: 0.2,
    };

    let experience = MockExperienceHandleForE2E::new(0.5);
    let pool = create_test_pool_lazy();

    let scored = score_candidate_plans(&candidates, "test", &weights, &pool, &experience)
        .await
        .expect("score candidates");

    // Verify K candidates are present
    assert_eq!(scored.len(), 3, "should have 3 candidates");

    // Verify all confidence in [0.0–1.0]
    for (idx, (candidate, breakdown)) in scored.iter().enumerate() {
        assert!(
            candidate.confidence >= 0.0 && candidate.confidence <= 1.0,
            "candidate {} confidence {} out of range",
            idx,
            candidate.confidence
        );
        assert!(
            breakdown.is_valid(),
            "candidate {} breakdown should be valid",
            idx
        );
    }
}
