use evohime_update_agent::{
    compare_semver, select_outdated, validate_component_manifest, ComponentManifest,
    InstalledManifest, ModuleRecord, UpdateCandidate, UpdaterStatus,
};
use sha2::Digest;
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
    let data_dir = data_directory(&install_dir);
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
    let (updates, remote_error) = match remote_updates(&data_dir, &install_dir) {
        Ok(updates) => (updates, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    write_status(
        &data_dir,
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
    let action = match ui::run_preflight_window(&install_dir, &updates, remote_error.as_deref()) {
        Ok(action) => action,
        Err(error) => return fail(error),
    };
    #[cfg(windows)]
    if action == ui::UiAction::Update {
        if let Err(error) = apply_updates(&install_dir, &data_dir, &updates) {
            return fail(error);
        }
    }
    let shell = install_dir.join("EvoHime.exe");
    if !shell.is_file() {
        return fail(format!("shell is missing: {}", shell.display()));
    }
    let mut child = match Command::new(shell).current_dir(&install_dir).spawn() {
        Ok(child) => child,
        Err(error) => return fail(error),
    };
    #[cfg(windows)]
    monitor_running_shell(&mut child, &data_dir, &install_dir);
    let _ = child;
    ExitCode::SUCCESS
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
    artifact: String,
    size: u64,
    sha256: String,
}

fn data_directory(install_dir: &Path) -> PathBuf {
    if let Ok(value) = env::var("EVOHIME_DATA_DIR") {
        if !value.trim().is_empty() {
            return PathBuf::from(value);
        }
    }
    env::var("LOCALAPPDATA")
        .map(|value| PathBuf::from(value).join("EvoHime"))
        .unwrap_or_else(|_| install_dir.to_path_buf())
}

fn remote_updates(data_dir: &Path, install_dir: &Path) -> Result<Vec<UpdateCandidate>, String> {
    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(data_dir.join("update.json")).map_err(|error| error.to_string())?,
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
        if manifest.artifact.is_empty()
            || manifest.artifact.contains('/')
            || manifest.artifact.contains('\\')
            || manifest.size == 0
            || manifest.sha256.len() != 64
            || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!("updater: некорректный manifest для {module}"));
        }
        let current = installed
            .get(*module)
            .cloned()
            .unwrap_or_else(|| "0.0.0".into());
        if compare_semver(&current, &manifest.version).is_lt() {
            let artifact = manifest.artifact.clone();
            let download_url = release
                .assets
                .iter()
                .find(|asset| asset.name == artifact)
                .map(|asset| asset.browser_download_url.clone())
                .ok_or_else(|| format!("updater: artifact отсутствует для {module}"))?;
            updates.push(UpdateCandidate {
                module: (*module).into(),
                installed: current,
                available: manifest.version,
                artifact,
                size: manifest.size,
                sha256: manifest.sha256,
                download_url,
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

fn apply_updates(
    install_dir: &Path,
    data_dir: &Path,
    updates: &[UpdateCandidate],
) -> Result<(), String> {
    let staging = data_dir.join("update-staging");
    let state = data_dir.join("update-state");
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("EvoHime-Updater")
        .build()
        .map_err(|error| error.to_string())?;
    let mut selected = Vec::new();
    let mut applied = Vec::new();
    for update in updates {
        if update.module == "updater"
            || update.module == "ui-bundle"
            || update.module == "listener-runtime"
        {
            continue;
        }
        if update.artifact.contains('/')
            || update.artifact.contains('\\')
            || update.artifact.is_empty()
        {
            return Err(format!(
                "updater: небезопасный artifact для {}",
                update.module
            ));
        }
        let bytes = client
            .get(&update.download_url)
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .bytes()
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 != update.size {
            return Err(format!("updater: размер не совпал для {}", update.module));
        }
        let digest = sha2::Sha256::digest(&bytes);
        let actual = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if actual != update.sha256.to_ascii_lowercase() {
            return Err(format!("updater: SHA-256 не совпал для {}", update.module));
        }
        fs::write(staging.join(&update.artifact), &bytes).map_err(|error| error.to_string())?;
        selected.push(update.artifact.clone());
        applied.push(update);
    }
    if selected.is_empty() {
        return Ok(());
    }
    let manifest = serde_json::json!({
        "schema": "evohime.component-manifest.v1", "os": "windows", "architecture": "x64",
        "components": applied.iter().map(|item| serde_json::json!({
            "id": item.module, "version": item.available, "artifact": item.artifact,
            "path": item.artifact, "size": item.size, "sha256": item.sha256,
            "dependencies": [], "required": true, "restart": "module"
        })).collect::<Vec<_>>()
    });
    fs::write(
        staging.join("evohime.components.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let worker = install_dir.join("evohime-transaction.exe");
    if !worker.is_file() {
        return Err("updater: transaction worker отсутствует".into());
    }
    let staging_arg = staging.to_string_lossy().into_owned();
    let install_arg = install_dir.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let selected_arg = selected.join(",");
    let status = Command::new(worker)
        .args([
            "--apply-staging",
            "--staging",
            staging_arg.as_str(),
            "--install-dir",
            install_arg.as_str(),
            "--state-dir",
            state_arg.as_str(),
            "--selected",
            selected_arg.as_str(),
        ])
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!(
            "updater: transaction worker завершился с кодом {}",
            status.code().unwrap_or(-1)
        ));
    }
    merge_installed_manifest(install_dir, applied)
}

fn merge_installed_manifest(
    install_dir: &Path,
    applied: Vec<&UpdateCandidate>,
) -> Result<(), String> {
    let path = install_dir.join("evohime.components.json");
    let mut root = serde_json::from_str::<serde_json::Value>(
        &fs::read_to_string(&path).unwrap_or_else(|_| "{\"components\":[]}".into()),
    )
    .map_err(|error| error.to_string())?;
    let components = root
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| "updater: component manifest повреждён".to_owned())?;
    for update in applied {
        let value = serde_json::json!({"id": update.module, "version": update.available, "artifact": update.artifact, "path": update.artifact, "size": update.size, "sha256": update.sha256, "required": true, "restart": "module"});
        if let Some(existing) = components.iter_mut().find(|item| {
            item.get("id").and_then(serde_json::Value::as_str) == Some(update.module.as_str())
        }) {
            *existing = value;
        } else {
            components.push(value);
        }
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&root).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn monitor_running_shell(child: &mut std::process::Child, data_dir: &Path, install_dir: &Path) {
    while child.try_wait().ok().flatten().is_none() {
        std::thread::sleep(std::time::Duration::from_secs(30 * 60));
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        match remote_updates(data_dir, install_dir) {
            Ok(updates) => write_status(
                data_dir,
                if updates.is_empty() {
                    "ready"
                } else {
                    "available"
                },
                if updates.is_empty() {
                    "Все модули актуальны."
                } else {
                    "Доступны обновления модулей."
                },
                updates.iter().map(|item| item.module.clone()).collect(),
            ),
            Err(error) => write_status(data_dir, "check-failed", &error, Vec::new()),
        }
    }
}

fn write_status(data_dir: &Path, phase: &'static str, message: &str, modules: Vec<String>) {
    let state = data_dir.join("update-state");
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
