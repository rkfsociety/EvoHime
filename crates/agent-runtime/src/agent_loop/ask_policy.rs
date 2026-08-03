use serde::{Deserialize, Serialize};
use crate::agent_loop::risk_engine::RiskLevel;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskDecision {
    Proceed,
    Ask,
    RequireApproval,
}

impl AskDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            AskDecision::Proceed => "proceed",
            AskDecision::Ask => "ask",
            AskDecision::RequireApproval => "require_approval",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskPolicyConfig {
    /// Thresholds for each risk level
    pub risk_thresholds: RiskThresholds,
    /// Minimum confidence threshold when >=2 signals missing
    pub missing_signal_ask_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskThresholds {
    pub none: ThresholdPair,
    pub low: ThresholdPair,
    pub medium: ThresholdPair,
    pub high: ThresholdPair,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdPair {
    pub proceed: f32,
    pub ask: f32,
    /// High-risk tasks might have a require_approval floor
    pub require: Option<f32>,
}

impl Default for AskPolicyConfig {
    fn default() -> Self {
        Self {
            risk_thresholds: RiskThresholds {
                none: ThresholdPair {
                    proceed: 0.65,
                    ask: 0.40,
                    require: None,
                },
                low: ThresholdPair {
                    proceed: 0.70,
                    ask: 0.45,
                    require: None,
                },
                medium: ThresholdPair {
                    proceed: 0.75,
                    ask: 0.50,
                    require: None,
                },
                high: ThresholdPair {
                    proceed: 0.85,
                    ask: 0.65,
                    require: Some(0.30),
                },
            },
            missing_signal_ask_threshold: 0.5,
        }
    }
}

/// Determine ask decision based on confidence and risk level
pub fn decide_ask_policy(
    confidence_score: f32,
    risk_level: RiskLevel,
    missing_signals_count: usize,
    config: &AskPolicyConfig,
) -> AskDecision {
    // High-risk always requires approval if confidence is below require threshold
    if risk_level == RiskLevel::High {
        if let Some(require_threshold) = config.risk_thresholds.high.require {
            if confidence_score < require_threshold {
                return AskDecision::RequireApproval;
            }
        }
    }

    // If 2+ signals missing, be more conservative
    if missing_signals_count >= 2 {
        if confidence_score < config.missing_signal_ask_threshold {
            return AskDecision::RequireApproval;
        } else if confidence_score < (config.missing_signal_ask_threshold + 0.2) {
            return AskDecision::Ask;
        }
    }

    // Get thresholds for this risk level
    let thresholds = match risk_level {
        RiskLevel::None => &config.risk_thresholds.none,
        RiskLevel::Low => &config.risk_thresholds.low,
        RiskLevel::Medium => &config.risk_thresholds.medium,
        RiskLevel::High => &config.risk_thresholds.high,
    };

    // Apply risk-aware thresholds
    if confidence_score >= thresholds.proceed {
        AskDecision::Proceed
    } else if confidence_score >= thresholds.ask {
        AskDecision::Ask
    } else {
        AskDecision::RequireApproval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_high_risk_requires_approval() {
        let config = AskPolicyConfig::default();
        let require_threshold = config.risk_thresholds.high.require.unwrap();

        let decision = decide_ask_policy(require_threshold - 0.01, RiskLevel::High, 0, &config);
        assert_eq!(decision, AskDecision::RequireApproval);

        let decision = decide_ask_policy(require_threshold + 0.01, RiskLevel::High, 0, &config);
        assert_ne!(decision, AskDecision::RequireApproval);
    }

    #[test]
    fn test_missing_signals_trigger_ask() {
        let config = AskPolicyConfig::default();
        let threshold = config.missing_signal_ask_threshold;

        // With 2+ missing signals and low confidence
        let decision = decide_ask_policy(threshold - 0.1, RiskLevel::None, 2, &config);
        assert_eq!(decision, AskDecision::RequireApproval);

        // With 2+ missing signals and medium confidence
        let decision = decide_ask_policy(threshold + 0.1, RiskLevel::None, 2, &config);
        assert_eq!(decision, AskDecision::Ask);
    }

    #[test]
    fn test_risk_aware_thresholds() {
        let config = AskPolicyConfig::default();

        let conf = 0.73;

        // Low risk: threshold 0.70, should proceed
        let decision = decide_ask_policy(conf, RiskLevel::Low, 0, &config);
        assert_eq!(decision, AskDecision::Proceed);

        // High risk: threshold 0.85, should ask
        let decision = decide_ask_policy(conf, RiskLevel::High, 0, &config);
        assert_eq!(decision, AskDecision::Ask);
    }

    #[test]
    fn test_decision_ordering() {
        let config = AskPolicyConfig::default();

        // Very low confidence
        let decision = decide_ask_policy(0.1, RiskLevel::None, 0, &config);
        assert_eq!(decision, AskDecision::RequireApproval);

        // Medium confidence
        let decision = decide_ask_policy(0.55, RiskLevel::None, 0, &config);
        assert_eq!(decision, AskDecision::Ask);

        // High confidence
        let decision = decide_ask_policy(0.9, RiskLevel::None, 0, &config);
        assert_eq!(decision, AskDecision::Proceed);
    }
}
