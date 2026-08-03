use std::collections::HashMap;

use evohime_storage::{get_tools_success_rates, ToolMetricsReliability};
use sqlx::PgPool;
use uuid::Uuid;

use super::confidence_gate::{compute_confidence, ConfidenceComputeResult};
use super::model_confidence::{ConfidenceReliability, ModelConfidenceSignal};
use super::risk_engine::determine_risk_level;
use evohime_protocol::PlanStep;

fn tool_metrics_reliability_to_confidence(
    reliability: &ToolMetricsReliability,
) -> ConfidenceReliability {
    match reliability {
        ToolMetricsReliability::High => ConfidenceReliability::High,
        ToolMetricsReliability::Medium => ConfidenceReliability::Medium,
        ToolMetricsReliability::Low => ConfidenceReliability::Low,
    }
}

/// High-level compute_plan_confidence function
/// Orchestrates all signals and returns the final decision
pub async fn compute_plan_confidence(
    pool: &PgPool,
    plan_steps: &[PlanStep],
    model_signal: &ModelConfidenceSignal,
    _task_id: Uuid,
) -> ConfidenceComputeResult {
    // 1. Determine risk level from planned steps
    let _risk_level = determine_risk_level(plan_steps);

    // 2. Get tool success rates
    let tool_names: Vec<String> = plan_steps
        .iter()
        .map(|s| s.tool_name.clone())
        .collect();

    let tool_success_rates = match get_tools_success_rates(pool, &tool_names, 30).await {
        Ok(rates) => rates,
        Err(_) => vec![], // Fallback: no history available
    };

    // Build map of tool name -> success rate
    let mut tool_rate_map: HashMap<String, (f32, ToolMetricsReliability)> = HashMap::new();
    for rate in tool_success_rates {
        tool_rate_map.insert(rate.tool_name, (rate.smoothed_rate, rate.reliability.clone()));
    }

    // Get worst (most conservative) tool success rate
    let tool_success_rate = tool_names
        .iter()
        .filter_map(|name| tool_rate_map.get(name))
        .map(|(rate, _)| *rate)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.5);

    // Get worst tool reliability
    let tool_reliability = tool_names
        .iter()
        .filter_map(|name| tool_rate_map.get(name))
        .map(|(_, reliability)| reliability)
        .fold(ToolMetricsReliability::High, |worst, current| {
            // Worst is the one with highest numeric value (Low > Medium > High)
            if current_reliability_score(current) > current_reliability_score(&worst) {
                current.clone()
            } else {
                worst
            }
        });

    // 3. Experience alignment (simplified: placeholder)
    let experience_alignment = 0.5; // TODO: Retrieve from memory on plan creation

    // 4. Reflection confidence (simplified: start high, would be updated during execution)
    let reflection_confidence = 0.7;

    // 5. Track missing signals
    let mut missing_signals = Vec::new();
    if tool_names.is_empty() {
        missing_signals.push("no_tools_planned".to_string());
    }
    if model_signal.reliability == ConfidenceReliability::VeryLow {
        missing_signals.push("weak_model_confidence".to_string());
    }

    // 6. Compute overall confidence
    let result = compute_confidence(
        model_signal,
        experience_alignment,
        ConfidenceReliability::Low, // Experience: assume low reliability for now
        tool_success_rate,
        tool_metrics_reliability_to_confidence(&tool_reliability),
        reflection_confidence,
        ConfidenceReliability::Medium, // Reflection: assume medium for now
        missing_signals,
    );

    result
}

fn current_reliability_score(reliability: &ToolMetricsReliability) -> u8 {
    match reliability {
        ToolMetricsReliability::High => 0,
        ToolMetricsReliability::Medium => 1,
        ToolMetricsReliability::Low => 2,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_missing_signals_detection() {
        // This would be tested with actual database
    }
}
