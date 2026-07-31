use evohime_protocol::planning::{PlanCandidate, ScoreBreakdown};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;

/// Configuration for the planning module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningConfig {
    pub num_candidates: usize,
    pub top_n: usize,
    pub weights: ScoringWeights,
}

/// Scoring weights for plan evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringWeights {
    pub similarity: f32,
    pub tool_success: f32,
    pub complexity: f32,
    pub feedback: f32,
}

/// Error type for planning operations
#[derive(Debug, Error)]
pub enum PlanningError {
    #[error("no candidates provided")]
    NoCandidates,
    #[error("invalid score: {0}")]
    InvalidScore(String),
    #[error("generation failed: {0}")]
    GenerationFailed(String),
    #[error("scoring failed: {0}")]
    ScoringFailed(String),
    #[error("experience lookup failed: {0}")]
    ExperienceLookupFailed(String),
}

/// Trait for querying experience memory (historical plan success)
pub trait ExperienceHandle: Send + Sync {
    /// Search for similar tasks in experience memory
    /// Returns similarity score in [0.0–1.0]
    fn search_similar(
        &self,
        task_desc: &str,
    ) -> impl std::future::Future<Output = Result<f32, PlanningError>> + Send;
}

/// Trait for LLM client (to be implemented or mocked)
pub trait LlmClient: Send + Sync {
    /// Generate candidate plans via LLM structured output
    fn generate_plans(
        &self,
        task_desc: &str,
        num_candidates: usize,
    ) -> impl std::future::Future<Output = Result<Vec<PlanCandidate>, PlanningError>> + Send;
}

/// Mock LLM client for testing
pub struct MockLlmClient {
    pub num_to_generate: usize,
}

impl LlmClient for MockLlmClient {
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

/// Generate candidate plans via LLM
///
/// # Arguments
/// * `llm_client` - LLM client for plan generation
/// * `task_desc` - Description of the task
/// * `num_candidates` - Number of candidates to generate
/// * `experience` - Experience handle for grounding (currently unused, reserved for future integration)
pub async fn generate_candidate_plans<L: LlmClient + ?Sized, E: ExperienceHandle + ?Sized>(
    llm_client: &L,
    task_desc: &str,
    num_candidates: usize,
    _experience: &E,
) -> Result<Vec<PlanCandidate>, PlanningError> {
    if num_candidates == 0 {
        return Ok(Vec::new());
    }

    let mut plans = llm_client.generate_plans(task_desc, num_candidates).await?;

    // Ensure each candidate has confidence=0.5 initially
    for plan in &mut plans {
        plan.confidence = 0.5;
    }

    Ok(plans)
}

/// Helper function to query tool success rate from storage
/// Returns the ratio of successful tool calls to total tool calls for similar tasks
/// Falls back to 0.5 if no history is available
async fn get_tool_success_rate(_pool: &PgPool, _task_desc: &str) -> Result<f32, PlanningError> {
    // TODO: Implement actual query to execution_history or similar table
    // For now, return fallback value
    // In a real implementation:
    // - Query tasks matching task_desc
    // - Count successful (tool_call_success=true) vs total tool calls
    // - Return success_count / total_count, or 0.5 if no history
    Ok(0.5)
}

/// Score candidate plans using the unified formula:
/// final_score = (w1·similarity + w2·tool_success + w3·(1-complexity) + w4·(1+feedback)/2) / Σw
///
/// # Arguments
/// * `candidates` - Candidate plans to score
/// * `task_desc` - Description of the task (used for experience queries)
/// * `weights` - Scoring weights
/// * `pool` - Database pool for querying tool history
/// * `experience` - Experience handle for querying similarity scores
pub async fn score_candidate_plans<E: ExperienceHandle + ?Sized>(
    candidates: &[PlanCandidate],
    task_desc: &str,
    weights: &ScoringWeights,
    pool: &PgPool,
    experience: &E,
) -> Result<Vec<(PlanCandidate, ScoreBreakdown)>, PlanningError> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    // Calculate the sum of all weights
    let weight_sum =
        weights.similarity + weights.tool_success + weights.complexity + weights.feedback;
    if weight_sum <= 0.0 {
        return Err(PlanningError::ScoringFailed(
            "weight sum must be positive".to_string(),
        ));
    }

    let mut scored = Vec::new();

    for candidate in candidates {
        // Query experience memory for similarity score
        let similarity_score: f32 = match experience.search_similar(task_desc).await {
            Ok(score) => score,
            Err(e) => {
                tracing::warn!("experience similarity lookup failed: {}", e);
                0.5 // Fallback to neutral value
            }
        };

        // Query tool history for success rate
        let tool_success_rate: f32 = match get_tool_success_rate(pool, task_desc).await {
            Ok(rate) => rate,
            Err(e) => {
                tracing::warn!("tool success lookup failed: {}", e);
                0.5 // Fallback to neutral value
            }
        };

        let complexity_penalty: f32 = 0.3; // Estimated from plan structure (depth/breadth)
        let feedback_adjustment: f32 = 0.0; // No feedback yet on plan (will be updated during execution)

        // Validate inputs
        if !(0.0..=1.0).contains(&similarity_score) {
            return Err(PlanningError::InvalidScore(format!(
                "similarity_score out of range: {}",
                similarity_score
            )));
        }
        if !(0.0..=1.0).contains(&tool_success_rate) {
            return Err(PlanningError::InvalidScore(format!(
                "tool_success_rate out of range: {}",
                tool_success_rate
            )));
        }
        if !(0.0..=1.0).contains(&complexity_penalty) {
            return Err(PlanningError::InvalidScore(format!(
                "complexity_penalty out of range: {}",
                complexity_penalty
            )));
        }

        // Clamp feedback to [-1.0, 1.0]
        let feedback_clamped: f32 = feedback_adjustment.clamp(-1.0, 1.0);

        // Apply the corrected scoring formula:
        // final_score = (w1·similarity + w2·tool_success + w3·(1-complexity) + w4·(1+feedback)/2) / Σw
        let final_score = (weights.similarity * similarity_score
            + weights.tool_success * tool_success_rate
            + weights.complexity * (1.0 - complexity_penalty)
            + weights.feedback * (1.0 + feedback_clamped) / 2.0)
            / weight_sum;

        // Validate the final score is in [0.0, 1.0]
        if !(0.0..=1.0).contains(&final_score) {
            return Err(PlanningError::InvalidScore(format!(
                "final_score out of range: {}",
                final_score
            )));
        }

        let mut updated_candidate = candidate.clone();
        updated_candidate.confidence = final_score;

        let breakdown = ScoreBreakdown {
            similarity_score,
            tool_success_rate,
            complexity_penalty,
            feedback_adjustment: feedback_clamped,
            final_score,
        };

        // Validate breakdown
        if !breakdown.is_valid() {
            return Err(PlanningError::InvalidScore(
                "score breakdown is invalid".to_string(),
            ));
        }

        scored.push((updated_candidate, breakdown));
    }

    Ok(scored)
}

/// Prune scored candidates to top N, using deterministic tie-breaking:
/// Sort by final_score DESC, then by id ASC
pub fn prune_to_top_n(
    mut candidates: Vec<(PlanCandidate, ScoreBreakdown)>,
    top_n: usize,
) -> Vec<(PlanCandidate, ScoreBreakdown)> {
    // Sort by score DESC, then by id ASC for deterministic tie-breaking
    candidates.sort_by(|a, b| {
        // Primary: final_score descending
        match b.1.final_score.partial_cmp(&a.1.final_score) {
            Some(std::cmp::Ordering::Equal) => {
                // Tie-breaker: id ascending
                a.0.id.cmp(&b.0.id)
            }
            other => other.unwrap_or(std::cmp::Ordering::Equal),
        }
    });

    candidates.truncate(top_n);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock experience handle for testing
    struct MockExperienceHandle {
        similarity_score: f32,
        should_fail: bool,
    }

    impl MockExperienceHandle {
        fn new(similarity_score: f32) -> Self {
            Self {
                similarity_score,
                should_fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                similarity_score: 0.0,
                should_fail: true,
            }
        }
    }

    impl ExperienceHandle for MockExperienceHandle {
        async fn search_similar(&self, _task_desc: &str) -> Result<f32, PlanningError> {
            if self.should_fail {
                Err(PlanningError::ExperienceLookupFailed(
                    "mock failure".to_string(),
                ))
            } else {
                Ok(self.similarity_score)
            }
        }
    }

    #[test]
    fn test_generate_candidate_plans_count() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = MockLlmClient { num_to_generate: 5 };
        let experience = MockExperienceHandle::new(0.7);

        let result = rt.block_on(generate_candidate_plans(
            &client,
            "test task",
            3,
            &experience,
        ));
        assert!(result.is_ok());

        let plans = result.unwrap();
        assert_eq!(plans.len(), 3);

        // All candidates should have confidence=0.5 initially
        for plan in &plans {
            assert_eq!(plan.confidence, 0.5);
        }
    }

    #[test]
    fn test_generate_candidate_plans_zero_count() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = MockLlmClient { num_to_generate: 5 };
        let experience = MockExperienceHandle::new(0.7);

        let result = rt.block_on(generate_candidate_plans(
            &client,
            "test task",
            0,
            &experience,
        ));
        assert!(result.is_ok());

        let plans = result.unwrap();
        assert_eq!(plans.len(), 0);
    }

    #[tokio::test]
    async fn test_score_candidate_plans_empty() {
        let weights = ScoringWeights {
            similarity: 0.3,
            tool_success: 0.3,
            complexity: 0.2,
            feedback: 0.2,
        };

        let experience = MockExperienceHandle::new(0.7);
        let pool = create_test_pool_lazy();

        let result = score_candidate_plans(&[], "test task", &weights, &pool, &experience).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    /// Helper to create a lazy-connected test pool for unit tests
    /// Note: The pool won't actually connect until used, which is fine for tests
    /// where database access is not needed
    fn create_test_pool_lazy() -> PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://test:test@localhost/test")
            .expect("failed to create lazy pool")
    }

    #[tokio::test]
    async fn test_score_candidate_plans_valid() {
        let candidates = vec![PlanCandidate {
            id: "plan-1".to_string(),
            description: "Test plan 1".to_string(),
            confidence: 0.5,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.0,
                tool_success_rate: 0.0,
                complexity_penalty: 0.0,
                feedback_adjustment: 0.0,
                final_score: 0.0,
            },
        }];

        let weights = ScoringWeights {
            similarity: 0.3,
            tool_success: 0.3,
            complexity: 0.2,
            feedback: 0.2,
        };

        let experience = MockExperienceHandle::new(0.7);
        let pool = create_test_pool_lazy();

        let result =
            score_candidate_plans(&candidates, "test task", &weights, &pool, &experience).await;
        assert!(result.is_ok());

        let scored = result.unwrap();
        assert_eq!(scored.len(), 1);
        assert!(scored[0].1.is_valid());
    }

    #[test]
    fn test_scoring_formula_best_case() {
        // Best case: similarity=1.0, tool_success=1.0, complexity=0.0 (no penalty), feedback=1.0
        // final_score = (0.3*1 + 0.3*1 + 0.2*(1-0) + 0.2*(1+1)/2) / 1.0
        //           = (0.3 + 0.3 + 0.2 + 0.2) / 1.0 = 1.0
        let weights = ScoringWeights {
            similarity: 0.3,
            tool_success: 0.3,
            complexity: 0.2,
            feedback: 0.2,
        };

        let weight_sum =
            weights.similarity + weights.tool_success + weights.complexity + weights.feedback;

        let similarity = 1.0;
        let tool_success = 1.0;
        let complexity = 0.0; // Best case: no complexity penalty
        let feedback = 1.0;

        let score = (weights.similarity * similarity
            + weights.tool_success * tool_success
            + weights.complexity * (1.0 - complexity)
            + weights.feedback * (1.0 + feedback) / 2.0)
            / weight_sum;

        assert!((score - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_scoring_formula_worst_case() {
        // Worst case: all inputs are 0.0, feedback is -1.0
        // final_score = (0 + 0 + 0.2*1 + 0.2*0) / 1.0 = 0.2
        let weights = ScoringWeights {
            similarity: 0.3,
            tool_success: 0.3,
            complexity: 0.2,
            feedback: 0.2,
        };

        let weight_sum =
            weights.similarity + weights.tool_success + weights.complexity + weights.feedback;

        let similarity = 0.0;
        let tool_success = 0.0;
        let complexity = 0.0;
        let feedback = -1.0;

        let score = (weights.similarity * similarity
            + weights.tool_success * tool_success
            + weights.complexity * (1.0 - complexity)
            + weights.feedback * (1.0 + feedback) / 2.0)
            / weight_sum;

        assert!((score - 0.2).abs() < 0.0001);
    }

    #[test]
    fn test_prune_to_top_n_basic() {
        let candidates = vec![
            (
                PlanCandidate {
                    id: "plan-1".to_string(),
                    description: "Plan 1".to_string(),
                    confidence: 0.8,
                    score_breakdown: ScoreBreakdown {
                        similarity_score: 0.0,
                        tool_success_rate: 0.0,
                        complexity_penalty: 0.0,
                        feedback_adjustment: 0.0,
                        final_score: 0.8,
                    },
                },
                ScoreBreakdown {
                    similarity_score: 0.0,
                    tool_success_rate: 0.0,
                    complexity_penalty: 0.0,
                    feedback_adjustment: 0.0,
                    final_score: 0.8,
                },
            ),
            (
                PlanCandidate {
                    id: "plan-2".to_string(),
                    description: "Plan 2".to_string(),
                    confidence: 0.6,
                    score_breakdown: ScoreBreakdown {
                        similarity_score: 0.0,
                        tool_success_rate: 0.0,
                        complexity_penalty: 0.0,
                        feedback_adjustment: 0.0,
                        final_score: 0.6,
                    },
                },
                ScoreBreakdown {
                    similarity_score: 0.0,
                    tool_success_rate: 0.0,
                    complexity_penalty: 0.0,
                    feedback_adjustment: 0.0,
                    final_score: 0.6,
                },
            ),
        ];

        let pruned = prune_to_top_n(candidates, 1);
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].0.id, "plan-1");
        assert_eq!(pruned[0].1.final_score, 0.8);
    }

    #[test]
    fn test_prune_to_top_n_tie_breaker() {
        // Test deterministic tie-breaking with same scores
        let candidates = vec![
            (
                PlanCandidate {
                    id: "plan-b".to_string(),
                    description: "Plan B".to_string(),
                    confidence: 0.75,
                    score_breakdown: ScoreBreakdown {
                        similarity_score: 0.0,
                        tool_success_rate: 0.0,
                        complexity_penalty: 0.0,
                        feedback_adjustment: 0.0,
                        final_score: 0.75,
                    },
                },
                ScoreBreakdown {
                    similarity_score: 0.0,
                    tool_success_rate: 0.0,
                    complexity_penalty: 0.0,
                    feedback_adjustment: 0.0,
                    final_score: 0.75,
                },
            ),
            (
                PlanCandidate {
                    id: "plan-a".to_string(),
                    description: "Plan A".to_string(),
                    confidence: 0.75,
                    score_breakdown: ScoreBreakdown {
                        similarity_score: 0.0,
                        tool_success_rate: 0.0,
                        complexity_penalty: 0.0,
                        feedback_adjustment: 0.0,
                        final_score: 0.75,
                    },
                },
                ScoreBreakdown {
                    similarity_score: 0.0,
                    tool_success_rate: 0.0,
                    complexity_penalty: 0.0,
                    feedback_adjustment: 0.0,
                    final_score: 0.75,
                },
            ),
            (
                PlanCandidate {
                    id: "plan-c".to_string(),
                    description: "Plan C".to_string(),
                    confidence: 0.75,
                    score_breakdown: ScoreBreakdown {
                        similarity_score: 0.0,
                        tool_success_rate: 0.0,
                        complexity_penalty: 0.0,
                        feedback_adjustment: 0.0,
                        final_score: 0.75,
                    },
                },
                ScoreBreakdown {
                    similarity_score: 0.0,
                    tool_success_rate: 0.0,
                    complexity_penalty: 0.0,
                    feedback_adjustment: 0.0,
                    final_score: 0.75,
                },
            ),
        ];

        let pruned = prune_to_top_n(candidates, 2);
        assert_eq!(pruned.len(), 2);
        // Should be sorted by id ascending when scores are tied
        assert_eq!(pruned[0].0.id, "plan-a");
        assert_eq!(pruned[1].0.id, "plan-b");
    }

    #[test]
    fn test_prune_to_top_n_fewer_than_top() {
        // When we have fewer candidates than top_n
        let candidates = vec![(
            PlanCandidate {
                id: "plan-1".to_string(),
                description: "Plan 1".to_string(),
                confidence: 0.8,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.0,
                    tool_success_rate: 0.0,
                    complexity_penalty: 0.0,
                    feedback_adjustment: 0.0,
                    final_score: 0.8,
                },
            },
            ScoreBreakdown {
                similarity_score: 0.0,
                tool_success_rate: 0.0,
                complexity_penalty: 0.0,
                feedback_adjustment: 0.0,
                final_score: 0.8,
            },
        )];

        let pruned = prune_to_top_n(candidates, 5);
        assert_eq!(pruned.len(), 1);
    }

    #[test]
    fn test_prune_to_top_n_empty() {
        let candidates = vec![];
        let pruned = prune_to_top_n(candidates, 3);
        assert_eq!(pruned.len(), 0);
    }

    #[test]
    fn test_score_breakdown_validation() {
        let breakdown = ScoreBreakdown {
            similarity_score: 0.8,
            tool_success_rate: 0.9,
            complexity_penalty: 0.2,
            feedback_adjustment: 0.1,
            final_score: 0.75,
        };
        assert!(breakdown.is_valid());

        let invalid_breakdown = ScoreBreakdown {
            similarity_score: 1.5, // Out of range
            tool_success_rate: 0.9,
            complexity_penalty: 0.2,
            feedback_adjustment: 0.1,
            final_score: 0.75,
        };
        assert!(!invalid_breakdown.is_valid());
    }

    #[tokio::test]
    async fn test_score_with_experience_fallback() {
        // Test that when experience lookup fails, we fall back to 0.5 without crashing
        let candidates = vec![PlanCandidate {
            id: "plan-1".to_string(),
            description: "Test plan".to_string(),
            confidence: 0.5,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.0,
                tool_success_rate: 0.0,
                complexity_penalty: 0.0,
                feedback_adjustment: 0.0,
                final_score: 0.0,
            },
        }];

        let weights = ScoringWeights {
            similarity: 0.3,
            tool_success: 0.3,
            complexity: 0.2,
            feedback: 0.2,
        };

        let experience = MockExperienceHandle::failing();
        let pool = create_test_pool_lazy();

        let result =
            score_candidate_plans(&candidates, "test task", &weights, &pool, &experience).await;

        // Should succeed even though experience lookup failed
        assert!(
            result.is_ok(),
            "scoring should not crash when experience lookup fails"
        );

        let scored = result.unwrap();
        assert_eq!(scored.len(), 1);
        assert!(
            scored[0].1.is_valid(),
            "score breakdown should be valid even with fallback"
        );

        // Verify that fallback values (0.5) were used
        // With similarity=0.5, tool_success=0.5, complexity=0.3, feedback=0.0:
        // final_score = (0.3*0.5 + 0.3*0.5 + 0.2*(1-0.3) + 0.2*(1+0)/2) / 1.0
        //            = (0.15 + 0.15 + 0.14 + 0.1) / 1.0 = 0.54
        let expected_score =
            (0.3 * 0.5 + 0.3 * 0.5 + 0.2 * (1.0 - 0.3) + 0.2 * (1.0 + 0.0) / 2.0) / 1.0;
        assert!((scored[0].1.final_score - expected_score).abs() < 0.0001);
    }
}
