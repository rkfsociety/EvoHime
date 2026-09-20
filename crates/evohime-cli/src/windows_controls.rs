use evohime_cli::ExitCode;

use crate::windows_endpoint::CoreClient;
use crate::windows_output;

pub(crate) async fn status(client: &mut CoreClient, task_id: String, json: bool) -> ExitCode {
    match client.snapshot(task_id.clone()).await {
        Ok(event) => {
            windows_output::print_event(&event, &task_id, json);
            ExitCode::Completed
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::CoreUnavailable
        }
    }
}

pub(crate) async fn cancel(client: &mut CoreClient, task_id: String, json: bool) -> ExitCode {
    match client.stop(task_id.clone()).await {
        Ok(()) => windows_output::print_cancel_requested(&task_id, json),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::CoreUnavailable
        }
    }
}
