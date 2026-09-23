use super::client::CoreClient;
use evohime_desktop_ipc::generated;
use tokio::io::{AsyncRead, AsyncWrite};

impl<S> CoreClient<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Starts a Core task using a validated request and returns its first event.
    pub async fn start(
        &mut self,
        task_id: String,
        prompt: String,
        workspace: String,
    ) -> Result<(), String> {
        self.write(
            self.command_envelope(generated::command_envelope::Command::StartTask(
                generated::StartTask {
                    task_id,
                    prompt,
                    workspace_path: workspace,
                    preferred_route_hint: String::new(),
                    execution_kind: "agent".into(),
                    conversation_id: String::new(),
                    client_message_id: String::new(),
                },
            )),
        )
        .await
    }

    /// Requests cancellation of the specified running task.
    pub async fn stop(&mut self, task_id: String) -> Result<(), String> {
        self.write(
            self.command_envelope(generated::command_envelope::Command::StopTask(
                generated::StopTask { task_id },
            )),
        )
        .await
    }
}
