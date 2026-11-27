#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use evohime_evals_service::EvalsService;
use evohime_service_contract::serve_stdio;

fn main() -> ExitCode {
    let service = EvalsService::new();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-evals exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}
