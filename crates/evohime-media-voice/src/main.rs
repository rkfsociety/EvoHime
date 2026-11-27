#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use evohime_media_voice::MediaVoiceService;
use evohime_service_contract::serve_stdio;

fn main() -> ExitCode {
    let service = MediaVoiceService::new();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-media-voice exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}
