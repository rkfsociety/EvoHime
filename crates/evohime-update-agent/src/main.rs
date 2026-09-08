#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

use evohime_update_agent::{
    compare_semver, is_valid_semver, select_outdated, InstalledManifest, ModuleRecord,
    UpdateCandidate, UpdaterModuleStatus, UpdaterStatus,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::Digest;
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--launch") {
        return launch_shell(&args);
    }
    if args.iter().any(|arg| arg == "--check" || arg == "--apply") {
        return control_update(&args);
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

fn control_update(args: &[String]) -> ExitCode {
    let Some(install_dir) = args
        .windows(2)
        .find(|pair| pair[0] == "--install-dir")
        .map(|pair| PathBuf::from(&pair[1]))
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(PathBuf::from))
        })
    else {
        return fail("cannot determine install directory");
    };
    let data_dir = data_directory(&install_dir);
    let updates = match remote_updates(&data_dir, &install_dir) {
        Ok(updates) => updates,
        Err(error) => {
            write_status(&data_dir, "failed", &error, &[]);
            return fail(error);
        }
    };
    if args.iter().any(|arg| arg == "--check") {
        write_status(
            &data_dir,
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
            &updates,
        );
        return ExitCode::SUCCESS;
    }
    write_status(
        &data_dir,
        "applying",
        "Применяю выбранные модульные обновления…",
        &updates,
    );
    let result = apply_updates(&install_dir, &data_dir, &updates, &|message, percent| {
        write_status(
            &data_dir,
            "applying",
            &format!("{message} — {percent}%"),
            &updates,
        );
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            write_status(&data_dir, "failed", &error, &updates);
            fail(error)
        }
    }
}

/// Compatibility entry point for older shortcuts. The visible updater is a
/// separate Electron application; this Rust process only forwards the launch
/// request and remains a headless worker.
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
    let updater = install_dir.join("EvoHimeUpdater.exe");
    if !updater.is_file() {
        return fail(format!(
            "Electron updater is missing: {}",
            updater.display()
        ));
    }
    match Command::new(updater)
        .args(["--evohime-updater", "--install-dir"])
        .arg(&install_dir)
        .current_dir(&install_dir)
        .spawn()
    {
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
    artifact: String,
    size: u64,
    sha256: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    changes: Vec<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default = "default_restart")]
    restart: String,
}

fn default_restart() -> String {
    "module".to_owned()
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
    let config = read_update_config(&data_dir.join("update.json"))?;
    let repository = config
        .get("repositoryUrl")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("https://github.com/rkfsociety/EvoHime.git")
        .trim_end_matches(".git");
    let Some(repository) = repository.strip_prefix("https://github.com/") else {
        return Err("updater: разрешён только GitHub HTTPS repository".into());
    };
    let repository = repository.trim_end_matches('/');
    let client = updater_http_client()?;
    let releases: Vec<Release> = get_json(
        &client,
        &format!("https://api.github.com/repos/{repository}/releases?per_page=100"),
        "список GitHub Release",
    )?;
    let installed_manifest = read_installed_module_manifest(install_dir)?;
    let mut installed_manifest = installed_manifest;
    if let Some(runtime) = read_runtime_version(data_dir)? {
        installed_manifest.components.push(runtime);
    }
    let installed = installed_manifest
        .components
        .iter()
        .map(|item| (item.id.as_str(), item.version.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut available = Vec::new();
    let mut manifests = std::collections::HashMap::new();
    for module in MODULE_IDS {
        let prefix = format!("module-{module}-v");
        let Some(release) = releases
            .iter()
            .filter(|release| {
                release.tag_name.starts_with(&prefix)
                    && is_valid_semver(&release.tag_name[prefix.len()..])
            })
            .max_by(|left, right| {
                compare_semver(
                    &left.tag_name[prefix.len()..],
                    &right.tag_name[prefix.len()..],
                )
            })
        else {
            continue;
        };
        let manifest_asset_name = if *module == "listener-runtime" {
            "listener-runtime.json".to_owned()
        } else {
            format!("{module}.manifest.json")
        };
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == manifest_asset_name)
            .ok_or_else(|| format!("updater: manifest отсутствует для {module}"))?;
        let manifest = if *module == "listener-runtime" {
            let runtime: RuntimeReleaseManifest = get_json(
                &client,
                &asset.browser_download_url,
                &format!("manifest {module}"),
            )?;
            validate_runtime_manifest(&runtime)?;
            RemoteManifest {
                module: runtime.module,
                version: runtime.version,
                artifact: manifest_asset_name,
                size: 0,
                sha256: String::new(),
                summary: "Библиотеки распознавания речи и модели для listener.".to_owned(),
                changes: vec!["Обновлён проверенный комплект библиотек и моделей.".to_owned()],
                dependencies: Vec::new(),
                restart: "listener".to_owned(),
            }
        } else {
            get_json(
                &client,
                &asset.browser_download_url,
                &format!("manifest {module}"),
            )?
        };
        if manifest.module != *module {
            return Err(format!("updater: manifest module mismatch for {module}"));
        }
        if !is_valid_semver(&manifest.version) {
            return Err(format!(
                "updater: некорректная версия в manifest для {module}"
            ));
        }
        if (*module != "listener-runtime" && manifest.artifact.is_empty())
            || manifest.artifact.contains('/')
            || manifest.artifact.contains('\\')
            || (*module != "listener-runtime" && manifest.size == 0)
            || (*module != "listener-runtime"
                && (manifest.sha256.len() != 64
                    || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())))
        {
            return Err(format!("updater: некорректный manifest для {module}"));
        }
        let artifact = manifest.artifact.clone();
        let download_url = release
            .assets
            .iter()
            .find(|asset| asset.name == artifact)
            .map(|asset| asset.browser_download_url.clone())
            .ok_or_else(|| format!("updater: artifact отсутствует для {module}"))?;
        available.push(ModuleRecord {
            id: (*module).into(),
            version: manifest.version.clone(),
            dependencies: manifest.dependencies.clone(),
        });
        manifests.insert((*module).to_owned(), (manifest, download_url));
    }
    let plan = select_outdated(&installed_manifest, &available)?;
    let updates = plan
        .modules
        .into_iter()
        .filter_map(|module| {
            let (manifest, download_url) = manifests.remove(&module)?;
            let current = installed.get(module.as_str()).copied().unwrap_or("0.0.0");
            if compare_semver(current, &manifest.version).is_ge() {
                return None;
            }
            Some(UpdateCandidate {
                module,
                installed: current.to_owned(),
                available: manifest.version,
                summary: manifest.summary,
                changes: manifest.changes,
                dependencies: manifest.dependencies,
                restart: manifest.restart,
                artifact: manifest.artifact,
                size: manifest.size,
                sha256: manifest.sha256,
                download_url,
            })
        })
        .collect();
    Ok(updates)
}

/// The installer writes this local JSON file. Accept a UTF-8 BOM so clients
/// installed by older packages remain updateable; JSON itself does not permit
/// that marker before its first token.
fn read_update_config(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(&text))
        .map_err(|error| format!("updater: update.json содержит некорректный JSON: {error}"))
}

fn updater_http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("EvoHime-Updater")
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("updater: не удалось создать HTTP-клиент: {error}"))
}

fn get_json<T: DeserializeOwned>(
    client: &reqwest::blocking::Client,
    url: &str,
    purpose: &str,
) -> Result<T, String> {
    let mut last_error = String::new();
    for attempt in 1..=2 {
        match get_json_once(client, url, purpose) {
            Ok(value) => return Ok(value),
            Err(error) => last_error = error,
        }
        if attempt == 1 {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    Err(last_error)
}

fn get_json_once<T: DeserializeOwned>(
    client: &reqwest::blocking::Client,
    url: &str,
    purpose: &str,
) -> Result<T, String> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("updater: {purpose}: сетевой запрос не удался: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("updater: {purpose}: не удалось прочитать ответ: {error}"))?;
    if !status.is_success() {
        return Err(format!("updater: {purpose}: GitHub вернул HTTP {status}"));
    }
    parse_json_body(&body, purpose)
}

fn parse_json_body<T: DeserializeOwned>(body: &str, purpose: &str) -> Result<T, String> {
    let body = body.trim_start_matches('\u{feff}').trim();
    if body.is_empty() {
        return Err(format!(
            "updater: {purpose}: GitHub вернул пустой ответ вместо JSON"
        ));
    }
    serde_json::from_str(body)
        .map_err(|error| format!("updater: {purpose}: GitHub вернул некорректный JSON: {error}"))
}

fn read_installed_module_manifest(install_dir: &Path) -> Result<InstalledManifest, String> {
    let path = install_dir.join("evohime.components.json");
    if !path.is_file() {
        return Ok(InstalledManifest {
            components: Vec::new(),
        });
    }
    serde_json::from_str::<InstalledManifest>(
        &fs::read_to_string(path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("updater: component manifest повреждён: {error}"))
}

#[derive(Debug, Deserialize, Serialize)]
struct RuntimeReleaseManifest {
    schema: u32,
    module: String,
    version: String,
    abi: serde_json::Value,
    files: Vec<RuntimeReleaseEntry>,
    models: Vec<RuntimeReleaseEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RuntimeReleaseEntry {
    name: String,
    size: u64,
    sha256: String,
}

fn read_runtime_version(data_dir: &Path) -> Result<Option<ModuleRecord>, String> {
    let path = data_dir
        .join("tools")
        .join("listener")
        .join("listener-runtime.json");
    if !path.is_file() {
        return Ok(None);
    }
    let manifest = serde_json::from_str::<RuntimeReleaseManifest>(
        &fs::read_to_string(path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("updater: manifest listener-runtime повреждён: {error}"))?;
    validate_runtime_manifest(&manifest)?;
    Ok(Some(ModuleRecord {
        id: "listener-runtime".to_owned(),
        version: manifest.version,
        dependencies: Vec::new(),
    }))
}

fn validate_runtime_manifest(manifest: &RuntimeReleaseManifest) -> Result<(), String> {
    if manifest.schema != 1
        || manifest.module != "listener-runtime"
        || !is_valid_semver(&manifest.version)
        || manifest.files.is_empty()
        || manifest.models.is_empty()
    {
        return Err("updater: некорректный manifest listener-runtime".to_owned());
    }
    let mut total = 0u64;
    for entry in manifest.files.iter().chain(manifest.models.iter()) {
        if entry.name.is_empty()
            || entry.name.contains('\\')
            || entry.name.contains("..")
            || Path::new(&entry.name).is_absolute()
            || entry.size == 0
            || entry.sha256.len() != 64
            || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "updater: небезопасная запись runtime {}",
                entry.name
            ));
        }
        total = total
            .checked_add(entry.size)
            .ok_or_else(|| "updater: размер listener-runtime переполнен".to_owned())?;
    }
    if total > 4 * 1024 * 1024 * 1024 {
        return Err("updater: listener-runtime слишком велик".to_owned());
    }
    Ok(())
}

fn apply_updates(
    install_dir: &Path,
    data_dir: &Path,
    updates: &[UpdateCandidate],
    progress: &dyn Fn(&str, u8),
) -> Result<(), String> {
    let staging = data_dir.join("update-staging");
    let state = data_dir.join("update-state");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let client = updater_http_client()?;
    let mut selected = Vec::new();
    let mut applied = Vec::new();
    let mut ui_update = None;
    let updater_update = updates.iter().find(|update| update.module == "updater");
    let runtime_update = updates
        .iter()
        .find(|update| update.module == "listener-runtime");
    let native_updates = updates
        .iter()
        .filter(|update| {
            update.module != "updater"
                && update.module != "ui-bundle"
                && update.module != "listener-runtime"
        })
        .collect::<Vec<_>>();
    let total_bytes = native_updates.iter().map(|update| update.size).sum::<u64>();
    let mut completed_bytes = 0u64;
    for update in native_updates {
        if update.artifact.contains('/')
            || update.artifact.contains('\\')
            || update.artifact.is_empty()
        {
            return Err(format!(
                "updater: небезопасный artifact для {}",
                update.module
            ));
        }
        progress(
            &format!("Скачивание {}", update.module),
            progress_percent(completed_bytes, total_bytes),
        );
        download_verified_file(
            &client,
            update,
            &staging.join(&update.artifact),
            |downloaded| {
                progress(
                    &format!("Скачивание {}", update.module),
                    progress_percent(completed_bytes + downloaded, total_bytes),
                )
            },
        )?;
        completed_bytes += update.size;
        progress(
            &format!("Скачан {}", update.module),
            progress_percent(completed_bytes, total_bytes),
        );
        selected.push(update.artifact.clone());
        applied.push(update);
    }
    if let Some(update) = updates.iter().find(|update| update.module == "ui-bundle") {
        progress("Скачивание ui-bundle", 0);
        download_verified_file(
            &client,
            update,
            &staging.join("ui-bundle.zip"),
            |downloaded| {
                progress(
                    "Скачивание ui-bundle",
                    progress_percent(downloaded, update.size),
                )
            },
        )?;
        extract_ui_bundle(&staging.join("ui-bundle.zip"), &staging.join("ui-bundle"))?;
        ui_update = Some(update);
        progress("ui-bundle подготовлен", 100);
    }
    if let Some(update) = updater_update {
        progress("Скачивание updater", 0);
        download_verified_file(
            &client,
            update,
            &staging.join("evohime-updater.exe.next"),
            |downloaded| {
                progress(
                    "Скачивание updater",
                    progress_percent(downloaded, update.size),
                )
            },
        )?;
        progress("updater подготовлен", 100);
    }
    if selected.is_empty() && ui_update.is_none() {
        if let Some(update) = runtime_update {
            apply_listener_runtime(&client, update, data_dir, progress)?;
        }
        if let Some(update) = updater_update {
            let manifest_next = install_dir.join("evohime.components.json.next");
            let component_updates = vec![update];
            merge_installed_manifest_to(install_dir, &component_updates, &manifest_next)?;
            schedule_updater_replacement(install_dir, data_dir, &staging, &manifest_next, update)?;
        }
        write_status(data_dir, "ready", "Обновления модулей применены.", &[]);
        return Ok(());
    }
    let manifest = serde_json::json!({
        "schema": "evohime.component-manifest.v1", "os": "windows", "architecture": "x64",
        "components": applied.iter().chain(ui_update.iter()).map(|item| serde_json::json!({
            "id": item.module, "version": item.available, "artifact": item.artifact,
            "path": item.artifact, "size": item.size, "sha256": item.sha256,
            "dependencies": item.dependencies, "required": true, "restart": item.restart
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
    let worker_copy = state.join(format!("transaction-{}-worker.exe", std::process::id()));
    fs::create_dir_all(&state).map_err(|error| error.to_string())?;
    fs::copy(&worker, &worker_copy).map_err(|error| error.to_string())?;
    let mode = if ui_update.is_some() {
        "--apply-components"
    } else {
        "--apply-staging"
    };
    let mut command = Command::new(&worker_copy);
    command.args([
        "--worker",
        mode,
        "--staging",
        staging_arg.as_str(),
        "--install-dir",
        install_arg.as_str(),
        "--state-dir",
        state_arg.as_str(),
    ]);
    if !selected.is_empty() {
        command.args(["--selected", selected_arg.as_str()]);
    }
    if let Some(update) = ui_update {
        command.args(["--ui-version", update.available.as_str()]);
    }
    progress("Применение модулей", 0);
    let status = command.status().map_err(|error| error.to_string())?;
    let _ = fs::remove_file(&worker_copy);
    if !status.success() {
        return Err(format!(
            "updater: transaction worker завершился с кодом {}",
            status.code().unwrap_or(-1)
        ));
    }
    progress("Модули применены", 100);
    let mut component_updates = applied.into_iter().chain(ui_update).collect::<Vec<_>>();
    if let Some(update) = updater_update {
        let manifest_next = install_dir.join("evohime.components.json.next");
        component_updates.push(update);
        merge_installed_manifest_to(install_dir, &component_updates, &manifest_next)?;
        apply_listener_runtime_if_needed(&client, runtime_update, data_dir, progress)?;
        schedule_updater_replacement(install_dir, data_dir, &staging, &manifest_next, update)?;
        write_status(data_dir, "ready", "Обновления модулей применены.", &[]);
        return Ok(());
    }
    merge_installed_manifest_to(
        install_dir,
        &component_updates,
        &install_dir.join("evohime.components.json"),
    )?;
    apply_listener_runtime_if_needed(&client, runtime_update, data_dir, progress)?;
    write_status(data_dir, "ready", "Обновления модулей применены.", &[]);
    Ok(())
}

fn apply_listener_runtime_if_needed(
    client: &reqwest::blocking::Client,
    update: Option<&UpdateCandidate>,
    data_dir: &Path,
    progress: &dyn Fn(&str, u8),
) -> Result<(), String> {
    if let Some(update) = update {
        apply_listener_runtime(client, update, data_dir, progress)?;
    }
    Ok(())
}

fn progress_percent(done: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    ((done.saturating_mul(100) / total).min(99)) as u8
}

fn download_verified_file(
    client: &reqwest::blocking::Client,
    update: &UpdateCandidate,
    target: &Path,
    on_progress: impl Fn(u64),
) -> Result<(), String> {
    let temporary = target.with_extension("part");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let result = (|| {
        let mut response = client
            .get(&update.download_url)
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let mut file = fs::File::create(&temporary).map_err(|error| error.to_string())?;
        let mut digest = sha2::Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let count = response
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .ok_or_else(|| format!("updater: размер переполнен для {}", update.module))?;
            if total > update.size {
                return Err(format!(
                    "updater: размер больше манифеста для {}",
                    update.module
                ));
            }
            file.write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
            digest.update(&buffer[..count]);
            on_progress(total);
        }
        if total != update.size {
            return Err(format!("updater: размер не совпал для {}", update.module));
        }
        let actual = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if actual != update.sha256.to_ascii_lowercase() {
            return Err(format!("updater: SHA-256 не совпал для {}", update.module));
        }
        fs::rename(&temporary, target).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn extract_ui_bundle(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let archive_file = fs::File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(archive_file).map_err(|error| error.to_string())?;
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if entry.is_symlink() {
            return Err("updater: UI archive содержит symlink".to_owned());
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| "updater: UI archive содержит небезопасный путь".to_owned())?;
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = fs::File::create(&target).map_err(|error| error.to_string())?;
            std::io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
        }
    }
    if !destination.join("index.html").is_file() {
        return Err("updater: UI archive не содержит index.html".to_owned());
    }
    Ok(())
}

fn apply_listener_runtime(
    client: &reqwest::blocking::Client,
    update: &UpdateCandidate,
    data_dir: &Path,
    progress: &dyn Fn(&str, u8),
) -> Result<(), String> {
    let runtime: RuntimeReleaseManifest =
        get_json(client, &update.download_url, "manifest listener-runtime")?;
    validate_runtime_manifest(&runtime)?;
    let entries = runtime
        .files
        .iter()
        .chain(runtime.models.iter())
        .collect::<Vec<_>>();
    let total = entries.iter().map(|entry| entry.size).sum::<u64>();
    let mut completed = 0u64;
    let staging = data_dir.join("update-staging").join("listener-runtime");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let base_url = update
        .download_url
        .rsplit_once('/')
        .map(|(base, _)| base.to_owned())
        .ok_or_else(|| "updater: некорректный URL listener-runtime".to_owned())?;
    for entry in entries {
        let target = safe_runtime_path(&staging, &entry.name)?;
        let artifact_name = Path::new(&entry.name)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "updater: некорректное имя файла listener-runtime".to_owned())?;
        let file_update = UpdateCandidate {
            module: "listener-runtime".to_owned(),
            installed: update.installed.clone(),
            available: update.available.clone(),
            summary: update.summary.clone(),
            changes: update.changes.clone(),
            dependencies: update.dependencies.clone(),
            restart: update.restart.clone(),
            artifact: artifact_name.to_owned(),
            size: entry.size,
            sha256: entry.sha256.clone(),
            download_url: format!("{base_url}/{artifact_name}"),
        };
        progress(
            &format!("Скачивание listener-runtime: {}", entry.name),
            progress_percent(completed, total),
        );
        download_verified_file(client, &file_update, &target, |downloaded| {
            progress(
                &format!("Скачивание listener-runtime: {}", entry.name),
                progress_percent(completed + downloaded, total),
            )
        })?;
        completed += entry.size;
    }
    fs::write(
        staging.join("listener-runtime.json"),
        serde_json::to_vec_pretty(&runtime).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let tools_root = data_dir.join("tools");
    fs::create_dir_all(&tools_root).map_err(|error| error.to_string())?;
    let target = tools_root.join("listener");
    let backup = tools_root.join(format!("listener.rollback-{}", std::process::id()));
    let had_old = target.exists();
    if had_old {
        if backup.exists() {
            fs::remove_dir_all(&backup).map_err(|error| error.to_string())?;
        }
        fs::rename(&target, &backup).map_err(|error| error.to_string())?;
    }
    let result = fs::rename(&staging, &target).map_err(|error| error.to_string());
    if let Err(error) = result {
        if had_old {
            let _ = fs::rename(&backup, &target);
        }
        return Err(format!(
            "updater: не удалось активировать listener-runtime: {error}"
        ));
    }
    if had_old {
        fs::remove_dir_all(&backup).map_err(|error| error.to_string())?;
    }
    progress("listener-runtime применён", 100);
    Ok(())
}

fn safe_runtime_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "updater: небезопасный путь listener-runtime: {relative}"
        ));
    }
    Ok(root.join(path))
}

fn merge_installed_manifest_to(
    install_dir: &Path,
    applied: &[&UpdateCandidate],
    destination: &Path,
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
    let temporary = destination.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&root).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(temporary, destination).map_err(|error| error.to_string())
}

fn schedule_updater_replacement(
    install_dir: &Path,
    data_dir: &Path,
    staging: &Path,
    manifest_next: &Path,
    update: &UpdateCandidate,
) -> Result<(), String> {
    let state_dir = data_dir.join("update-state");
    fs::create_dir_all(&state_dir).map_err(|error| error.to_string())?;
    let script = state_dir.join(format!("updater-bootstrap-{}.cmd", std::process::id()));
    let marker = state_dir.join("updater-relaunch.pending");
    let updater = install_dir.join("evohime-updater.exe");
    let updater_ui = install_dir.join("EvoHimeUpdater.exe");
    let staged = staging.join("evohime-updater.exe.next");
    let backup = state_dir.join("updater-previous.exe");
    let manifest_backup = state_dir.join("components-previous.json");
    let quote = |path: &Path| format!("\"{}\"", path.display());
    let content = format!(
        "@echo off\r\nsetlocal\r\n:wait\r\ntasklist /FI \"PID eq {pid}\" 2>NUL | findstr /C:\"{pid}\" >NUL\r\nif not errorlevel 1 (timeout /t 1 /nobreak >NUL & goto wait)\r\ncopy /Y {updater} {backup} >NUL\r\nmove /Y {staged} {updater} >NUL\r\nif errorlevel 1 (move /Y {backup} {updater} >NUL & exit /b 1)\r\ncopy /Y {manifest} {manifest_backup} >NUL\r\nmove /Y {manifest_next} {manifest} >NUL\r\nif errorlevel 1 (move /Y {backup} {updater} >NUL & move /Y {manifest_backup} {manifest} >NUL & exit /b 1)\r\ndel /Q {backup} 2>NUL\r\ndel /Q {manifest_backup} 2>NUL\r\ndel /Q {marker} 2>NUL\r\nstart \"\" {updater_ui} --evohime-updater --install-dir {install_dir}\r\ndel /Q \"%~f0\" 2>NUL\r\n",
        pid = std::process::id(),
        updater = quote(&updater),
        updater_ui = quote(&updater_ui),
        staged = quote(&staged),
        backup = quote(&backup),
        manifest = quote(&install_dir.join("evohime.components.json")),
        manifest_next = quote(manifest_next),
        manifest_backup = quote(&manifest_backup),
        marker = quote(&marker),
        install_dir = quote(install_dir),
    );
    fs::write(&script, content).map_err(|error| error.to_string())?;
    fs::write(&marker, format!("{}\n", update.available)).map_err(|error| error.to_string())?;
    Command::new("cmd.exe")
        .args(["/D", "/C", script.to_string_lossy().as_ref()])
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn write_status(data_dir: &Path, phase: &'static str, message: &str, updates: &[UpdateCandidate]) {
    let state = data_dir.join("update-state");
    if fs::create_dir_all(&state).is_ok() {
        let status = UpdaterStatus {
            schema: "evohime.updater-status.v1",
            phase,
            message: message.to_owned(),
            modules: updates.iter().map(|item| item.module.clone()).collect(),
            available: updates
                .iter()
                .map(|item| UpdaterModuleStatus {
                    module: item.module.clone(),
                    installed: item.installed.clone(),
                    available: item.available.clone(),
                    summary: item.summary.clone(),
                    changes: item.changes.clone(),
                })
                .collect(),
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

#[cfg(test)]
mod tests {
    use super::{parse_json_body, read_update_config};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn json_response_reports_an_empty_body_without_a_raw_parser_error() {
        let error = parse_json_body::<serde_json::Value>("  \n", "manifest core").unwrap_err();
        assert_eq!(
            error,
            "updater: manifest core: GitHub вернул пустой ответ вместо JSON"
        );
    }

    #[test]
    fn json_response_reports_invalid_json_with_its_purpose() {
        let error =
            parse_json_body::<serde_json::Value>("<html>", "список GitHub Release").unwrap_err();
        assert!(
            error.starts_with("updater: список GitHub Release: GitHub вернул некорректный JSON:")
        );
    }

    #[test]
    fn update_config_accepts_the_bom_written_by_older_installers() {
        let path = std::env::temp_dir().join(format!(
            "evohime-update-config-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::write(&path, b"\xef\xbb\xbf{\"enabled\":true}").expect("write update config");

        let config = read_update_config(&path).expect("read BOM-prefixed update config");
        let _ = fs::remove_file(&path);

        assert_eq!(config["enabled"], true);
    }
}
