use std::path::PathBuf;
use std::process::{Command, ExitCode};

/// Update worker.
///
/// It always re-executes itself from a temporary copy: it replaces the very
/// directory it lives in, so the original executable must not be running while
/// the swap happens.
///
/// Two modes exist. `--installer` runs a downloaded Inno Setup package, and
/// `--apply-staging` installs a package the shell rebuilt from source.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|arg| arg == "--worker") {
        return spawn_worker(&args[1..]);
    }

    let arguments = &args[1..];
    let result = if arguments.iter().any(|arg| arg == "--apply-staging") {
        parse_staged_args(arguments).and_then(|staged| {
            evohime_tx::apply_staged(evohime_tx::StagedApply {
                staging: &staged.staging,
                install_dir: &staged.install_dir,
                state_dir: &staged.state_dir,
                wait_pid: staged.wait_pid,
                relaunch: staged.relaunch.as_deref(),
            })
            .map_err(|error| error.to_string())
        })
    } else {
        parse_worker_args(arguments).and_then(|(installer, install_dir, state_dir)| {
            evohime_tx::run_update(&installer, &install_dir, &state_dir)
                .map_err(|error| error.to_string())
        })
    };

    match result {
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

struct StagedArgs {
    staging: PathBuf,
    install_dir: PathBuf,
    state_dir: PathBuf,
    wait_pid: Option<u32>,
    relaunch: Option<PathBuf>,
}

fn parse_staged_args(args: &[String]) -> Result<StagedArgs, String> {
    let staging = required(args, "--staging")?;
    let install_dir = required(args, "--install-dir")?;
    let state_dir = optional(args, "--state-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_dir);
    let wait_pid = match optional(args, "--wait-pid") {
        Some(value) => Some(
            value
                .parse::<u32>()
                .map_err(|_| "--wait-pid must be a process id".to_string())?,
        ),
        None => None,
    };
    Ok(StagedArgs {
        staging,
        install_dir,
        state_dir,
        wait_pid,
        relaunch: optional(args, "--relaunch").map(PathBuf::from),
    })
}

fn parse_worker_args(args: &[String]) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let installer = required(args, "--installer")?;
    let install_dir = required(args, "--install-dir")?;
    let state_dir = optional(args, "--state-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_dir);
    Ok((installer, install_dir, state_dir))
}

fn required(args: &[String], name: &str) -> Result<PathBuf, String> {
    optional(args, name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing argument: {name}"))
}

fn optional(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
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
