//! LLM call telemetry for TokenJam / OpenTelemetry GenAI semconv (Stage 7.34).

use evohime_model_gateway::LlmUsage;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info_span, Span};
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

/// Open a GenAI client span (exported via OTLP when configured).
pub fn start_llm_span(meta: &LlmCallMeta) -> (Span, Instant) {
    let span = info_span!(
        "gen_ai.chat",
        otel.name = %format!("chat {}", meta.model),
        otel.kind = "client",
        gen_ai.operation.name = "chat",
        gen_ai.provider.name = %meta.provider,
        gen_ai.request.model = %meta.model,
        gen_ai.usage.input_tokens = tracing::field::Empty,
        gen_ai.usage.output_tokens = tracing::field::Empty,
        session.id = %meta.session_id,
        correlation_id = %meta.task_id,
        evohime.llm.phase = %meta.phase,
    );
    (span, Instant::now())
}

#[derive(Debug, Clone)]
pub struct LlmCallMeta {
    pub phase: &'static str,
    pub provider: String,
    pub model: String,
    pub session_id: Uuid,
    pub task_id: Uuid,
}

pub fn finish_llm_span(
    span: &Span,
    started: Instant,
    meta: &LlmCallMeta,
    usage: Option<LlmUsage>,
    ok: bool,
    telemetry: Option<&Arc<dyn LlmTelemetry>>,
) {
    if let Some(usage) = usage {
        span.record("gen_ai.usage.input_tokens", usage.prompt_tokens as i64);
        span.record(
            "gen_ai.usage.output_tokens",
            usage.completion_tokens as i64,
        );
    }
    let duration_ms = started.elapsed().as_millis() as u64;
    if !ok {
        span.record("otel.status_code", "ERROR");
    }
    if let Some(sink) = telemetry {
        sink.record(&LlmCallRecord {
            phase: meta.phase,
            provider: meta.provider.clone(),
            model: meta.model.clone(),
            session_id: meta.session_id,
            task_id: meta.task_id,
            usage,
            duration_ms,
            ok,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Capture(Mutex<Vec<LlmCallRecord>>);

    impl LlmTelemetry for Capture {
        fn record(&self, call: &LlmCallRecord) {
            self.0.lock().expect("lock").push(call.clone());
        }
    }

    #[test]
    fn finish_records_usage_to_sink() {
        let capture = Arc::new(Capture(Mutex::new(Vec::new())));
        let sink: Arc<dyn LlmTelemetry> = capture.clone();
        let meta = LlmCallMeta {
            phase: "plan",
            provider: "mock".into(),
            model: "mock-model".into(),
            session_id: Uuid::nil(),
            task_id: Uuid::nil(),
        };
        let (span, started) = start_llm_span(&meta);
        let _enter = span.enter();
        finish_llm_span(
            &span,
            started,
            &meta,
            Some(LlmUsage::from_parts(10, 5)),
            true,
            Some(&sink),
        );
        let rows = capture.0.lock().expect("lock");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].usage.map(|u| u.total_tokens), Some(15));
        assert_eq!(rows[0].phase, "plan");
    }
}
