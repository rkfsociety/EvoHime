//! Shared stream/event helpers for the agent loop.
use super::AgentError;
use evohime_protocol::ServerEvent;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) const MODEL_REQUEST_COOLDOWN: Duration = Duration::from_secs(6);

pub(crate) fn emit(
    event_tx: &UnboundedSender<ServerEvent>,
    event: ServerEvent,
) -> Result<(), AgentError> {
    event_tx.send(event).map_err(|_| AgentError::EventChannel)
}
