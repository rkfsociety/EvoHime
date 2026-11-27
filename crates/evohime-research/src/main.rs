#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use evohime_research::ResearchService;
use evohime_service_contract::serve_stdio;

fn main() -> ExitCode {
    let service = ResearchService::new();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-research exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}
