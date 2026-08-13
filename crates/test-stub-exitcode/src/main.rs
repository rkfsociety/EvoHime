use std::{env, process::ExitCode};

fn main() -> ExitCode {
    let code = env::args()
        .nth(1)
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(2);
    ExitCode::from(code)
}
