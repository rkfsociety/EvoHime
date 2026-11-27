#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use evohime_computer_use::ComputerUseService;
use evohime_service_contract::serve_stdio;

fn main() -> ExitCode {
    let service = ComputerUseService::new();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-computer-use exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}
