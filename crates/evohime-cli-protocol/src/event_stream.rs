use super::{auth::validate_event_generation, client::CoreClient};
use evohime_desktop_ipc::{generated, transport};
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite};

impl<S> CoreClient<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub(crate) async fn read_event(&mut self) -> Result<generated::EventEnvelope, String> {
        let payload = transport::read_frame(&mut self.stream)
            .await
            .map_err(|error| error.to_string())?;
        let event = generated::EventEnvelope::decode(payload.as_slice())
            .map_err(|error| format!("protocol_error: {error}"))?;
        validate_event_generation(&event, &self.core_instance_id, self.session_epoch)?;
        self.sequence = self.sequence.max(event.sequence_id);
        Ok(event)
    }

    pub async fn next(&mut self) -> Result<generated::EventEnvelope, String> {
        self.read_event().await
    }
}
