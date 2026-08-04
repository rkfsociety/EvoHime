use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|arg| arg == "--worker") {
        return spawn_worker(&args[1..]);
    }

    match parse_worker_args(&args[1..]).and_then(|(installer, install_dir, state_dir)| {
        evohime_tx::run_update(&installer, &install_dir, &state_dir)
            .map_err(|error| error.to_string())
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("EvoHime update failed: {error}");
            ExitCode::from(1)
        }
    }
}

fn spawn_worker(args: &[String]) -> ExitCode {
    let worker = std::env::temp_dir().join(format!(
        "evohime-transaction-{}-worker.exe",
        std::process::id()
    ));
    let current = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => return report_error(error),
    };
    if let Err(error) = std::fs::copy(&current, &worker) {
        return report_error(error);
    }
    let result = Command::new(&worker).arg("--worker").args(args).spawn();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&worker);
        return report_error(error);
    }
    ExitCode::SUCCESS
}

fn parse_worker_args(args: &[String]) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let value = |name: &str| {
        args.windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| PathBuf::from(&pair[1]))
            .ok_or_else(|| format!("missing argument: {name}"))
    };
    let installer = value("--installer")?;
    let install_dir = value("--install-dir")?;
    let state_dir = value("--state-dir").unwrap_or_else(|_| default_state_dir());
    Ok((installer, install_dir, state_dir))
}

fn default_state_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("EvoHime")
        .join("update-state")
}

fn report_error(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("EvoHime updater failed to start: {error}");
    ExitCode::from(1)
}
