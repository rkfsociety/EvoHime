//! Bridge agent-runtime LLM telemetry into PipelineMetrics.

use crate::observability::PipelineMetrics;
use evohime_agent_runtime::{LlmCallRecord, LlmTelemetry};
use std::sync::Arc;

pub struct PipelineLlmTelemetry {
    metrics: Arc<PipelineMetrics>,
}

impl PipelineLlmTelemetry {
    pub fn new(metrics: Arc<PipelineMetrics>) -> Arc<dyn LlmTelemetry> {
        Arc::new(Self { metrics })
    }
}

impl LlmTelemetry for PipelineLlmTelemetry {
    fn record(&self, call: &LlmCallRecord) {
        let usage = call
            .usage
            .map(|u| (u.prompt_tokens, u.completion_tokens));
        self.metrics.llm_call(
            call.phase,
            &call.model,
            usage,
            call.duration_ms,
            call.ok,
        );
    }
}
