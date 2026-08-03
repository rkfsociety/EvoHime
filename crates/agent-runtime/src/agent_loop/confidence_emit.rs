use chrono::Utc;
use evohime_protocol::ServerEvent;
use tokio::sync::mpsc::UnboundedSender;

use super::confidence_compute::compute_plan_confidence;
use super::confidence_gate::ConfidenceComputeResult;
use super::model_confidence::{ModelConfidenceSignal, ConfidenceSource, ConfidenceReliability};
use crate::AgentConfig;
use evohime_protocol::PlanStep;
use sqlx::PgPool;

/// Emit confidence event before executing a tool
pub async fn emit_confidence_before_tool(
    config: &AgentConfig,
    event_tx: &UnboundedSender<ServerEvent>,
    pool: Option<&PgPool>,
    plan_steps: &[PlanStep],
) -> Result<ConfidenceComputeResult, Box<dyn std::error::Error>> {
    // Use fallback model confidence if pool not available
    let model_signal = ModelConfidenceSignal {
        score: 0.7,
        reliability: ConfidenceReliability::Low,
        source: ConfidenceSource::FallbackUncertain,
    };

    let result = if let Some(pool) = pool {
        compute_plan_confidence(pool, plan_steps, &model_signal, config.task_id)
            .await
    } else {
        // Fallback: simple computation without DB access
        super::confidence_gate::compute_confidence(
            &model_signal,
            0.5,
            ConfidenceReliability::Low,
            0.5,
            ConfidenceReliability::Low,
            0.5,
            ConfidenceReliability::Low,
            vec!["no_database".to_string()],
        )
    };

    // Emit confidence event
    let risk_level = super::risk_engine::determine_risk_level(plan_steps);
    let decision = super::ask_policy::decide_ask_policy(
        result.confidence_score,
        risk_level,
        result.missing_signals.len(),
        &super::ask_policy::AskPolicyConfig::default(),
    );
    let recommendation = decision.as_str().to_string();

    let _ = event_tx.send(ServerEvent::AgentConfidence {
        task_id: config.task_id,
        timestamp: Utc::now(),
        confidence_version: "1".to_string(),
        confidence_score: result.confidence_score,
        risk_level: risk_level.as_str().to_string(),
        breakdown: serde_json::to_value(&result.breakdown).unwrap_or(serde_json::json!({})),
        reliability: serde_json::to_value(&result.reliability).unwrap_or(serde_json::json!({})),
        missing_signals: result.missing_signals.clone(),
        recommendation,
    });

    Ok(result)
}
