#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use evohime_model_adapters::ModelAdaptersService;
use evohime_service_contract::serve_stdio;

fn main() -> ExitCode {
    let service = ModelAdaptersService::new();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-model-adapters exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}
