//! Shared stream/event helpers for the agent loop.
use super::AgentError;
use evohime_protocol::ServerEvent;
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) const PLANNING_TIMEOUT: Duration = Duration::from_secs(90);
pub(crate) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) const MODEL_REQUEST_COOLDOWN: Duration = Duration::from_secs(6);
pub(crate) const MAX_REPLAN_ROUNDS: usize = 3;
pub(crate) fn emit(event_tx: &UnboundedSender<ServerEvent>, event: ServerEvent) -> Result<(), AgentError> {
    event_tx.send(event).map_err(|_| AgentError::EventChannel)
}
pub(crate) async fn collect_stream_text(
    mut stream: impl futures_util::Stream<Item = Result<String, evohime_model_gateway::providers::ProviderError>>
        + Unpin,
) -> Result<String, AgentError> {
    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        output.push_str(&chunk?);
    }
    Ok(output)
}

pub(crate) async fn collect_stream_text_with_timeout(
    stream: impl futures_util::Stream<Item = Result<String, evohime_model_gateway::providers::ProviderError>>
        + Unpin,
    timeout: Duration,
    phase: &'static str,
) -> Result<String, AgentError> {
    tokio::time::timeout(timeout, collect_stream_text(stream))
        .await
        .map_err(|_| AgentError::ModelTimeout {
            phase,
            timeout_seconds: timeout.as_secs(),
        })?
}
