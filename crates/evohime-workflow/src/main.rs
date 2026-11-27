#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use evohime_service_contract::serve_stdio;
use evohime_workflow::WorkflowEngineService;

fn main() -> ExitCode {
    let service = WorkflowEngineService::new();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-workflow exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}
