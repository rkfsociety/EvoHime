#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use evohime_service_contract::serve_stdio;
use evohime_tools_workspace::ToolsWorkspaceService;

fn main() -> ExitCode {
    let service = ToolsWorkspaceService::new();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-tools-workspace exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}
