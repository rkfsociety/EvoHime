use evohime_update_agent::{
    select_outdated, validate_component_manifest, ComponentManifest, InstalledManifest,
    ModuleRecord,
};
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--launch") {
        return launch_shell(&args);
    }
    let Some(path) = args
        .windows(2)
        .find(|pair| pair[0] == "--manifest")
        .map(|pair| &pair[1])
    else {
        eprintln!("usage: evohime-updater --manifest <installed-manifest.json> --available <manifest.json>");
        return ExitCode::from(2);
    };
    let Some(available_path) = args
        .windows(2)
        .find(|pair| pair[0] == "--available")
        .map(|pair| &pair[1])
    else {
        return ExitCode::from(2);
    };
    let installed = match read::<InstalledManifest>(path) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let available = match read::<Vec<ModuleRecord>>(available_path) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    match select_outdated(&installed, &available) {
        Ok(plan) => {
            println!("{}", serde_json::to_string(&plan).expect("plan serializes"));
            ExitCode::SUCCESS
        }
        Err(error) => fail(error),
    }
}

/// The updater is the installed entry point. It remains independent from the
/// Electron shell: the preflight currently validates local manifests, then
/// starts the shell as a child. Applying a staged module can therefore replace
/// the shell without requiring the updater itself to be replaced in-process.
fn launch_shell(args: &[String]) -> ExitCode {
    let install_dir = args
        .windows(2)
        .find(|pair| pair[0] == "--install-dir")
        .map(|pair| PathBuf::from(&pair[1]))
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(PathBuf::from))
        });
    let Some(install_dir) = install_dir else {
        return fail("cannot determine install directory");
    };
    let manifest_path = install_dir.join("evohime.components.json");
    if manifest_path.is_file() {
        let manifest = match fs::read_to_string(&manifest_path)
            .map_err(|error| error.to_string())
            .and_then(|text| {
                serde_json::from_str::<ComponentManifest>(&text).map_err(|error| error.to_string())
            }) {
            Ok(value) => value,
            Err(error) => return fail(format!("invalid component manifest: {error}")),
        };
        if let Err(error) = validate_component_manifest(&manifest, &install_dir) {
            return fail(format!("installation integrity check failed: {error}"));
        }
    }
    let shell = install_dir.join("EvoHime.exe");
    if !shell.is_file() {
        return fail(format!("shell is missing: {}", shell.display()));
    }
    match Command::new(shell).current_dir(&install_dir).spawn() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
}
fn fail(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("updater: {error}");
    ExitCode::from(1)
}
