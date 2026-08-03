use evohime_agent_runtime::agent_loop::{
    ask_policy::{decide_ask_policy, AskDecision, AskPolicyConfig},
    confidence_gate::compute_confidence,
    model_confidence::{ConfidenceReliability, ModelConfidenceSignal, ConfidenceSource},
    risk_engine::RiskLevel,
};

#[test]
fn test_high_risk_requires_approval() {
    let config = AskPolicyConfig::default();
    let require_threshold = config.risk_thresholds.high.require.unwrap();

    // Just below require threshold for high risk
    let decision = decide_ask_policy(require_threshold - 0.01, RiskLevel::High, 0, &config);
    assert_eq!(decision, AskDecision::RequireApproval);

    // Above require threshold but below proceed threshold
    let decision = decide_ask_policy(require_threshold + 0.05, RiskLevel::High, 0, &config);
    assert_eq!(decision, AskDecision::Ask);

    // Above proceed threshold for high risk
    let decision = decide_ask_policy(
        config.risk_thresholds.high.proceed + 0.01,
        RiskLevel::High,
        0,
        &config,
    );
    assert_eq!(decision, AskDecision::Proceed);
}

#[test]
fn test_missing_signals_trigger_ask() {
    let config = AskPolicyConfig::default();
    let threshold = config.missing_signal_ask_threshold;

    // With 2+ missing signals and confidence below threshold
    let decision = decide_ask_policy(threshold - 0.1, RiskLevel::None, 2, &config);
    assert_eq!(decision, AskDecision::RequireApproval);

    // With 2+ missing signals and confidence in ask range
    let decision = decide_ask_policy(threshold + 0.1, RiskLevel::None, 2, &config);
    assert_eq!(decision, AskDecision::Ask);

    // With only 1 missing signal (below threshold)
    let decision = decide_ask_policy(0.4, RiskLevel::None, 1, &config);
    assert_ne!(decision, AskDecision::RequireApproval);
}

#[test]
fn test_confidence_score_normalization() {
    let model_signal = ModelConfidenceSignal {
        score: 0.8,
        reliability: ConfidenceReliability::High,
        source: ConfidenceSource::LogProbs,
    };

    let result = compute_confidence(
        &model_signal,
        0.7,  // experience
        ConfidenceReliability::Medium,
        0.6,  // tools
        ConfidenceReliability::High,
        0.5,  // reflection
        ConfidenceReliability::Medium,
        vec![],
    );

    // Score should be clamped to [0.0, 1.0]
    assert!(result.confidence_score >= 0.0 && result.confidence_score <= 1.0);

    // Score should be reasonable (weighted avg with penalties)
    let raw: f32 = 0.35 * 0.8 + 0.25 * 0.7 + 0.25 * 0.6 + 0.15 * 0.5;
    let with_penalties: f32 = raw - 0.05 - 0.05 - 0.05; // Small penalties from Medium reliability
    assert!((result.confidence_score - with_penalties.clamp(0.0, 1.0)).abs() < 0.05);
}

#[test]
fn test_confidence_with_low_reliability_penalty() {
    let model_low = ModelConfidenceSignal {
        score: 0.8,
        reliability: ConfidenceReliability::VeryLow, // Heavy penalty
        source: ConfidenceSource::FallbackUncertain,
    };

    let model_high = ModelConfidenceSignal {
        score: 0.8,
        reliability: ConfidenceReliability::High, // No penalty
        source: ConfidenceSource::LogProbs,
    };

    let result_low = compute_confidence(
        &model_low,
        0.7,
        ConfidenceReliability::Medium,
        0.6,
        ConfidenceReliability::Medium,
        0.5,
        ConfidenceReliability::Medium,
        vec![],
    );

    let result_high = compute_confidence(
        &model_high,
        0.7,
        ConfidenceReliability::Medium,
        0.6,
        ConfidenceReliability::Medium,
        0.5,
        ConfidenceReliability::Medium,
        vec![],
    );

    // Low reliability score should be notably lower
    assert!(result_low.confidence_score < result_high.confidence_score);
}

#[test]
fn test_risk_aware_thresholds() {
    let config = AskPolicyConfig::default();
    let test_confidence = 0.73;

    // Same confidence with different risk levels
    let decision_none = decide_ask_policy(test_confidence, RiskLevel::None, 0, &config);
    let decision_medium = decide_ask_policy(test_confidence, RiskLevel::Medium, 0, &config);
    let decision_high = decide_ask_policy(test_confidence, RiskLevel::High, 0, &config);

    // Lower risk should proceed, higher risk should ask
    assert_eq!(decision_none, AskDecision::Proceed);
    assert_eq!(decision_medium, AskDecision::Proceed);
    assert_eq!(decision_high, AskDecision::Ask);
}

#[test]
fn test_missing_signals_tracked() {
    let model_signal = ModelConfidenceSignal {
        score: 0.5,
        reliability: ConfidenceReliability::VeryLow,
        source: ConfidenceSource::FallbackUncertain,
    };

    let result = compute_confidence(
        &model_signal,
        0.5,
        ConfidenceReliability::Low,
        0.5,
        ConfidenceReliability::Low,
        0.5,
        ConfidenceReliability::Low,
        vec!["no_memory".to_string(), "no_tool_stats".to_string()],
    );

    assert_eq!(result.missing_signals.len(), 2);
    assert!(result.missing_signals.contains(&"no_memory".to_string()));
    assert!(result.missing_signals.contains(&"no_tool_stats".to_string()));
}

#[test]
fn test_breakdown_structure() {
    let model_signal = ModelConfidenceSignal {
        score: 0.9,
        reliability: ConfidenceReliability::High,
        source: ConfidenceSource::LogProbs,
    };

    let result = compute_confidence(
        &model_signal,
        0.8,
        ConfidenceReliability::High,
        0.7,
        ConfidenceReliability::Medium,
        0.6,
        ConfidenceReliability::Medium,
        vec![],
    );

    // Verify breakdown has all signals
    assert_eq!(result.breakdown.model.score, 0.9);
    assert_eq!(result.breakdown.experience.score, 0.8);
    assert_eq!(result.breakdown.tools.score, 0.7);
    assert_eq!(result.breakdown.reflection.score, 0.6);

    // Verify reliability tracking
    assert!(result.reliability.contains_key("model"));
    assert!(result.reliability.contains_key("experience"));
    assert!(result.reliability.contains_key("tools"));
    assert!(result.reliability.contains_key("reflection"));
}
