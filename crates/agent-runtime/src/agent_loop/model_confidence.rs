use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceReliability {
    High,
    Medium,
    Low,
    VeryLow,
}

impl ConfidenceReliability {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfidenceReliability::High => "high",
            ConfidenceReliability::Medium => "medium",
            ConfidenceReliability::Low => "low",
            ConfidenceReliability::VeryLow => "very_low",
        }
    }

    /// Penalty to apply to overall confidence score based on reliability
    pub fn penalty(&self) -> f32 {
        match self {
            ConfidenceReliability::High => 0.0,
            ConfidenceReliability::Medium => 0.05,
            ConfidenceReliability::Low => 0.1,
            ConfidenceReliability::VeryLow => 0.15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfidenceSignal {
    pub score: f32,      // [0.0, 1.0]
    pub reliability: ConfidenceReliability,
    pub source: ConfidenceSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceSource {
    LogProbs,
    StructuredOutput,
    ThinkingTokens,
    Heuristics,
    FallbackUncertain,
}

impl ConfidenceSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfidenceSource::LogProbs => "logprobs",
            ConfidenceSource::StructuredOutput => "structured_output",
            ConfidenceSource::ThinkingTokens => "thinking_tokens",
            ConfidenceSource::Heuristics => "heuristics",
            ConfidenceSource::FallbackUncertain => "fallback_uncertain",
        }
    }
}

/// Extract model confidence from completion statistics
/// Priority order:
/// 1. Logprobs if available (high reliability)
/// 2. Structured output confidence field (medium reliability)
/// 3. Thinking token count / total tokens (low reliability - thinking ≠ confidence)
/// 4. Keyword heuristics (very_low reliability)
/// 5. Fallback: neutral 0.5 (low reliability)
pub fn extract_model_confidence(
    completion: &serde_json::Value,
    thinking_tokens_opt: Option<u32>,
    total_tokens: u32,
) -> ModelConfidenceSignal {
    // Try logprobs first (if provider supports)
    if let Some(logprobs) = completion.get("logprobs").and_then(|lp| lp.as_object()) {
        if let Some(score) = extract_from_logprobs(logprobs) {
            return ModelConfidenceSignal {
                score,
                reliability: ConfidenceReliability::High,
                source: ConfidenceSource::LogProbs,
            };
        }
    }

    // Try structured output confidence field
    if let Some(confidence) = completion
        .get("confidence")
        .and_then(|c| c.as_f64())
        .map(|c| c as f32)
    {
        if confidence >= 0.0 && confidence <= 1.0 {
            return ModelConfidenceSignal {
                score: confidence,
                reliability: ConfidenceReliability::Medium,
                source: ConfidenceSource::StructuredOutput,
            };
        }
    }

    // Try thinking tokens as heuristic (longer thinking ≠ higher confidence, but think about it carefully)
    if let Some(thinking_tokens) = thinking_tokens_opt {
        // Heuristic: if thinking is <20% of total, might indicate uncertainty
        // If thinking is >60% of total, model is being thorough but might indicate uncertainty too
        let thinking_ratio = thinking_tokens as f32 / (total_tokens as f32).max(1.0);

        let score = if thinking_ratio < 0.1 {
            0.6 // Limited thinking → less thorough
        } else if thinking_ratio < 0.2 {
            0.65
        } else if thinking_ratio < 0.5 {
            0.70 // Normal thinking depth
        } else {
            0.65 // Excessive thinking might indicate uncertainty
        };

        return ModelConfidenceSignal {
            score,
            reliability: ConfidenceReliability::Low,
            source: ConfidenceSource::ThinkingTokens,
        };
    }

    // Try keyword heuristics on the completion text
    if let Some(text) = completion.get("content").and_then(|c| c.as_str()) {
        if let Some(score) = extract_from_keywords(text) {
            return ModelConfidenceSignal {
                score,
                reliability: ConfidenceReliability::VeryLow,
                source: ConfidenceSource::Heuristics,
            };
        }
    }

    // Fallback: neutral, low reliability
    ModelConfidenceSignal {
        score: 0.5,
        reliability: ConfidenceReliability::Low,
        source: ConfidenceSource::FallbackUncertain,
    }
}

fn extract_from_logprobs(logprobs: &serde_json::Map<String, serde_json::Value>) -> Option<f32> {
    // Average the probabilities of tokens (simplified approach)
    // Real implementation would be more sophisticated
    let mut total_prob: f32 = 0.0;
    let mut count = 0;

    for (_key, value) in logprobs.iter() {
        if let Some(prob) = value.as_f64() {
            total_prob += (prob as f32).exp(); // logprob → probability
            count += 1;
        }
    }

    if count > 0 {
        let avg_prob = total_prob / (count as f32);
        Some(avg_prob.clamp(0.0, 1.0))
    } else {
        None
    }
}

fn extract_from_keywords(text: &str) -> Option<f32> {
    let text_lower = text.to_lowercase();

    let uncertain_keywords = [
        "maybe", "perhaps", "i think", "i'm not sure", "uncertain", "unclear",
        "could be", "might be", "possibly", "probably not", "i doubt",
    ];

    let confident_keywords = [
        "definitely", "certainly", "clearly", "obviously", "absolutely",
        "without doubt", "absolutely sure",
    ];

    let mut uncertain_count = 0;
    let mut confident_count = 0;

    for keyword in &uncertain_keywords {
        if text_lower.contains(keyword) {
            uncertain_count += 1;
        }
    }

    for keyword in &confident_keywords {
        if text_lower.contains(keyword) {
            confident_count += 1;
        }
    }

    if uncertain_count > 0 || confident_count > 0 {
        // Score based on ratio
        let total = (uncertain_count + confident_count) as f32;
        let confidence = (confident_count as f32) / total;
        Some(confidence)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_reliability_penalty() {
        assert_eq!(ConfidenceReliability::High.penalty(), 0.0);
        assert_eq!(ConfidenceReliability::Medium.penalty(), 0.05);
        assert_eq!(ConfidenceReliability::Low.penalty(), 0.1);
        assert_eq!(ConfidenceReliability::VeryLow.penalty(), 0.15);
    }

    #[test]
    fn test_fallback_confidence() {
        let signal = extract_model_confidence(&serde_json::json!({}), None, 100);
        assert_eq!(signal.score, 0.5);
        assert_eq!(signal.reliability, ConfidenceReliability::Low);
    }

    #[test]
    fn test_keyword_extraction() {
        let text = "I'm not sure about this, but I think it might work";
        let score = extract_from_keywords(text).unwrap();
        assert!(score < 0.7); // More uncertain keywords
    }

    #[test]
    fn test_thinking_ratio_heuristic() {
        // 10% thinking
        let signal = extract_model_confidence(&serde_json::json!({}), Some(10), 100);
        assert!(signal.score < 0.7);

        // 30% thinking (normal)
        let signal = extract_model_confidence(&serde_json::json!({}), Some(30), 100);
        assert!(signal.score >= 0.65);
    }
}
