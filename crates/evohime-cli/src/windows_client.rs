use evohime_cli::Command;
use evohime_cli::ExitCode;

use crate::windows_endpoint::connect;
use crate::windows_output;
use crate::windows_watch::watch_events;

pub async fn run(command: Command) -> ExitCode {
    let mut client = match connect(0).await {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::CoreUnavailable;
        }
    };
    match command {
        Command::Doctor { json } => windows_output::print_doctor(json),
        Command::Run {
            prompt,
            workspace,
            workflow,
            json,
            detach,
        } => {
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
            watch_events(&mut client, &run_id, json).await
        }
        Command::Watch { task_id, json } => watch_events(&mut client, &task_id, json).await,
        Command::Status { task_id, json } => match client.snapshot(task_id.clone()).await {
            Ok(event) => {
                windows_output::print_event(&event, &task_id, json);
                ExitCode::Completed
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::CoreUnavailable
            }
        },
        Command::Cancel { task_id, json } => match client.stop(task_id.clone()).await {
            Ok(()) => windows_output::print_cancel_requested(&task_id, json),
            Err(error) => {
                eprintln!("{error}");
                ExitCode::CoreUnavailable
            }
        },
        Command::Resume { task_id, json } => watch_events(&mut client, &task_id, json).await,
    }
}
