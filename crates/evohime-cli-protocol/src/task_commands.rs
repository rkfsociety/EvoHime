use super::client::CoreClient;
use evohime_desktop_ipc::generated;
use tokio::io::{AsyncRead, AsyncWrite};

impl<S> CoreClient<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn start(
        &mut self,
        task_id: String,
        prompt: String,
        workspace: String,
    ) -> Result<(), String> {
        self.write(generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: uuid::Uuid::new_v4().to_string(),
            client_id: self.client_id.clone(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            command: Some(generated::command_envelope::Command::StartTask(
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
        })
        .await
    }

    pub async fn stop(&mut self, task_id: String) -> Result<(), String> {
        self.write(generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: uuid::Uuid::new_v4().to_string(),
            client_id: self.client_id.clone(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            command: Some(generated::command_envelope::Command::StopTask(
                generated::StopTask { task_id },
            )),
        })
        .await
    }
}
