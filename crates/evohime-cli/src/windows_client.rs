use evohime_cli::Command;
use evohime_cli::ExitCode;

use crate::windows_endpoint::connect;
use crate::windows_output;
use crate::windows_run::run_task;
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
        } => run_task(&mut client, prompt, workspace, workflow, json, detach).await,
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
