//! Shared stream/event helpers for the agent loop.
use super::AgentError;
use evohime_model_gateway::{ChatStreamItem, LlmUsage};
use evohime_protocol::ServerEvent;
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) const PLANNING_TIMEOUT: Duration = Duration::from_secs(90);
pub(crate) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) const MODEL_REQUEST_COOLDOWN: Duration = Duration::from_secs(6);
pub(crate) const MAX_REPLAN_ROUNDS: usize = 3;

pub(crate) fn emit(
    event_tx: &UnboundedSender<ServerEvent>,
    event: ServerEvent,
) -> Result<(), AgentError> {
    event_tx.send(event).map_err(|_| AgentError::EventChannel)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CollectedStream {
    pub text: String,
    pub usage: Option<LlmUsage>,
}

pub(crate) async fn collect_stream(
    mut stream: impl futures_util::Stream<
            Item = Result<ChatStreamItem, evohime_model_gateway::providers::ProviderError>,
        > + Unpin,
) -> Result<CollectedStream, AgentError> {
    let mut output = CollectedStream::default();
    while let Some(chunk) = stream.next().await {
        match chunk? {
            ChatStreamItem::Delta(text) => output.text.push_str(&text),
            ChatStreamItem::Usage(usage) => output.usage = Some(usage),
        }
    }
    Ok(output)
}

pub(crate) async fn collect_stream_text_with_timeout(
    stream: impl futures_util::Stream<
            Item = Result<ChatStreamItem, evohime_model_gateway::providers::ProviderError>,
        > + Unpin,
    timeout: Duration,
    phase: &'static str,
) -> Result<CollectedStream, AgentError> {
    tokio::time::timeout(timeout, collect_stream(stream))
        .await
        .map_err(|_| AgentError::ModelTimeout {
            phase,
            timeout_seconds: timeout.as_secs(),
        })?
}

/// Collect a model stream and emit GenAI / TokenJam-compatible telemetry.
pub(crate) async fn collect_llm_stream_with_telemetry(
    config: &super::AgentConfig,
    gateway: &evohime_model_gateway::ModelGateway,
    route: &str,
    model: Option<&str>,
    phase: &'static str,
    stream: impl futures_util::Stream<
            Item = Result<ChatStreamItem, evohime_model_gateway::providers::ProviderError>,
        > + Unpin,
    timeout: Duration,
) -> Result<CollectedStream, AgentError> {
    let provider = gateway
        .route_provider_kind(route)
        .map(|kind| kind.as_str().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let model_name = gateway
        .resolve_model_name(route, model)
        .unwrap_or_else(|_| model.unwrap_or("unknown").to_string());
    let meta = crate::llm_telemetry::LlmCallMeta {
        phase,
        provider,
        model: model_name,
        session_id: config.session_id,
        task_id: config.task_id,
    };
    let (span, started) = crate::llm_telemetry::start_llm_span(&meta);
    let result = {
        let _guard = span.enter();
        collect_stream_text_with_timeout(stream, timeout, phase).await
    };
    let (usage, ok) = match &result {
        Ok(collected) => (collected.usage, true),
        Err(_) => (None, false),
    };
    crate::llm_telemetry::finish_llm_span(
        &span,
        started,
        &meta,
        usage,
        ok,
        config.telemetry.as_ref(),
    );
    result
}
