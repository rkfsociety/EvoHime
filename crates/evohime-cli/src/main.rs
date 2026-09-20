use evohime_cli::{parse_args, ExitCode};

#[cfg(windows)]
mod windows_client;
#[cfg(windows)]
mod windows_endpoint;
#[cfg(windows)]
mod windows_output;

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
    #[cfg(windows)]
    let code = windows_client::run(command).await;
    #[cfg(not(windows))]
    let code = {
        let _ = command;
        eprintln!("core_unavailable: eva поддерживается только в Windows-сборке EvoHime");
        ExitCode::CoreUnavailable
    };
    std::process::exit(code as i32);
}
