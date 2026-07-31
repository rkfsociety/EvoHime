//! LLM call telemetry for TokenJam / OpenTelemetry GenAI semconv (Stage 7.34).

use evohime_model_gateway::LlmUsage;
use uuid::Uuid;

/// One completed (or failed) model invocation.
#[derive(Debug, Clone)]
pub struct LlmCallRecord {
    pub phase: &'static str,
    pub provider: String,
    pub model: String,
    pub session_id: Uuid,
    pub task_id: Uuid,
    pub usage: Option<LlmUsage>,
    pub duration_ms: u64,
    pub ok: bool,
}

/// Optional sink for aggregated counters (server PipelineMetrics).
pub trait LlmTelemetry: Send + Sync {
    fn record(&self, call: &LlmCallRecord);
}
