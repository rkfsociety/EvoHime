use serde::{Deserialize, Serialize};

/// LLM response for plan generation with dependency structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanGenerationResponse {
    pub plan_title: String,
    pub reasoning: String,
    // Uses crate::PlanStep which has depends_on field
    pub steps: Vec<crate::PlanStep>,
}

/// Represents a single candidate plan with its evaluation metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanCandidate {
    pub id: String,
    pub description: String,
    pub confidence: f32,
    pub score_breakdown: ScoreBreakdown,
}

/// Detailed breakdown of how a plan candidate was scored
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreBreakdown {
    pub similarity_score: f32,
    pub tool_success_rate: f32,
    pub complexity_penalty: f32,
    pub feedback_adjustment: f32,
    pub final_score: f32,
}

impl ScoreBreakdown {
    /// Validates score components:
    /// - similarity_score, tool_success_rate, complexity_penalty, final_score: [0.0–1.0]
    /// - feedback_adjustment: [-1.0, +1.0] (internal representation for scoring formula)
    ///
    /// Conversion note: feedback_adjustment is stored in [-1.0, +1.0] for formula computation.
    /// When serializing to frontend, normalize via: (1 + feedback_adjustment) / 2 → [0.0, 1.0]
    pub fn is_valid(&self) -> bool {
        self.similarity_score >= 0.0
            && self.similarity_score <= 1.0
            && self.tool_success_rate >= 0.0
            && self.tool_success_rate <= 1.0
            && self.complexity_penalty >= 0.0
            && self.complexity_penalty <= 1.0
            && self.feedback_adjustment >= -1.0
            && self.feedback_adjustment <= 1.0
            && self.final_score >= 0.0
            && self.final_score <= 1.0
    }
}

impl PlanCandidate {
    /// Validates that confidence is in the valid range [0.0–1.0]
    /// and that the embedded score breakdown is also valid
    pub fn is_valid(&self) -> bool {
        self.confidence >= 0.0 && self.confidence <= 1.0 && self.score_breakdown.is_valid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_breakdown_is_valid_when_all_scores_in_range() {
        let breakdown = ScoreBreakdown {
            similarity_score: 0.85,
            tool_success_rate: 0.9,
            complexity_penalty: 0.1,
            feedback_adjustment: 0.05,
            final_score: 0.75,
        };
        assert!(breakdown.is_valid());
    }

    #[test]
    fn score_breakdown_is_valid_at_boundaries() {
        let breakdown = ScoreBreakdown {
            similarity_score: 0.0,
            tool_success_rate: 1.0,
            complexity_penalty: 0.5,
            feedback_adjustment: 0.25,
            final_score: 0.75,
        };
        assert!(breakdown.is_valid());
    }

    #[test]
    fn score_breakdown_is_invalid_when_similarity_score_exceeds_one() {
        let breakdown = ScoreBreakdown {
            similarity_score: 1.1,
            tool_success_rate: 0.9,
            complexity_penalty: 0.1,
            feedback_adjustment: 0.05,
            final_score: 0.75,
        };
        assert!(!breakdown.is_valid());
    }

    #[test]
    fn score_breakdown_is_invalid_when_tool_success_rate_negative() {
        let breakdown = ScoreBreakdown {
            similarity_score: 0.85,
            tool_success_rate: -0.1,
            complexity_penalty: 0.1,
            feedback_adjustment: 0.05,
            final_score: 0.75,
        };
        assert!(!breakdown.is_valid());
    }

    #[test]
    fn score_breakdown_is_invalid_when_complexity_penalty_exceeds_one() {
        let breakdown = ScoreBreakdown {
            similarity_score: 0.85,
            tool_success_rate: 0.9,
            complexity_penalty: 1.5,
            feedback_adjustment: 0.05,
            final_score: 0.75,
        };
        assert!(!breakdown.is_valid());
    }

    #[test]
    fn score_breakdown_is_valid_when_feedback_adjustment_negative_in_range() {
        let breakdown = ScoreBreakdown {
            similarity_score: 0.85,
            tool_success_rate: 0.9,
            complexity_penalty: 0.1,
            feedback_adjustment: -0.5, // Negative feedback is valid (range is [-1.0, +1.0])
            final_score: 0.75,
        };
        assert!(breakdown.is_valid());
    }

    #[test]
    fn score_breakdown_is_invalid_when_feedback_adjustment_below_minus_one() {
        let breakdown = ScoreBreakdown {
            similarity_score: 0.85,
            tool_success_rate: 0.9,
            complexity_penalty: 0.1,
            feedback_adjustment: -1.5,
            final_score: 0.75,
        };
        assert!(!breakdown.is_valid());
    }

    #[test]
    fn score_breakdown_is_invalid_when_final_score_exceeds_one() {
        let breakdown = ScoreBreakdown {
            similarity_score: 0.85,
            tool_success_rate: 0.9,
            complexity_penalty: 0.1,
            feedback_adjustment: 0.05,
            final_score: 1.1,
        };
        assert!(!breakdown.is_valid());
    }

    #[test]
    fn plan_candidate_is_valid_when_confidence_and_breakdown_valid() {
        let candidate = PlanCandidate {
            id: "plan-1".to_string(),
            description: "Execute task in parallel".to_string(),
            confidence: 0.85,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.9,
                tool_success_rate: 0.85,
                complexity_penalty: 0.1,
                feedback_adjustment: 0.0,
                final_score: 0.8,
            },
        };
        assert!(candidate.is_valid());
    }

    #[test]
    fn plan_candidate_is_valid_at_confidence_boundaries() {
        let candidate = PlanCandidate {
            id: "plan-2".to_string(),
            description: "Sequential execution".to_string(),
            confidence: 0.0,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.5,
                tool_success_rate: 0.5,
                complexity_penalty: 0.5,
                feedback_adjustment: 0.0,
                final_score: 0.5,
            },
        };
        assert!(candidate.is_valid());

        let candidate2 = PlanCandidate {
            id: "plan-3".to_string(),
            description: "High confidence plan".to_string(),
            confidence: 1.0,
            score_breakdown: ScoreBreakdown {
                similarity_score: 1.0,
                tool_success_rate: 1.0,
                complexity_penalty: 0.0,
                feedback_adjustment: 0.0,
                final_score: 1.0,
            },
        };
        assert!(candidate2.is_valid());
    }

    #[test]
    fn plan_candidate_is_invalid_when_confidence_exceeds_one() {
        let candidate = PlanCandidate {
            id: "plan-4".to_string(),
            description: "Invalid confidence".to_string(),
            confidence: 1.5,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.9,
                tool_success_rate: 0.85,
                complexity_penalty: 0.1,
                feedback_adjustment: 0.0,
                final_score: 0.8,
            },
        };
        assert!(!candidate.is_valid());
    }

    #[test]
    fn plan_candidate_is_invalid_when_confidence_negative() {
        let candidate = PlanCandidate {
            id: "plan-5".to_string(),
            description: "Negative confidence".to_string(),
            confidence: -0.1,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.9,
                tool_success_rate: 0.85,
                complexity_penalty: 0.1,
                feedback_adjustment: 0.0,
                final_score: 0.8,
            },
        };
        assert!(!candidate.is_valid());
    }

    #[test]
    fn plan_candidate_is_invalid_when_breakdown_invalid() {
        let candidate = PlanCandidate {
            id: "plan-6".to_string(),
            description: "Invalid breakdown".to_string(),
            confidence: 0.85,
            score_breakdown: ScoreBreakdown {
                similarity_score: 1.1, // Invalid
                tool_success_rate: 0.85,
                complexity_penalty: 0.1,
                feedback_adjustment: 0.0,
                final_score: 0.8,
            },
        };
        assert!(!candidate.is_valid());
    }

    #[test]
    fn plan_candidate_serializes_to_json() {
        let candidate = PlanCandidate {
            id: "plan-1".to_string(),
            description: "Test plan".to_string(),
            confidence: 0.85,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.9,
                tool_success_rate: 0.85,
                complexity_penalty: 0.1,
                feedback_adjustment: 0.0,
                final_score: 0.8,
            },
        };

        let json = serde_json::to_value(&candidate).expect("serializes to JSON");
        assert_eq!(json["id"], "plan-1");
        assert_eq!(json["description"], "Test plan");
        assert!(json["confidence"].is_number());
        assert!(json["score_breakdown"]["similarity_score"].is_number());
        assert!(json["score_breakdown"]["final_score"].is_number());
    }

    #[test]
    fn plan_candidate_round_trips_through_json() {
        let original = PlanCandidate {
            id: "plan-1".to_string(),
            description: "Test plan".to_string(),
            confidence: 0.75,
            score_breakdown: ScoreBreakdown {
                similarity_score: 0.88,
                tool_success_rate: 0.92,
                complexity_penalty: 0.05,
                feedback_adjustment: 0.02,
                final_score: 0.82,
            },
        };

        let json = serde_json::to_string(&original).expect("serializes");
        let deserialized: PlanCandidate = serde_json::from_str(&json).expect("deserializes");

        assert_eq!(deserialized, original);
    }
}
