use evohime_cli::ExitCode;

use crate::windows_endpoint::CoreClient;
use crate::windows_output;
use crate::windows_watch::watch_events;

pub(crate) async fn run_task(
    client: &mut CoreClient,
    prompt: String,
    workspace: String,
    workflow: Option<String>,
    json: bool,
    detach: bool,
) -> ExitCode {
    let request = evohime_cli_contract::RunRequest {
        schema_version: evohime_cli_contract::SCHEMA_VERSION,
        prompt: prompt.clone(),
        workspace: workspace.clone(),
        output_mode: if json {
            evohime_cli_contract::OutputMode::Ndjson
        } else {
            evohime_cli_contract::OutputMode::Human
        },
        approval_mode: evohime_cli_contract::ApprovalMode::DenyIfApprovalRequired,
        detach,
    };
    if evohime_cli_contract::validate_request(&request).is_err() {
        eprintln!("invalid invocation: bounded Core CLI request is invalid");
        return ExitCode::InvalidInvocation;
    }
    let run_id = uuid::Uuid::new_v4().to_string();
    let start_result = if let Some(template_id) = workflow {
        client
            .start_workflow(run_id.clone(), template_id, workspace)
            .await
    } else {
        client.start(run_id.clone(), prompt, workspace).await
    };
    if let Err(error) = start_result {
        eprintln!("{error}");
        return ExitCode::CoreUnavailable;
    }
    if detach {
        return windows_output::print_run_accepted(&run_id, json);
    }
    watch_events(client, &run_id, json).await
}
