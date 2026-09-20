use super::client::CoreClient;
use evohime_desktop_ipc::generated;
use tokio::io::{AsyncRead, AsyncWrite};

const MAX_SNAPSHOT_INTERLEAVED_EVENTS: usize = 128;

impl<S> CoreClient<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn snapshot(&mut self, task_id: String) -> Result<generated::EventEnvelope, String> {
        let expected_task_id = task_id.clone();
        self.write(
            self.command_envelope(generated::command_envelope::Command::GetTaskSnapshot(
                generated::GetTaskSnapshot {
                    project_id: String::new(),
                    task_id,
                },
            )),
        )
        .await?;
        for _ in 0..=MAX_SNAPSHOT_INTERLEAVED_EVENTS {
            let event = self.read_event().await?;
            if event.event_type == "task.snapshot" && event.task_id == expected_task_id {
                return Ok(event);
            }
        }
        Err("protocol_error: task snapshot response missing".into())
    }
}
