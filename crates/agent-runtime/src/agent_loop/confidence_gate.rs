use serde::{Deserialize, Serialize};
use crate::agent_loop::model_confidence::{ConfidenceReliability, ModelConfidenceSignal};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceBreakdown {
    pub model: SignalBreakdown,
    pub experience: SignalBreakdown,
    pub tools: SignalBreakdown,
    pub reflection: SignalBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalBreakdown {
    pub score: f32,
    pub reliability: String,
    pub source: Option<String>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceComputeResult {
    pub confidence_score: f32, // [0.0, 1.0]
    pub breakdown: ConfidenceBreakdown,
    pub reliability: std::collections::HashMap<String, String>,
    pub missing_signals: Vec<String>,
}

/// Compute overall confidence score from 4 independent signals
/// Formula: confidence = 0.35*model + 0.25*exp + 0.25*tool + 0.15*reflection
/// With reliability-based penalties applied after aggregation
pub fn compute_confidence(
    model_signal: &ModelConfidenceSignal,
    experience_alignment: f32,         // [0.0, 1.0]
    experience_reliability: ConfidenceReliability,
    tool_success_rate: f32,            // [0.0, 1.0]
    tool_reliability: ConfidenceReliability,
    reflection_confidence: f32,        // [0.0, 1.0]
    reflection_reliability: ConfidenceReliability,
    missing_signals: Vec<String>,
) -> ConfidenceComputeResult {
    // Weights: must sum to 1.0
    const WEIGHT_MODEL: f32 = 0.35;
    const WEIGHT_EXPERIENCE: f32 = 0.25;
    const WEIGHT_TOOLS: f32 = 0.25;
    const WEIGHT_REFLECTION: f32 = 0.15;

    // Raw weighted average (before penalties)
    let raw_score = WEIGHT_MODEL * model_signal.score
        + WEIGHT_EXPERIENCE * experience_alignment
        + WEIGHT_TOOLS * tool_success_rate
        + WEIGHT_REFLECTION * reflection_confidence;

    // Apply reliability penalties
    let mut score = raw_score;
    score -= model_signal.reliability.penalty();
    score -= experience_reliability.penalty();
    score -= tool_reliability.penalty();
    score -= reflection_reliability.penalty();

    // Clamp to [0.0, 1.0]
    let confidence_score = score.clamp(0.0, 1.0);

    // Build breakdown for transparency
    let breakdown = ConfidenceBreakdown {
        model: SignalBreakdown {
            score: model_signal.score,
            reliability: model_signal.reliability.as_str().to_string(),
            source: Some(model_signal.source.as_str().to_string()),
            details: None,
        },
        experience: SignalBreakdown {
            score: experience_alignment,
            reliability: experience_reliability.as_str().to_string(),
            source: None,
            details: None,
        },
        tools: SignalBreakdown {
            score: tool_success_rate,
            reliability: tool_reliability.as_str().to_string(),
            source: None,
            details: None,
        },
        reflection: SignalBreakdown {
            score: reflection_confidence,
            reliability: reflection_reliability.as_str().to_string(),
            source: None,
            details: None,
        },
    };

    let mut reliability = std::collections::HashMap::new();
    reliability.insert("model".to_string(), model_signal.reliability.as_str().to_string());
    reliability.insert("experience".to_string(), experience_reliability.as_str().to_string());
    reliability.insert("tools".to_string(), tool_reliability.as_str().to_string());
    reliability.insert("reflection".to_string(), reflection_reliability.as_str().to_string());

    ConfidenceComputeResult {
        confidence_score,
        breakdown,
        reliability,
        missing_signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weights_normalize() {
        const WEIGHT_MODEL: f32 = 0.35;
        const WEIGHT_EXPERIENCE: f32 = 0.25;
        const WEIGHT_TOOLS: f32 = 0.25;
        const WEIGHT_REFLECTION: f32 = 0.15;

        let sum = WEIGHT_MODEL + WEIGHT_EXPERIENCE + WEIGHT_TOOLS + WEIGHT_REFLECTION;
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_high_confidence() {
        let model = ModelConfidenceSignal {
            score: 0.9,
            reliability: ConfidenceReliability::High,
            source: crate::agent_loop::model_confidence::ConfidenceSource::LogProbs,
        };

        let result = compute_confidence(
            &model,
            0.85, // experience
            ConfidenceReliability::Medium,
            0.80, // tools
            ConfidenceReliability::High,
            0.75, // reflection
            ConfidenceReliability::Medium,
            vec![],
        );

        // Should be quite high (around 0.8+)
        assert!(result.confidence_score > 0.75);
    }

    #[test]
    fn test_compute_with_penalties() {
        let model = ModelConfidenceSignal {
            score: 0.8,
            reliability: ConfidenceReliability::VeryLow, // Penalty: -0.15
            source: crate::agent_loop::model_confidence::ConfidenceSource::FallbackUncertain,
        };

        let result = compute_confidence(
            &model,
            0.7,
            ConfidenceReliability::Low,    // Penalty: -0.1
            0.6,
            ConfidenceReliability::Low,    // Penalty: -0.1
            0.5,
            ConfidenceReliability::VeryLow, // Penalty: -0.15
            vec![],
        );

        // Should have significant penalties applied
        let raw = 0.35 * 0.8 + 0.25 * 0.7 + 0.25 * 0.6 + 0.15 * 0.5;
        let penalized = raw - 0.15 - 0.1 - 0.1 - 0.15;
        assert!((result.confidence_score - penalized.clamp(0.0, 1.0)).abs() < 0.01);
    }
}
