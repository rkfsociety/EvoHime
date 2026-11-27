#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use evohime_ambient::AmbientService;
use evohime_service_contract::serve_stdio;

fn main() -> ExitCode {
    let service = AmbientService::new();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-ambient exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}
