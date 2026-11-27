#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use evohime_rag_memory::RagMemoryService;
use evohime_service_contract::serve_stdio;

fn main() -> ExitCode {
    let service = RagMemoryService::new();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-rag-memory exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}
