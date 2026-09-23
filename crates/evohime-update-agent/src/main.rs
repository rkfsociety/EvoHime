//! Windows update agent that validates and applies staged EvoHime packages.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

use evohime_update_agent::{
    compare_semver, deserialize_nullable_vec, is_valid_semver, read_recovery_journal,
    select_outdated, validate_pe_artifact, validate_pe_image, write_recovery_journal,
    InstalledManifest, ModuleRecord, UpdateCandidate, UpdaterModuleStatus, UpdaterStatus,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::Digest;
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};

#[path = "update_agent_artifacts.rs"]
mod artifacts;
use artifacts::*;

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--self-test") {
        return self_test(&args);
    }
    if args.iter().any(|arg| arg == "--launch") {
        return launch_shell(&args);
    }
    if args.iter().any(|arg| arg == "--check" || arg == "--apply") {
        return control_update(&args);
    }
    let Some(path) = argument_value(&args, "--manifest") else {
        eprintln!("usage: evohime-updater --manifest <installed-manifest.json> --available <manifest.json>");
        return ExitCode::from(2);
    };
    let Some(available_path) = argument_value(&args, "--available") else {
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
        Ok(plan) => match serde_json::to_string(&plan) {
            Ok(serialized) => {
                println!("{serialized}");
                ExitCode::SUCCESS
            }
            Err(error) => fail(format!("could not serialize update plan: {error}")),
        },
        Err(error) => fail(error),
    }
}

fn control_update(args: &[String]) -> ExitCode {
    let Some(install_dir) = argument_value(args, "--install-dir")
        .map(PathBuf::from)
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(PathBuf::from))
        })
    else {
        return fail("cannot determine install directory");
    };
    let data_dir = data_directory(&install_dir);
    let wait_pid = match optional_process_id(args, "--wait-pid") {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let relaunch = argument_value(args, "--relaunch").map(PathBuf::from);
    let health_file = argument_value(args, "--health-file").map(PathBuf::from);
    let _ = ensure_fallback(&install_dir, &data_dir);
    write_recovery_phase(&data_dir, "prepared", None);
    let updates = match remote_updates(&data_dir, &install_dir) {
        Ok(updates) => updates,
        Err(error) => {
            write_recovery_phase(&data_dir, "manual-recovery", Some("manifest-or-network"));
            write_status(&data_dir, "failed", &error, &[]);
            return fail(error);
        }
    };
    if args.iter().any(|arg| arg == "--check") {
        write_recovery_phase(&data_dir, "verified", None);
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
    write_recovery_phase(&data_dir, "downloaded", None);
    let result = apply_updates(
        &install_dir,
        &data_dir,
        &updates,
        wait_pid,
        relaunch.as_deref(),
        health_file.as_deref(),
        &|message, percent| {
            write_status(
                &data_dir,
                "applying",
                &format!("{message} — {percent}%"),
                &updates,
            );
        },
    );
    match result {
        Ok(()) => {
            write_recovery_phase(&data_dir, "committed", None);
            ExitCode::SUCCESS
        }
        Err(error) => {
            write_recovery_phase(&data_dir, "rolled-back", Some("apply-failed"));
            write_status(&data_dir, "failed", &error, &updates);
            fail(error)
        }
    }
}

fn write_recovery_phase(data_dir: &Path, phase: &str, reason: Option<&str>) {
    let path = data_dir.join("update-state").join("recovery.json");
    let mut journal = read_recovery_journal(&path)
        .ok()
        .flatten()
        .unwrap_or_else(|| {
            evohime_update_agent::RecoveryJournal::new(
                format!("update-{}", std::process::id()),
                phase,
            )
        });
    journal.phase = phase.to_owned();
    journal.fallback_available = data_dir
        .join("update-state")
        .join("updater-fallback.exe")
        .is_file();
    journal.reason_code = reason.map(str::to_owned);
    if reason.is_some() {
        journal.retry_count = journal
            .retry_count
            .saturating_add(1)
            .min(evohime_update_agent::MAX_RECOVERY_ATTEMPTS);
    }
    let _ = write_recovery_journal(&path, &journal);
}

/// Compatibility entry point for older shortcuts. The visible updater is a
/// standalone Electron package in the updater module; this Rust process only forwards the launch
/// request and remains a headless worker.
fn launch_shell(args: &[String]) -> ExitCode {
    let install_dir = argument_value(args, "--install-dir")
        .map(PathBuf::from)
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(PathBuf::from))
        });
    let Some(install_dir) = install_dir else {
        return fail("cannot determine install directory");
    };
    let data_dir = data_directory(&install_dir);
    if let Err(error) = launch_preflight(&install_dir, &data_dir) {
        let fallback = data_dir.join("update-state").join("updater-fallback.exe");
        if fallback.is_file() && fallback != env::current_exe().unwrap_or_default() {
            let _ = Command::new(fallback)
                .args(["--launch", "--install-dir"])
                .arg(&install_dir)
                .spawn();
            return ExitCode::SUCCESS;
        }
        write_status(
            &data_dir,
            "manual-recovery",
            &format!("Автоматическое восстановление остановлено: {error}"),
            &[],
        );
        return fail("updater recovery requires manual action");
    }
    let updater = updater_ui_executable(&install_dir);
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

fn updater_ui_executable(install_dir: &Path) -> PathBuf {
    let packaged = install_dir.join("updater").join("EvoHimeUpdater.exe");
    if packaged.is_file() {
        packaged
    } else {
        // Clients installed before the module split keep the legacy entrypoint
        // at the installation root until the first updater package succeeds.
        install_dir.join("EvoHimeUpdater.exe")
    }
}

fn self_test(args: &[String]) -> ExitCode {
    let Some(install_dir) = argument_value(args, "--install-dir")
        .map(PathBuf::from)
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from))
        })
    else {
        return fail("cannot determine install directory");
    };
    match self_test_inner(&install_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

fn self_test_inner(install_dir: &Path) -> Result<(), String> {
    if !install_dir.is_absolute() || !install_dir.is_dir() {
        return Err("invalid install directory".into());
    }
    let current = env::current_exe().map_err(|e| e.to_string())?;
    let metadata = fs::metadata(&current).map_err(|e| e.to_string())?;
    let mut file = fs::File::open(&current).map_err(|e| e.to_string())?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    validate_pe_artifact(
        &current,
        metadata.len(),
        &format!("{:x}", digest.finalize()),
    )?;
    let data = data_directory(install_dir);
    fs::create_dir_all(data.join("update-state")).map_err(|e| e.to_string())?;
    if let Some(journal) = read_recovery_journal(&data.join("update-state").join("recovery.json"))?
    {
        if journal.phase == "manual-recovery" {
            return Err("recovery journal requires manual action".into());
        }
    }
    Ok(())
}

fn launch_preflight(install_dir: &Path, data_dir: &Path) -> Result<(), String> {
    let state = data_dir.join("update-state");
    fs::create_dir_all(&state).map_err(|e| e.to_string())?;
    ensure_fallback(install_dir, data_dir)?;
    let current = env::current_exe().map_err(|e| e.to_string())?;
    let current_hash = hash_file(&current)?;
    let fallback = state.join("updater-fallback.exe");
    let fallback_metadata = fs::metadata(&fallback).map_err(|e| e.to_string())?;
    validate_pe_artifact(&fallback, fallback_metadata.len(), &hash_file(&fallback)?)?;
    let journal_path = state.join("recovery.json");
    if let Some(journal) = read_recovery_journal(&journal_path)? {
        if journal.phase == "replaced" || journal.phase == "self-tested" {
            let mut committed = journal;
            committed.phase = "committed".into();
            committed.active_sha256 = current_hash.clone();
            committed.fallback_available = true;
            write_recovery_journal(&journal_path, &committed)?;
        }
    }
    let updater_ui = updater_ui_executable(install_dir);
    if !updater_ui.is_file()
        || !updater_ui
            .parent()
            .map(|parent| parent.join("resources").join("app.asar").is_file())
            .unwrap_or(false)
    {
        return Err(format!(
            "Electron updater UI is missing or incomplete: {}",
            updater_ui.display()
        ));
    }
    self_test_inner(install_dir)?;
    let mut tested = read_recovery_journal(&journal_path)?.unwrap_or_else(|| {
        evohime_update_agent::RecoveryJournal::new(
            format!("launch-{}", std::process::id()),
            "self-tested",
        )
    });
    tested.phase = "self-tested".into();
    tested.active_sha256 = current_hash;
    tested.fallback_available = true;
    write_recovery_journal(&journal_path, &tested)
}

fn ensure_fallback(install_dir: &Path, data_dir: &Path) -> Result<(), String> {
    let current = env::current_exe().map_err(|e| e.to_string())?;
    let fallback = data_dir.join("update-state").join("updater-fallback.exe");
    if fallback.is_file() {
        let metadata = fs::metadata(&fallback).map_err(|e| e.to_string())?;
        if validate_pe_artifact(&fallback, metadata.len(), &hash_file(&fallback)?).is_ok() {
            return Ok(());
        }
        fs::remove_file(&fallback).map_err(|e| e.to_string())?;
    }
    if !install_dir.is_absolute() {
        return Err("install directory must be absolute".into());
    }
    let fallback_parent = fallback
        .parent()
        .ok_or_else(|| "updater fallback path has no parent".to_owned())?;
    fs::create_dir_all(fallback_parent).map_err(|e| e.to_string())?;
    let temporary = fallback.with_extension("exe.part");
    fs::copy(&current, &temporary).map_err(|e| e.to_string())?;
    fs::rename(&temporary, &fallback).map_err(|e| e.to_string())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
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
const MAX_JSON_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_LOCAL_JSON_BYTES: usize = 256 * 1024;
const MAX_COMPATIBLE_SUMMARY_BYTES: usize = 16 * 1024;
const MAX_COMPATIBLE_CHANGES: usize = 64;
const MAX_COMPATIBLE_CHANGE_BYTES: usize = 8 * 1024;
const MAX_COMPATIBLE_DEPENDENCY_BYTES: usize = 64;
const MAX_UPDATE_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024;

fn argument_value<'a>(args: &'a [String], name: &str) -> Option<&'a String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .filter(|pair| !pair[1].starts_with("--"))
        .map(|pair| &pair[1])
}

fn optional_process_id(args: &[String], name: &str) -> Result<Option<u32>, String> {
    argument_value(args, name)
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|_| format!("{name} must be a process id"))
        })
        .transpose()
}

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
struct CompatibleManifest {
    schema: String,
    product: String,
    os: String,
    architecture: String,
    updater: UpdaterRequirement,
    components: Vec<CompatibleComponent>,
}

#[derive(serde::Deserialize)]
struct UpdaterRequirement {
    minimum_version: String,
    update_first: bool,
}

#[derive(serde::Deserialize)]
struct CompatibleComponent {
    id: String,
    version: String,
    release_tag: String,
    manifest_asset: String,
    artifact: Option<String>,
    size: u64,
    sha256: String,
    #[serde(default, deserialize_with = "deserialize_nullable_vec")]
    dependencies: Vec<String>,
    restart: String,
    #[serde(default)]
    protocol: String,
    #[serde(default)]
    summary: String,
    #[serde(default, deserialize_with = "deserialize_nullable_vec")]
    changes: Vec<String>,
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

fn read_compatible_manifest(
    client: &UpdaterHttpClient,
    releases: &[Release],
) -> Result<CompatibleManifest, String> {
    let release = releases
        .iter()
        .find(|release| release.tag_name == "compatibility")
        .ok_or_else(|| "updater: release совместимого комплекта отсутствует".to_owned())?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == "evohime.compatible.json")
        .ok_or_else(|| "updater: манифест совместимого комплекта отсутствует".to_owned())?;
    if !is_github_release_asset_url(&asset.browser_download_url) {
        return Err("updater: URL compatible manifest не является GitHub release asset".into());
    }
    get_json(
        client,
        &asset.browser_download_url,
        "манифест совместимого комплекта",
    )
}

fn validate_compatible_manifest(manifest: &CompatibleManifest) -> Result<(), String> {
    if manifest.schema != "evohime.compatible-set.v1"
        || manifest.product != "EvoHime"
        || manifest.os != "windows"
        || manifest.architecture != "x64"
        || !is_valid_semver(&manifest.updater.minimum_version)
        || manifest.components.is_empty()
        || manifest.components.len() != MODULE_IDS.len()
    {
        return Err("updater: некорректный манифест совместимого комплекта".to_owned());
    }
    let mut ids = std::collections::HashSet::new();
    for component in &manifest.components {
        if !MODULE_IDS.contains(&component.id.as_str()) || !ids.insert(component.id.as_str()) {
            return Err(format!(
                "updater: некорректный компонент совместимого комплекта: {}",
                component.id
            ));
        }
        if component.summary.len() > MAX_COMPATIBLE_SUMMARY_BYTES
            || component.changes.len() > MAX_COMPATIBLE_CHANGES
            || component
                .changes
                .iter()
                .any(|change| change.len() > MAX_COMPATIBLE_CHANGE_BYTES)
            || component.dependencies.len() > MODULE_IDS.len()
            || component
                .dependencies
                .iter()
                .any(|dependency| dependency.len() > MAX_COMPATIBLE_DEPENDENCY_BYTES)
        {
            return Err(format!(
                "updater: метаданные совместимого компонента превышают лимит: {}",
                component.id
            ));
        }
        let mut dependencies =
            std::collections::HashSet::with_capacity(component.dependencies.len());
        if component
            .dependencies
            .iter()
            .any(|dependency| dependency == &component.id || !dependencies.insert(dependency))
        {
            return Err(format!(
                "updater: повторная или циклическая зависимость компонента: {}",
                component.id
            ));
        }
        let prefix = format!("module-{}-v", component.id);
        if !component.release_tag.starts_with(&prefix)
            || !is_valid_semver(&component.release_tag[prefix.len()..])
            || !is_valid_semver(&component.version)
            || compare_semver(&component.version, &component.release_tag[prefix.len()..])
                != std::cmp::Ordering::Equal
            || component.manifest_asset.is_empty()
            || component.manifest_asset.contains('/')
            || component.manifest_asset.contains('\\')
            || component.manifest_asset.contains(':')
            || component.manifest_asset.contains("..")
        {
            return Err(format!(
                "updater: некорректная запись совместимого компонента: {}",
                component.id
            ));
        }
        if component.id == "listener-runtime" {
            if component.artifact.is_some() || component.size != 0 || !component.sha256.is_empty() {
                return Err("updater: listener-runtime не должен иметь бинарный hash".to_owned());
            }
        } else if component.artifact.as_deref().is_none_or(|artifact| {
            artifact.is_empty()
                || artifact.contains('/')
                || artifact.contains('\\')
                || artifact.contains(':')
                || artifact.contains("..")
        }) || component.size == 0
            || component.size > MAX_UPDATE_ARTIFACT_BYTES
            || component.sha256.len() != 64
            || !component
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "updater: некорректный artifact или hash для {}",
                component.id
            ));
        }
    }
    for component in &manifest.components {
        for dependency in &component.dependencies {
            if !ids.contains(dependency.as_str()) {
                return Err(format!(
                    "updater: зависимость {} отсутствует в совместимом комплекте",
                    dependency
                ));
            }
        }
    }
    let mut unresolved = ids.clone();
    while !unresolved.is_empty() {
        let resolved = manifest
            .components
            .iter()
            .filter(|component| {
                unresolved.contains(component.id.as_str())
                    && component
                        .dependencies
                        .iter()
                        .all(|dependency| !unresolved.contains(dependency.as_str()))
            })
            .map(|component| component.id.as_str())
            .collect::<Vec<_>>();
        if resolved.is_empty() {
            return Err("updater: цикл зависимостей в совместимом комплекте".to_owned());
        }
        for id in resolved {
            unresolved.remove(id);
        }
    }
    let updater = manifest
        .components
        .iter()
        .find(|component| component.id == "updater")
        .ok_or_else(|| "updater: совместимый комплект не содержит updater".to_owned())?;
    if compare_semver(&updater.version, &manifest.updater.minimum_version).is_lt() {
        return Err("updater: версия updater ниже требования совместимого комплекта".to_owned());
    }
    Ok(())
}

fn updater_first_if_required(
    updates: Vec<UpdateCandidate>,
    installed_version: &str,
    minimum_version: &str,
    update_first: bool,
) -> Result<Vec<UpdateCandidate>, String> {
    if !update_first || !compare_semver(installed_version, minimum_version).is_lt() {
        return Ok(updates);
    }
    updates
        .into_iter()
        .find(|update| update.module == "updater")
        .map(|update| vec![update])
        .ok_or_else(|| {
            "updater: compatible set не содержит доступного обновления updater".to_owned()
        })
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
    let client = updater_http_client(resolve_github_token(&config))?;
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
    let compatibility = read_compatible_manifest(&client, &releases)?;
    validate_compatible_manifest(&compatibility)?;
    let installed_updater = installed.get("updater").copied().unwrap_or("0.0.0");
    let compatible_updater = compatibility
        .components
        .iter()
        .find(|component| component.id == "updater")
        .ok_or_else(|| "updater: compatible manifest has no updater component".to_owned())?;
    if compare_semver(installed_updater, &compatibility.updater.minimum_version).is_lt()
        && compare_semver(installed_updater, &compatible_updater.version).is_ge()
    {
        let suffix = if compatibility.updater.update_first {
            " (updater должен обновляться первым)"
        } else {
            ""
        };
        return Err(format!(
            "updater: совместимый комплект требует обновления updater{suffix}"
        ));
    }
    let mut available = Vec::new();
    let mut manifests = std::collections::HashMap::new();
    for component in compatibility.components {
        let release = releases
            .iter()
            .find(|release| release.tag_name == component.release_tag)
            .ok_or_else(|| format!("updater: release отсутствует для {}", component.id))?;
        let manifest_asset = release
            .assets
            .iter()
            .find(|asset| asset.name == component.manifest_asset)
            .ok_or_else(|| format!("updater: manifest отсутствует для {}", component.id))?;
        let (artifact, download_url) = if component.id == "listener-runtime" {
            (
                component.manifest_asset.clone(),
                manifest_asset.browser_download_url.clone(),
            )
        } else {
            let artifact = component
                .artifact
                .clone()
                .ok_or_else(|| format!("updater: artifact отсутствует для {}", component.id))?;
            let download_url = release
                .assets
                .iter()
                .find(|asset| asset.name == artifact)
                .map(|asset| asset.browser_download_url.clone())
                .ok_or_else(|| format!("updater: artifact отсутствует для {}", component.id))?;
            (artifact, download_url)
        };
        if !is_github_release_asset_url(&download_url) {
            return Err(format!(
                "updater: URL release asset недопустим для {}",
                component.id
            ));
        }
        available.push(ModuleRecord {
            id: component.id.clone(),
            version: component.version.clone(),
            dependencies: component.dependencies.clone(),
        });
        manifests.insert(component.id.clone(), (component, artifact, download_url));
    }
    let plan = select_outdated(&installed_manifest, &available)?;
    let updates: Vec<UpdateCandidate> = plan
        .modules
        .into_iter()
        .filter_map(|module| {
            let (manifest, artifact, download_url) = manifests.remove(&module)?;
            let current = installed.get(module.as_str()).copied().unwrap_or("0.0.0");
            if compare_semver(current, &manifest.version).is_ge() {
                return None;
            }
            Some(UpdateCandidate {
                module,
                installed: current.to_owned(),
                available: manifest.version,
                summary: if manifest.summary.is_empty() {
                    format!("Совместимый комплект: {}.", manifest.protocol)
                } else {
                    manifest.summary
                },
                changes: if manifest.changes.is_empty() {
                    vec!["Обновлена версия в проверенном совместимом комплекте.".to_owned()]
                } else {
                    manifest.changes
                },
                dependencies: manifest.dependencies,
                restart: manifest.restart,
                artifact,
                size: manifest.size,
                sha256: manifest.sha256,
                download_url,
            })
        })
        .collect();
    updater_first_if_required(
        updates,
        installed_updater,
        &compatibility.updater.minimum_version,
        compatibility.updater.update_first,
    )
}

/// The installer writes this local JSON file. Accept a UTF-8 BOM so clients
/// installed by older packages remain updateable; JSON itself does not permit
/// that marker before its first token.
fn read_update_config(path: &Path) -> Result<serde_json::Value, String> {
    let text = read_bounded_text(path)?;
    serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(&text))
        .map_err(|error| format!("updater: update.json содержит некорректный JSON: {error}"))
}

struct UpdaterHttpClient {
    client: reqwest::blocking::Client,
    github_token: Option<String>,
}

impl UpdaterHttpClient {
    fn get(&self, url: &str) -> reqwest::blocking::RequestBuilder {
        let request = self.client.get(url);
        if is_github_api_url(url) {
            if let Some(token) = self.github_token.as_deref() {
                return request.bearer_auth(token);
            }
        }
        request
    }
}

fn updater_http_client(github_token: Option<String>) -> Result<UpdaterHttpClient, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("EvoHime-Updater")
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if is_trusted_github_url(attempt.url()) && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|error| format!("updater: не удалось создать HTTP-клиент: {error}"))?;
    Ok(UpdaterHttpClient {
        client,
        github_token,
    })
}

fn is_github_api_url(value: &str) -> bool {
    value
        .parse::<reqwest::Url>()
        .map(|url| url.scheme() == "https" && url.host_str() == Some("api.github.com"))
        .unwrap_or(false)
}

fn is_trusted_github_url(url: &reqwest::Url) -> bool {
    let trusted_host = url.host_str().is_some_and(|host| {
        host == "api.github.com" || host == "github.com" || host.ends_with(".githubusercontent.com")
    });
    let query_is_allowed = url.query().is_none()
        || url
            .host_str()
            .is_some_and(|host| host.ends_with(".githubusercontent.com"));
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && query_is_allowed
        && trusted_host
}

fn is_github_release_asset_url(value: &str) -> bool {
    let Ok(url) = value.parse::<reqwest::Url>() else {
        return false;
    };
    let Some(segments) = url.path_segments() else {
        return false;
    };
    let segments = segments.collect::<Vec<_>>();
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host_str() == Some("github.com")
        && segments.len() >= 4
        && segments
            .windows(2)
            .any(|pair| pair == ["releases", "download"])
}

fn resolve_github_token(config: &serde_json::Value) -> Option<String> {
    resolve_github_token_with(config, |name| env::var(name).ok(), read_gh_cli_token)
}

fn resolve_github_token_with<Environment, GhToken>(
    config: &serde_json::Value,
    environment: Environment,
    gh_token: GhToken,
) -> Option<String>
where
    Environment: Fn(&str) -> Option<String>,
    GhToken: Fn() -> Option<String>,
{
    normalize_github_token(environment("EVOHIME_UPDATE_GITHUB_TOKEN").as_deref())
        .or_else(|| {
            normalize_github_token(config.get("githubToken").and_then(|value| value.as_str()))
        })
        .or_else(|| normalize_github_token(environment("GH_TOKEN").as_deref()))
        .or_else(|| normalize_github_token(environment("GITHUB_TOKEN").as_deref()))
        .or_else(gh_token)
}

fn normalize_github_token(value: Option<&str>) -> Option<String> {
    let candidate = value?.trim();
    if !(20..=255).contains(&candidate.len())
        || !candidate
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return None;
    }
    Some(candidate.to_owned())
}

fn read_gh_cli_token() -> Option<String> {
    let mut command = Command::new("gh");
    command
        .args(["auth", "token"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_hidden_process(&mut command);
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| normalize_github_token(Some(line)))
}

fn configure_hidden_process(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn get_json<T: DeserializeOwned>(
    client: &UpdaterHttpClient,
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
    client: &UpdaterHttpClient,
    url: &str,
    purpose: &str,
) -> Result<T, String> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("updater: {purpose}: сетевой запрос не удался: {error}"))?;
    let status = response.status();
    let mut body = Vec::with_capacity(MAX_JSON_RESPONSE_BYTES.min(16 * 1024));
    response
        .take((MAX_JSON_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|error| format!("updater: {purpose}: не удалось прочитать ответ: {error}"))?;
    if body.len() > MAX_JSON_RESPONSE_BYTES {
        return Err(format!(
            "updater: {purpose}: ответ превышает лимит {} байт",
            MAX_JSON_RESPONSE_BYTES
        ));
    }
    let body = String::from_utf8(body)
        .map_err(|error| format!("updater: {purpose}: ответ не является UTF-8: {error}"))?;
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
    parse_json_text::<InstalledManifest>(&read_bounded_text(&path)?)
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
    let manifest = parse_json_text::<RuntimeReleaseManifest>(&read_bounded_text(&path)?)
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
    let mut names = std::collections::HashSet::new();
    for entry in manifest.files.iter().chain(manifest.models.iter()) {
        if entry.name.is_empty()
            || entry.name.len() > 260
            || !names.insert(entry.name.as_str())
            || entry.name == "."
            || entry.name.ends_with('/')
            || entry.name.contains('\\')
            || entry.name.contains(':')
            || entry.name.contains("..")
            || Path::new(&entry.name).is_absolute()
            || entry.size == 0
            || entry.sha256.len() != 64
            || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "updater: небезопасная или повторная запись runtime {}",
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
    wait_pid: Option<u32>,
    relaunch: Option<&Path>,
    health_file: Option<&Path>,
    progress: &dyn Fn(&str, u8),
) -> Result<(), String> {
    let staging = data_dir.join("update-staging");
    let result = apply_updates_inner(
        install_dir,
        data_dir,
        updates,
        wait_pid,
        relaunch,
        health_file,
        progress,
    );
    cleanup_failed_staging(&staging, result)
}

fn apply_updates_inner(
    install_dir: &Path,
    data_dir: &Path,
    updates: &[UpdateCandidate],
    wait_pid: Option<u32>,
    relaunch: Option<&Path>,
    health_file: Option<&Path>,
    progress: &dyn Fn(&str, u8),
) -> Result<(), String> {
    let staging = data_dir.join("update-staging");
    let state = data_dir.join("update-state");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let client = updater_http_client(resolve_github_token(&read_update_config(
        &data_dir.join("update.json"),
    )?))?;
    let mut selected = Vec::new();
    let mut applied = Vec::new();
    let mut ui_update = None;
    let mut shell_host_update = None;
    let updater_update = updates.iter().find(|update| update.module == "updater");
    let runtime_update = updates
        .iter()
        .find(|update| update.module == "listener-runtime");
    let native_updates = updates
        .iter()
        .filter(|update| {
            update.module != "updater"
                && update.module != "ui-bundle"
                && update.module != "shell-host"
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
    if let Some(update) = updates.iter().find(|update| update.module == "shell-host") {
        progress("Скачивание shell-host", 0);
        download_verified_file(
            &client,
            update,
            &staging.join("shell-host.zip"),
            |downloaded| {
                progress(
                    "Скачивание shell-host",
                    progress_percent(downloaded, update.size),
                )
            },
        )?;
        extract_shell_host(&staging.join("shell-host.zip"), &staging.join("shell-host"))?;
        selected.push("shell-host.zip".to_owned());
        applied.push(update);
        shell_host_update = Some(update);
        progress("shell-host подготовлен", 100);
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
            &staging.join("updater.zip"),
            |downloaded| {
                progress(
                    "Скачивание updater",
                    progress_percent(downloaded, update.size),
                )
            },
        )?;
        extract_updater_package(
            &staging.join("updater.zip"),
            &staging.join("updater-package"),
        )?;
        fs::copy(
            staging.join("updater-package").join("evohime-updater.exe"),
            staging.join("evohime-updater.exe.next"),
        )
        .map_err(|error| error.to_string())?;
        validate_pe_image(&staging.join("evohime-updater.exe.next"))
            .map_err(|error| format!("updater: staged worker PE validation failed: {error}"))?;
        fs::rename(
            staging.join("updater-package").join("updater"),
            staging.join("updater"),
        )
        .map_err(|error| error.to_string())?;
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
            write_status_requiring_exit(
                data_dir,
                "Загрузка завершена. Перезапускаю updater для применения…",
                updates,
            );
            schedule_updater_replacement(
                install_dir,
                data_dir,
                &staging,
                &manifest_next,
                update,
                wait_pid,
            )?;
        }
        cleanup_completed_staging(&staging, updater_update.is_some());
        if updater_update.is_none() {
            write_status(data_dir, "ready", "Обновления модулей применены.", &[]);
        }
        return Ok(());
    }
    write_staged_manifest(
        install_dir,
        &staging.join("evohime.components.json"),
        &applied,
        ui_update,
    )?;
    let native_selected = selected
        .iter()
        .filter(|path| path.as_str() != "shell-host.zip")
        .cloned()
        .collect::<Vec<_>>();
    progress("Применение модулей", 0);
    write_status_requiring_exit(
        data_dir,
        "Загрузка завершена. Закрываю окно на время применения…",
        updates,
    );
    // The recovery/update agent owns the transaction engine directly. It must
    // not depend on the currently installed transaction executable: that file
    // may be missing, corrupt, or be the very component being repaired.
    evohime_tx::apply_component_set_staged(evohime_tx::ComponentSetApply {
        staging: &staging,
        install_dir,
        state_dir: &state,
        native_selected: &native_selected,
        ui_version: ui_update.map(|update| update.available.as_str()),
        shell_host: shell_host_update.is_some(),
        wait_pid,
        relaunch: if updater_update.is_none() {
            relaunch
        } else {
            None
        },
        health_file,
    })
    .map_err(|error| format!("updater: встроенное применение модулей не удалось: {error}"))?;
    progress("Модули применены", 100);
    let mut component_updates = applied.into_iter().chain(ui_update).collect::<Vec<_>>();
    if let Some(update) = updater_update {
        let manifest_next = install_dir.join("evohime.components.json.next");
        component_updates.push(update);
        merge_installed_manifest_to(install_dir, &component_updates, &manifest_next)?;
        apply_listener_runtime_if_needed(&client, runtime_update, data_dir, progress)?;
        schedule_updater_replacement(
            install_dir,
            data_dir,
            &staging,
            &manifest_next,
            update,
            wait_pid,
        )?;
        write_status(data_dir, "ready", "Обновления модулей применены.", &[]);
        return Ok(());
    }
    merge_installed_manifest_to(
        install_dir,
        &component_updates,
        &install_dir.join("evohime.components.json"),
    )?;
    apply_listener_runtime_if_needed(&client, runtime_update, data_dir, progress)?;
    cleanup_completed_staging(&staging, updater_update.is_some());
    write_status(data_dir, "ready", "Обновления модулей применены.", &[]);
    Ok(())
}

fn merge_installed_manifest_to(
    install_dir: &Path,
    applied: &[&UpdateCandidate],
    destination: &Path,
) -> Result<(), String> {
    let path = install_dir.join("evohime.components.json");
    let existing = if path.exists() {
        read_bounded_text(&path)?
    } else {
        "{\"components\":[]}".into()
    };
    let mut root =
        serde_json::from_str::<serde_json::Value>(&existing).map_err(|error| error.to_string())?;
    normalize_legacy_component_manifest(&mut root);
    let installed = serde_json::from_value::<InstalledManifest>(root.clone())
        .map_err(|error| format!("updater: component manifest повреждён: {error}"))?;
    select_outdated(&installed, &[])
        .map_err(|error| format!("updater: component manifest повреждён: {error}"))?;
    let components = root
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| "updater: component manifest повреждён".to_owned())?;
    for update in applied {
        let (artifact, path, size, sha256) = if update.module == "shell-host" {
            let shell = install_dir.join("EvoHime.exe");
            let (size, sha256) = stream_file_hash(&shell)?;
            (
                "EvoHime.exe".to_owned(),
                "EvoHime.exe".to_owned(),
                size,
                sha256,
            )
        } else {
            (
                update.artifact.clone(),
                update.artifact.clone(),
                update.size,
                update.sha256.clone(),
            )
        };
        let value = serde_json::json!({
            "id": update.module,
            "version": update.available,
            "artifact": artifact,
            "path": path,
            "size": size,
            "sha256": sha256,
            "dependencies": transaction_dependencies(&update.module, &update.dependencies),
            "required": true,
            "protocol": "desktop-ipc-v1",
            "restart": update.restart
        });
        if let Some(existing) = components.iter_mut().find(|item| {
            item.get("id").and_then(serde_json::Value::as_str) == Some(update.module.as_str())
        }) {
            *existing = value;
        } else {
            components.push(value);
        }
    }
    let temporary = destination.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&root).map_err(|error| error.to_string())?;
    if let Err(error) = fs::write(&temporary, bytes) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    if let Err(error) = fs::rename(&temporary, destination) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

/// Builds the transaction marker from the complete installed inventory.
///
/// The transaction validator checks dependencies across the whole marker, not
/// only across files selected for replacement. Keeping unchanged components in
/// staging is therefore required when an update references an already
/// installed dependency (for example shell-host -> core).
fn write_staged_manifest(
    install_dir: &Path,
    destination: &Path,
    applied: &[&UpdateCandidate],
    ui_update: Option<&UpdateCandidate>,
) -> Result<(), String> {
    let path = install_dir.join("evohime.components.json");
    let existing = if path.exists() {
        read_bounded_text(&path)?
    } else {
        "{\"components\":[]}".into()
    };
    let mut root = serde_json::from_str::<serde_json::Value>(&existing)
        .map_err(|error| format!("updater: component manifest повреждён: {error}"))?;
    normalize_legacy_component_manifest(&mut root);
    let object = root
        .as_object_mut()
        .ok_or_else(|| "updater: component manifest должен быть JSON-объектом".to_owned())?;
    object.insert(
        "schema".to_owned(),
        serde_json::Value::String("evohime.component-manifest.v1".to_owned()),
    );
    object.insert(
        "product".to_owned(),
        serde_json::Value::String("EvoHime".to_owned()),
    );
    object.insert(
        "release_id".to_owned(),
        serde_json::Value::String("module-update".to_owned()),
    );
    object.insert(
        "os".to_owned(),
        serde_json::Value::String("windows".to_owned()),
    );
    object.insert(
        "architecture".to_owned(),
        serde_json::Value::String("x64".to_owned()),
    );
    object.insert(
        "release_commit".to_owned(),
        serde_json::Value::String("0".repeat(40)),
    );
    let installed = serde_json::from_value::<InstalledManifest>(root.clone())
        .map_err(|error| format!("updater: component manifest повреждён: {error}"))?;
    select_outdated(&installed, &[])
        .map_err(|error| format!("updater: component manifest повреждён: {error}"))?;
    let components = root
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| "updater: component manifest повреждён".to_owned())?;
    for update in applied.iter().copied().chain(ui_update) {
        let value = serde_json::json!({
            "id": update.module, "version": update.available, "artifact": update.artifact,
            "path": update.artifact, "size": update.size, "sha256": update.sha256,
            "dependencies": transaction_dependencies(&update.module, &update.dependencies),
            "required": true,
            "restart": update.restart
        });
        if let Some(existing) = components.iter_mut().find(|item| {
            item.get("id").and_then(serde_json::Value::as_str) == Some(update.module.as_str())
        }) {
            *existing = value;
        } else {
            components.push(value);
        }
    }
    fs::write(
        destination,
        serde_json::to_vec_pretty(&root).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

/// Normalizes legacy dependency values before handing the marker to the
/// embedded transaction engine.
///
/// Older installers wrote a single dependency as a JSON string or `null`
/// instead of an array. Also,
/// `listener-runtime` is stored and updated under the data directory, not in
/// the install tree. Older installers nevertheless recorded it as a component
/// dependency of `listener`, which made the transaction validator reject any
/// later native update because that external component was absent from the
/// staged install marker.
fn normalize_legacy_component_manifest(root: &mut serde_json::Value) {
    let Some(components) = root
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for component in components {
        if let Some(dependencies) = component.get_mut("dependencies") {
            match dependencies {
                serde_json::Value::String(value) => {
                    *dependencies =
                        serde_json::Value::Array(vec![serde_json::Value::String(value.clone())]);
                }
                serde_json::Value::Null => {
                    *dependencies = serde_json::Value::Array(Vec::new());
                }
                _ => {}
            }
        }
        if component.get("id").and_then(serde_json::Value::as_str) != Some("listener") {
            continue;
        }
        let Some(dependencies) = component.get_mut("dependencies") else {
            continue;
        };
        match dependencies {
            serde_json::Value::Array(values) => {
                values.retain(|value| value.as_str() != Some("listener-runtime"));
            }
            serde_json::Value::String(value) if value == "listener-runtime" => {
                *dependencies = serde_json::Value::Array(Vec::new());
            }
            _ => {}
        }
    }
}

/// Keeps data-directory runtimes out of the install-tree transaction marker.
///
/// `listener-runtime` participates in the available-module dependency graph so
/// that a listener update also selects a newer runtime. Its files are applied
/// separately under the data directory, however, and it must not be listed as
/// a dependency of an install-tree component in `evohime.components.json`.
fn transaction_dependencies(module: &str, dependencies: &[String]) -> Vec<String> {
    dependencies
        .iter()
        .filter(|dependency| !(module == "listener" && dependency.as_str() == "listener-runtime"))
        .cloned()
        .collect()
}

fn stream_file_hash(path: &Path) -> Result<(u64, String), String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut digest = sha2::Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        size = size.saturating_add(read as u64);
        digest.update(&buffer[..read]);
    }
    let sha256 = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok((size, sha256))
}

fn schedule_updater_replacement(
    install_dir: &Path,
    data_dir: &Path,
    staging: &Path,
    manifest_next: &Path,
    update: &UpdateCandidate,
    wait_pid: Option<u32>,
) -> Result<(), String> {
    let state_dir = data_dir.join("update-state");
    fs::create_dir_all(&state_dir).map_err(|error| error.to_string())?;
    let script = state_dir.join(format!("updater-bootstrap-{}.cmd", std::process::id()));
    let marker = state_dir.join("updater-relaunch.pending");
    let updater = install_dir.join("evohime-updater.exe");
    let updater_ui_dir = install_dir.join("updater");
    let legacy_updater_ui = install_dir.join("EvoHimeUpdater.exe");
    let staged = staging.join("evohime-updater.exe.next");
    let staging_dir = staging.to_path_buf();
    let staged_ui_dir = staging.join("updater");
    let staged_package = staging.join("updater.zip");
    let installed_package = install_dir.join("updater.zip");
    let backup = state_dir.join("updater-previous.exe");
    let ui_backup = state_dir.join("updater-previous");
    let package_backup = state_dir.join("updater-package-previous.zip");
    let legacy_backup = state_dir.join("updater-legacy-previous.exe");
    let manifest_backup = state_dir.join("components-previous.json");
    let manifest = install_dir.join("evohime.components.json");
    let paths = UpdaterBootstrapPaths {
        updater: &updater,
        updater_ui_dir: &updater_ui_dir,
        legacy_updater_ui: &legacy_updater_ui,
        staged: &staged,
        staging_dir: &staging_dir,
        staged_ui_dir: &staged_ui_dir,
        staged_package: &staged_package,
        installed_package: &installed_package,
        backup: &backup,
        ui_backup: &ui_backup,
        package_backup: &package_backup,
        legacy_backup: &legacy_backup,
        manifest: &manifest,
        manifest_next,
        manifest_backup: &manifest_backup,
        marker: &marker,
        install_dir,
        wait_pid,
    };
    let content = updater_bootstrap_script(std::process::id(), &paths);
    if let Err(error) = fs::write(&script, content) {
        cleanup_bootstrap_files(&script, &marker);
        return Err(error.to_string());
    }
    if let Err(error) = fs::write(&marker, format!("{}\n", update.available)) {
        cleanup_bootstrap_files(&script, &marker);
        return Err(error.to_string());
    }
    let mut command = Command::new("cmd.exe");
    command
        .current_dir(&state_dir)
        .args([
            "/D",
            "/C",
            script
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_hidden_process(&mut command);
    if let Err(error) = command.spawn() {
        cleanup_bootstrap_files(&script, &marker);
        return Err(error.to_string());
    }
    Ok(())
}

fn cleanup_bootstrap_files(script: &Path, marker: &Path) {
    let _ = fs::remove_file(script);
    let _ = fs::remove_file(marker);
}

struct UpdaterBootstrapPaths<'a> {
    updater: &'a Path,
    updater_ui_dir: &'a Path,
    legacy_updater_ui: &'a Path,
    staged: &'a Path,
    staging_dir: &'a Path,
    staged_ui_dir: &'a Path,
    staged_package: &'a Path,
    installed_package: &'a Path,
    backup: &'a Path,
    ui_backup: &'a Path,
    package_backup: &'a Path,
    legacy_backup: &'a Path,
    manifest: &'a Path,
    manifest_next: &'a Path,
    manifest_backup: &'a Path,
    marker: &'a Path,
    install_dir: &'a Path,
    wait_pid: Option<u32>,
}

fn updater_bootstrap_script(pid: u32, paths: &UpdaterBootstrapPaths<'_>) -> String {
    // The surrounding `set "NAME=value"` syntax supplies the quoting. Adding
    // another pair here produces values such as `"C:\\..."` and makes every
    // later `%NAME%` expansion an invalid double-quoted path.
    let quote = |path: &Path| path.display().to_string().replace('%', "%%");
    format!(
        "@echo off\r\nsetlocal\r\nset \"UPDATER={updater}\"\r\nset \"UPDATER_UI_DIR={updater_ui_dir}\"\r\nset \"LEGACY_UI={legacy_updater_ui}\"\r\nset \"STAGED={staged}\"\r\nset \"STAGING_DIR={staging_dir}\"\r\nset \"STAGED_UI_DIR={staged_ui_dir}\"\r\nset \"STAGED_PACKAGE={staged_package}\"\r\nset \"INSTALLED_PACKAGE={installed_package}\"\r\nset \"BACKUP={backup}\"\r\nset \"UI_BACKUP={ui_backup}\"\r\nset \"PACKAGE_BACKUP={package_backup}\"\r\nset \"LEGACY_BACKUP={legacy_backup}\"\r\nset \"MANIFEST={manifest}\"\r\nset \"MANIFEST_NEXT={manifest_next}\"\r\nset \"MANIFEST_BACKUP={manifest_backup}\"\r\nset \"MARKER={marker}\"\r\nset \"INSTALL_DIR={install_dir}\"\r\n:wait\r\ntasklist /FI \"PID eq {pid}\" 2>NUL | findstr /C:\"{pid}\" >NUL\r\nif not errorlevel 1 (timeout /t 1 /nobreak >NUL & goto wait)\r\nif not exist \"%STAGED%\" goto fail\r\nif not exist \"%STAGED_UI_DIR%\\EvoHimeUpdater.exe\" goto fail\r\nif not exist \"%STAGED_UI_DIR%\\resources\\app.asar\" goto fail\r\nif not exist \"%STAGED_PACKAGE%\" goto fail\r\ndel /Q \"%BACKUP%\" \"%PACKAGE_BACKUP%\" \"%LEGACY_BACKUP%\" \"%MANIFEST_BACKUP%\" 2>NUL\r\nif exist \"%UI_BACKUP%\" rmdir /S /Q \"%UI_BACKUP%\" 2>NUL\r\ncopy /Y \"%UPDATER%\" \"%BACKUP%\" >NUL\r\nif errorlevel 1 goto fail\r\nmove /Y \"%STAGED%\" \"%UPDATER%\" >NUL\r\nif errorlevel 1 goto restore\r\nif exist \"%UPDATER_UI_DIR%\" move /Y \"%UPDATER_UI_DIR%\" \"%UI_BACKUP%\" >NUL\r\nif errorlevel 1 goto restore\r\nmove /Y \"%STAGED_UI_DIR%\" \"%UPDATER_UI_DIR%\" >NUL\r\nif errorlevel 1 goto restore_ui\r\nif exist \"%LEGACY_UI%\" move /Y \"%LEGACY_UI%\" \"%LEGACY_BACKUP%\" >NUL\r\nif errorlevel 1 goto restore_ui\r\nif exist \"%INSTALLED_PACKAGE%\" move /Y \"%INSTALLED_PACKAGE%\" \"%PACKAGE_BACKUP%\" >NUL\r\nif errorlevel 1 goto restore_legacy\r\nmove /Y \"%STAGED_PACKAGE%\" \"%INSTALLED_PACKAGE%\" >NUL\r\nif errorlevel 1 goto restore_package\r\nif exist \"%MANIFEST%\" copy /Y \"%MANIFEST%\" \"%MANIFEST_BACKUP%\" >NUL\r\nif errorlevel 1 goto restore_package\r\nmove /Y \"%MANIFEST_NEXT%\" \"%MANIFEST%\" >NUL\r\nif errorlevel 1 goto restore_manifest\r\n\"%UPDATER%\" --check --install-dir \"%INSTALL_DIR%\" >NUL 2>NUL\r\nif errorlevel 1 goto restore_manifest\r\ndel /Q \"%BACKUP%\" \"%PACKAGE_BACKUP%\" \"%LEGACY_BACKUP%\" \"%MANIFEST_BACKUP%\" \"%MARKER%\" 2>NUL\r\nstart \"\" \"%UPDATER_UI_DIR%\\EvoHimeUpdater.exe\" --evohime-updater --install-dir \"%INSTALL_DIR%\"\r\ngoto cleanup\r\n:restore_manifest\r\nif exist \"%MANIFEST%\" del /Q \"%MANIFEST%\" 2>NUL\r\nif exist \"%MANIFEST_BACKUP%\" move /Y \"%MANIFEST_BACKUP%\" \"%MANIFEST%\" >NUL\r\n:restore_package\r\nif exist \"%INSTALLED_PACKAGE%\" del /Q \"%INSTALLED_PACKAGE%\" 2>NUL\r\nif exist \"%PACKAGE_BACKUP%\" move /Y \"%PACKAGE_BACKUP%\" \"%INSTALLED_PACKAGE%\" >NUL\r\n:restore_legacy\r\nif exist \"%LEGACY_UI%\" del /Q \"%LEGACY_UI%\" 2>NUL\r\nif exist \"%LEGACY_BACKUP%\" move /Y \"%LEGACY_BACKUP%\" \"%LEGACY_UI%\" >NUL\r\n:restore_ui\r\nif exist \"%UPDATER_UI_DIR%\" rmdir /S /Q \"%UPDATER_UI_DIR%\" 2>NUL\r\nif exist \"%UI_BACKUP%\" move /Y \"%UI_BACKUP%\" \"%UPDATER_UI_DIR%\" >NUL\r\n:restore\r\nif exist \"%UPDATER%\" del /Q \"%UPDATER%\" 2>NUL\r\nif exist \"%BACKUP%\" move /Y \"%BACKUP%\" \"%UPDATER%\" >NUL\r\n:fail\r\ndel /Q \"%MARKER%\" 2>NUL\r\nif exist \"%UPDATER_UI_DIR%\\EvoHimeUpdater.exe\" start \"\" \"%UPDATER_UI_DIR%\\EvoHimeUpdater.exe\" --evohime-updater --install-dir \"%INSTALL_DIR%\"\r\nif exist \"%LEGACY_UI%\" start \"\" \"%LEGACY_UI%\" --evohime-updater --install-dir \"%INSTALL_DIR%\"\r\n:cleanup\r\nrmdir /S /Q \"%UI_BACKUP%\" 2>NUL\r\nrmdir /S /Q \"%STAGING_DIR%\" 2>NUL\r\ndel /Q \"%~f0\" 2>NUL\r\n",
        pid = pid,
        updater = quote(paths.updater),
        updater_ui_dir = quote(paths.updater_ui_dir),
        legacy_updater_ui = quote(paths.legacy_updater_ui),
        staged = quote(paths.staged),
        staging_dir = quote(paths.staging_dir),
        staged_ui_dir = quote(paths.staged_ui_dir),
        staged_package = quote(paths.staged_package),
        installed_package = quote(paths.installed_package),
        backup = quote(paths.backup),
        ui_backup = quote(paths.ui_backup),
        package_backup = quote(paths.package_backup),
        legacy_backup = quote(paths.legacy_backup),
        manifest = quote(paths.manifest),
        manifest_next = quote(paths.manifest_next),
        manifest_backup = quote(paths.manifest_backup),
        marker = quote(paths.marker),
        install_dir = quote(paths.install_dir),
    )
    .replace(
        "setlocal\r\n",
        &format!(
            "setlocal\r\nset \"COMMITTED=0\"\r\nset \"UI_PID={}\"\r\n",
            paths.wait_pid.map(|pid| pid.to_string()).unwrap_or_default()
        ),
    )
    .replace(
        "if not exist \"%STAGED%\" goto fail",
        ":wait_ui\r\nif \"%UI_PID%\"==\"\" goto after_wait_ui\r\ntasklist /FI \"PID eq %UI_PID%\" 2>NUL | findstr /C:\"%UI_PID%\" >NUL\r\nif not errorlevel 1 (timeout /t 1 /nobreak >NUL & goto wait_ui)\r\n:after_wait_ui\r\nif not exist \"%STAGED%\" goto fail",
    )
    .replace(
        "del /Q \"%BACKUP%\" \"%PACKAGE_BACKUP%\" \"%LEGACY_BACKUP%\" \"%MANIFEST_BACKUP%\" \"%MARKER%\" 2>NUL\r\nstart",
        "del /Q \"%BACKUP%\" \"%PACKAGE_BACKUP%\" \"%LEGACY_BACKUP%\" \"%MANIFEST_BACKUP%\" \"%MARKER%\" 2>NUL\r\nset \"COMMITTED=1\"\r\nstart",
    )
    .replace(
        ":cleanup\r\nrmdir /S /Q \"%UI_BACKUP%\" 2>NUL\r\nrmdir /S /Q \"%STAGING_DIR%\" 2>NUL",
        ":cleanup\r\nif \"%COMMITTED%\"==\"1\" rmdir /S /Q \"%UI_BACKUP%\" 2>NUL\r\nif \"%COMMITTED%\"==\"1\" rmdir /S /Q \"%STAGING_DIR%\" 2>NUL",
    )
}

fn write_status(data_dir: &Path, phase: &'static str, message: &str, updates: &[UpdateCandidate]) {
    write_status_inner(data_dir, phase, message, updates, false);
}

fn write_status_requiring_exit(data_dir: &Path, message: &str, updates: &[UpdateCandidate]) {
    write_status_inner(data_dir, "applying", message, updates, true);
}

fn write_status_inner(
    data_dir: &Path,
    phase: &'static str,
    message: &str,
    updates: &[UpdateCandidate],
    requires_exit: bool,
) {
    let state = data_dir.join("update-state");
    if fs::create_dir_all(&state).is_ok() {
        let status = UpdaterStatus {
            schema: "evohime.updater-status.v1",
            phase,
            message: message.to_owned(),
            error: (phase == "failed").then(|| message.to_owned()),
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
            requires_exit,
            recovery: recovery_status(data_dir),
        };
        let path = state.join("updater.json");
        if let Ok(value) = serde_json::to_vec_pretty(&status) {
            let _ = fs::write(path, value);
        }
    }
}

fn recovery_status(data_dir: &Path) -> Option<evohime_update_agent::RecoveryStatus> {
    let state = data_dir.join("update-state");
    let journal = read_recovery_journal(&state.join("recovery.json"))
        .ok()
        .flatten()?;
    Some(evohime_update_agent::RecoveryStatus {
        phase: journal.phase,
        active_slot: journal.active_slot,
        active_version: journal.active_version,
        fallback_available: journal.fallback_available
            || state.join("updater-fallback.exe").is_file(),
        retry_count: journal.retry_count,
        reason_code: journal.reason_code,
    })
}

fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    read_bounded_text(Path::new(path)).and_then(|text| parse_json_text(&text))
}

fn parse_json_text<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(text))
        .map_err(|error| error.to_string())
}

fn read_bounded_text(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_LOCAL_JSON_BYTES as u64 {
        return Err(format!(
            "local JSON exceeds the read limit of {} bytes",
            MAX_LOCAL_JSON_BYTES
        ));
    }
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(MAX_LOCAL_JSON_BYTES.min(16 * 1024));
    file.take((MAX_LOCAL_JSON_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_LOCAL_JSON_BYTES {
        return Err(format!(
            "local JSON exceeds the read limit of {} bytes",
            MAX_LOCAL_JSON_BYTES
        ));
    }
    String::from_utf8(bytes).map_err(|error| error.to_string())
}
fn fail(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("updater: {error}");
    ExitCode::from(1)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
