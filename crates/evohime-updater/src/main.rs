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
    let result = if arguments.iter().any(|arg| arg == "--apply-components") {
        parse_component_set_args(arguments).and_then(|options| {
            evohime_tx::apply_component_set_staged(evohime_tx::ComponentSetApply {
                staging: &options.staging,
                install_dir: &options.install_dir,
                state_dir: &options.state_dir,
                native_selected: &options.native_selected,
                ui_version: options.ui_version.as_deref(),
                wait_pid: options.wait_pid,
                relaunch: options.relaunch.as_deref(),
                health_file: options.health_file.as_deref(),
            })
            .map_err(|error| error.to_string())
        })
    } else if arguments.iter().any(|arg| arg == "--apply-ui") {
        let staging = required(arguments, "--staging").map_err(|error| error.to_string());
        let install_root = required(arguments, "--install-root").map_err(|error| error.to_string());
        let version = optional(arguments, "--ui-version")
            .ok_or_else(|| "missing argument: --ui-version".to_owned());
        let wait_pid = optional(arguments, "--wait-pid")
            .map(|value| {
                value
                    .parse::<u32>()
                    .map_err(|_| "--wait-pid must be a process id".to_owned())
            })
            .transpose();
        let relaunch = optional(arguments, "--relaunch").map(PathBuf::from);
        let health_file = optional(arguments, "--health-file").map(PathBuf::from);
        staging
            .and_then(|staging| install_root.map(|root| (staging, root)))
            .and_then(|(staging, root)| version.map(|version| (staging, root, version)))
            .and_then(|values| match wait_pid {
                Ok(pid) => Ok((values, pid)),
                Err(error) => Err(error),
            })
            .and_then(|((staging, root, version), wait_pid)| {
                evohime_tx::apply_ui_bundle_staged_with_restart(
                    &staging,
                    &root,
                    &version,
                    wait_pid,
                    relaunch.as_deref(),
                    health_file.as_deref(),
                )
                .map_err(|error| error.to_string())
            })
    } else if arguments.iter().any(|arg| arg == "--apply-staging") {
        parse_staged_args(arguments).and_then(|staged| {
            if let Some(selected) = staged.selected {
                return evohime_tx::apply_selected_staged(
                    &staged.staging,
                    &staged.install_dir,
                    &staged.state_dir,
                    &selected,
                )
                .map_err(|error| error.to_string());
            }
            evohime_tx::apply_staged(evohime_tx::StagedApply {
                staging: &staged.staging,
                install_dir: &staged.install_dir,
                state_dir: &staged.state_dir,
                wait_pid: staged.wait_pid,
                relaunch: staged.relaunch.as_deref(),
                health_file: staged.health_file.as_deref(),
            })
            .map_err(|error| error.to_string())
        })
    } else {
        parse_worker_args(arguments).and_then(|worker| {
            evohime_tx::run_update(
                &worker.installer,
                &worker.install_dir,
                &worker.state_dir,
                worker.wait_pid,
                worker.relaunch.as_deref(),
                worker.health_file.as_deref(),
            )
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

struct ComponentSetArgs {
    staging: PathBuf,
    install_dir: PathBuf,
    state_dir: PathBuf,
    native_selected: Vec<String>,
    ui_version: Option<String>,
    wait_pid: Option<u32>,
    relaunch: Option<PathBuf>,
    health_file: Option<PathBuf>,
}

fn parse_component_set_args(args: &[String]) -> Result<ComponentSetArgs, String> {
    let staging = required(args, "--staging")?;
    let install_dir = required(args, "--install-dir")?;
    let state_dir = optional(args, "--state-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_dir);
    let native_selected = optional(args, "--selected")
        .unwrap_or_default()
        .split(',')
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect();
    let wait_pid = optional(args, "--wait-pid")
        .map(|v| {
            v.parse::<u32>()
                .map_err(|_| "--wait-pid must be a process id".to_owned())
        })
        .transpose()?;
    Ok(ComponentSetArgs {
        staging,
        install_dir,
        state_dir,
        native_selected,
        ui_version: optional(args, "--ui-version"),
        wait_pid,
        relaunch: optional(args, "--relaunch").map(PathBuf::from),
        health_file: optional(args, "--health-file").map(PathBuf::from),
    })
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
    let mut command = Command::new(&worker);
    evohime_tx::configure_hidden_process(&mut command);
    let result = command.arg("--worker").args(args).spawn();
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
    health_file: Option<PathBuf>,
    selected: Option<Vec<String>>,
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
        health_file: optional(args, "--health-file").map(PathBuf::from),
        selected: optional(args, "--selected")
            .map(|value| value.split(',').map(str::to_owned).collect()),
    })
}

fn parse_worker_args(args: &[String]) -> Result<WorkerArgs, String> {
    let installer = required(args, "--installer")?;
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
    Ok(WorkerArgs {
        installer,
        install_dir,
        state_dir,
        wait_pid,
        relaunch: optional(args, "--relaunch").map(PathBuf::from),
        health_file: optional(args, "--health-file").map(PathBuf::from),
    })
}

struct WorkerArgs {
    installer: PathBuf,
    install_dir: PathBuf,
    state_dir: PathBuf,
    wait_pid: Option<u32>,
    relaunch: Option<PathBuf>,
    health_file: Option<PathBuf>,
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
