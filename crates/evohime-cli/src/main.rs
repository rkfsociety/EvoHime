//! Command-line entry point for running and inspecting EvoHime Core tasks.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
use evohime_cli::{parse_args, ExitCode};

#[cfg(windows)]
#[path = "windows_endpoint.rs"]
mod endpoint;
#[cfg(target_os = "linux")]
#[path = "linux_endpoint.rs"]
mod endpoint;
#[cfg(any(windows, target_os = "linux"))]
mod windows_client;
#[cfg(any(windows, target_os = "linux"))]
mod windows_controls;
#[cfg(any(windows, target_os = "linux"))]
mod windows_event_output;
#[cfg(any(windows, target_os = "linux"))]
mod windows_output;
#[cfg(any(windows, target_os = "linux"))]
mod windows_run;
#[cfg(any(windows, target_os = "linux"))]
mod windows_watch;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse_args(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(ExitCode::InvalidInvocation as i32);
        }
    };
    #[cfg(any(windows, target_os = "linux"))]
    let code = windows_client::run(command).await;
    #[cfg(not(any(windows, target_os = "linux")))]
    let code = {
        let _ = command;
        eprintln!("core_unavailable: eva поддерживается в Windows и Linux-сборках EvoHime");
        ExitCode::CoreUnavailable
    };
    std::process::exit(code as i32);
}
