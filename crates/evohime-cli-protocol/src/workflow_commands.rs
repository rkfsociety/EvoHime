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
        self.write(
            self.command_envelope(generated::command_envelope::Command::StartWorkflow(
                generated::StartWorkflow {
                    template_id,
                    task_id,
                    workspace_path: workspace,
                    inputs: Vec::new(),
                    idempotency_key: uuid::Uuid::new_v4().to_string(),
                },
            )),
        )
        .await
    }
}
