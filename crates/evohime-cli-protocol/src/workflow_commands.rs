use super::client::CoreClient;
use evohime_desktop_ipc::generated;
use tokio::io::{AsyncRead, AsyncWrite};

impl<S> CoreClient<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn start_workflow(
        &mut self,
        task_id: String,
        template_id: String,
        workspace: String,
    ) -> Result<(), String> {
        self.write(generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: uuid::Uuid::new_v4().to_string(),
            client_id: self.client_id.clone(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            command: Some(generated::command_envelope::Command::StartWorkflow(
                generated::StartWorkflow {
                    template_id,
                    task_id,
                    workspace_path: workspace,
                    inputs: Vec::new(),
                    idempotency_key: uuid::Uuid::new_v4().to_string(),
                },
            )),
        })
        .await
    }
}
