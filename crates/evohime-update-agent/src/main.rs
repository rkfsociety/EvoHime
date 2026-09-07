use evohime_update_agent::{
    compare_semver, select_outdated, validate_component_manifest, ComponentManifest,
    InstalledManifest, ModuleRecord, UpdateCandidate, UpdaterStatus,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

#[cfg(windows)]
mod ui;

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
    let (updates, remote_error) = match remote_updates(&install_dir) {
        Ok(updates) => (updates, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    write_status(
        &install_dir,
        if remote_error.is_some() {
            "check-failed"
        } else {
            "ready"
        },
        remote_error.as_deref().unwrap_or(if updates.is_empty() {
            "Все модули актуальны."
        } else {
            "Доступны обновления модулей."
        }),
        updates.iter().map(|item| item.module.clone()).collect(),
    );
    #[cfg(windows)]
    if let Err(error) = ui::run_preflight_window(&install_dir, &updates, remote_error.as_deref()) {
        return fail(error);
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

const MODULE_IDS: &[&str] = &[
    "shell-host",
    "ui-bundle",
    "core",
    "supervisor",
    "cli",
    "analysis-worker",
    "listener",
    "listener-runtime",
    "transaction",
    "updater",
    "verifier",
];

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}
#[derive(serde::Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}
#[derive(serde::Deserialize)]
struct RemoteManifest {
    module: String,
    version: String,
}

fn remote_updates(install_dir: &Path) -> Result<Vec<UpdateCandidate>, String> {
    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(install_dir.join("update.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let repository = config
        .get("repositoryUrl")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("https://github.com/rkfsociety/EvoHime.git")
        .trim_end_matches(".git");
    let Some(repository) = repository.strip_prefix("https://github.com/") else {
        return Err("updater: разрешён только GitHub HTTPS repository".into());
    };
    let repository = repository.trim_end_matches('/');
    let client = reqwest::blocking::Client::builder()
        .user_agent("EvoHime-Updater")
        .build()
        .map_err(|error| error.to_string())?;
    let releases: Vec<Release> = client
        .get(format!(
            "https://api.github.com/repos/{repository}/releases?per_page=100"
        ))
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| error.to_string())?;
    let installed = read_installed_versions(install_dir);
    let mut updates = Vec::new();
    for module in MODULE_IDS {
        let prefix = format!("module-{module}-v");
        let Some(release) = releases
            .iter()
            .filter(|release| release.tag_name.starts_with(&prefix))
            .max_by(|left, right| {
                compare_semver(
                    &left.tag_name[prefix.len()..],
                    &right.tag_name[prefix.len()..],
                )
            })
        else {
            continue;
        };
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == format!("{module}.manifest.json"))
            .ok_or_else(|| format!("updater: manifest отсутствует для {module}"))?;
        let manifest: RemoteManifest = client
            .get(&asset.browser_download_url)
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json()
            .map_err(|error| error.to_string())?;
        if manifest.module != *module {
            return Err(format!("updater: manifest module mismatch for {module}"));
        }
        let current = installed
            .get(*module)
            .cloned()
            .unwrap_or_else(|| "0.0.0".into());
        if compare_semver(&current, &manifest.version).is_lt() {
            updates.push(UpdateCandidate {
                module: (*module).into(),
                installed: current,
                available: manifest.version,
            });
        }
    }
    Ok(updates)
}

fn read_installed_versions(install_dir: &Path) -> std::collections::HashMap<String, String> {
    let Ok(text) = fs::read_to_string(install_dir.join("evohime.components.json")) else {
        return std::collections::HashMap::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return std::collections::HashMap::new();
    };
    value
        .get("components")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some((
                item.get("id")?.as_str()?.to_owned(),
                item.get("version")?.as_str()?.to_owned(),
            ))
        })
        .collect()
}

fn write_status(install_dir: &Path, phase: &'static str, message: &str, modules: Vec<String>) {
    let state = install_dir.join("update-state");
    if fs::create_dir_all(&state).is_ok() {
        let status = UpdaterStatus {
            schema: "evohime.updater-status.v1",
            phase,
            message: message.to_owned(),
            modules,
        };
        let path = state.join("updater.json");
        if let Ok(value) = serde_json::to_vec_pretty(&status) {
            let _ = fs::write(path, value);
        }
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
