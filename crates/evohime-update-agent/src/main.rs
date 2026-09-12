#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

use evohime_update_agent::{
    compare_semver, deserialize_nullable_vec, is_valid_semver, select_outdated, InstalledManifest,
    ModuleRecord, UpdateCandidate, UpdaterModuleStatus, UpdaterStatus,
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

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<_>>();
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
        Ok(plan) => {
            println!("{}", serde_json::to_string(&plan).expect("plan serializes"));
            ExitCode::SUCCESS
        }
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
        .expect("validated compatible manifest");
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
        if self.github_token.is_some() && is_github_api_url(url) {
            request.bearer_auth(self.github_token.as_deref().expect("token is present"))
        } else {
            request
        }
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
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host_str().is_some_and(|host| {
            host == "api.github.com"
                || host == "github.com"
                || host.ends_with(".githubusercontent.com")
        })
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
    progress: &dyn Fn(&str, u8),
) -> Result<(), String> {
    let staging = data_dir.join("update-staging");
    let result = apply_updates_inner(install_dir, data_dir, updates, progress);
    cleanup_failed_staging(&staging, result)
}

fn apply_updates_inner(
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
        cleanup_completed_staging(&staging, updater_update.is_some());
        write_status(data_dir, "ready", "Обновления модулей применены.", &[]);
        return Ok(());
    }
    let manifest = serde_json::json!({
        "schema": "evohime.component-manifest.v1", "os": "windows", "architecture": "x64",
        "product": "EvoHime", "release_id": format!("module-update-{}", std::process::id()),
        "release_commit": "0000000000000000000000000000000000000000",
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
    let worker_copy = state.join(format!("transaction-{}-worker.exe", std::process::id()));
    fs::create_dir_all(&state).map_err(|error| error.to_string())?;
    fs::copy(&worker, &worker_copy).map_err(|error| error.to_string())?;
    let native_selected = selected
        .iter()
        .filter(|path| path.as_str() != "shell-host.zip")
        .cloned()
        .collect::<Vec<_>>();
    let selected_arg = native_selected.join(",");
    let mode = if shell_host_update.is_some() || ui_update.is_some() {
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
    if !native_selected.is_empty() {
        command.args(["--selected", selected_arg.as_str()]);
    }
    if shell_host_update.is_some() {
        command.arg("--shell-host");
    }
    if let Some(update) = ui_update {
        command.args(["--ui-version", update.available.as_str()]);
    }
    progress("Применение модулей", 0);
    let status = command.status();
    cleanup_worker_copy(&worker_copy);
    let status = status.map_err(|error| error.to_string())?;
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
    cleanup_completed_staging(&staging, updater_update.is_some());
    write_status(data_dir, "ready", "Обновления модулей применены.", &[]);
    Ok(())
}

fn cleanup_completed_staging(staging: &Path, keep_for_bootstrap: bool) {
    if !keep_for_bootstrap {
        let _ = fs::remove_dir_all(staging);
    }
}

fn cleanup_worker_copy(worker_copy: &Path) {
    let _ = fs::remove_file(worker_copy);
}

fn apply_listener_runtime_if_needed(
    client: &UpdaterHttpClient,
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
    client: &UpdaterHttpClient,
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
    let result = extract_ui_bundle_inner(archive_path, destination);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn extract_ui_bundle_inner(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let archive_file = fs::File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(archive_file).map_err(|error| error.to_string())?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("updater: UI archive содержит слишком много записей".to_owned());
    }
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let mut extracted_paths = std::collections::HashSet::new();
    let mut extracted_bytes = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if entry.is_symlink() {
            return Err("updater: UI archive содержит symlink".to_owned());
        }
        let relative = entry
            .enclosed_name()
            .and_then(|path| normalize_archive_path(&path))
            .ok_or_else(|| "updater: UI archive содержит небезопасный путь".to_owned())?;
        if !extracted_paths.insert(relative.to_owned()) {
            return Err("updater: UI archive содержит повторяющийся путь".to_owned());
        }
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = fs::File::create(&target).map_err(|error| error.to_string())?;
            let remaining = MAX_ARCHIVE_UNCOMPRESSED_BYTES
                .checked_sub(extracted_bytes)
                .ok_or_else(|| "updater: UI archive превышает лимит распаковки".to_owned())?;
            if entry.size() > remaining {
                return Err("updater: UI archive превышает лимит распаковки".to_owned());
            }
            let written = copy_reader_bounded(&mut entry, &mut file, remaining)?;
            if written != entry.size() {
                return Err("updater: UI archive содержит усечённый файл".to_owned());
            }
            extracted_bytes = extracted_bytes
                .checked_add(written)
                .ok_or_else(|| "updater: размер распаковки переполнен".to_owned())?;
        }
    }
    if !destination.join("index.html").is_file() {
        return Err("updater: UI archive не содержит index.html".to_owned());
    }
    Ok(())
}

fn extract_shell_host(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let result = extract_shell_host_inner(archive_path, destination);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn extract_shell_host_inner(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let archive_file = fs::File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(archive_file).map_err(|error| error.to_string())?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("updater: shell-host archive содержит слишком много записей".to_owned());
    }
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let mut extracted_paths = std::collections::HashSet::new();
    let mut extracted_bytes = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if entry.is_symlink() {
            return Err("updater: shell-host archive содержит symlink".to_owned());
        }
        let relative = entry
            .enclosed_name()
            .and_then(|path| normalize_archive_path(&path))
            .ok_or_else(|| "updater: shell-host archive содержит небезопасный путь".to_owned())?;
        if !extracted_paths.insert(relative.to_owned()) {
            return Err("updater: shell-host archive содержит повторяющийся путь".to_owned());
        }
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = fs::File::create(&target).map_err(|error| error.to_string())?;
            let remaining = MAX_ARCHIVE_UNCOMPRESSED_BYTES
                .checked_sub(extracted_bytes)
                .ok_or_else(|| {
                    "updater: shell-host archive превышает лимит распаковки".to_owned()
                })?;
            if entry.size() > remaining {
                return Err("updater: shell-host archive превышает лимит распаковки".to_owned());
            }
            let written = copy_reader_bounded(&mut entry, &mut file, remaining)?;
            if written != entry.size() {
                return Err("updater: shell-host archive содержит усечённый файл".to_owned());
            }
            extracted_bytes = extracted_bytes
                .checked_add(written)
                .ok_or_else(|| "updater: размер распаковки переполнен".to_owned())?;
        }
    }
    if !destination.join("EvoHime.exe").is_file()
        || !destination.join("resources").join("app.asar").is_file()
    {
        return Err("updater: shell-host archive не содержит полный Electron package".to_owned());
    }
    Ok(())
}

const MAX_ARCHIVE_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;

fn copy_reader_bounded<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    limit: u64,
) -> Result<u64, String> {
    let mut copied = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Ok(copied);
        }
        copied = copied
            .checked_add(count as u64)
            .ok_or_else(|| "updater: размер распаковки переполнен".to_owned())?;
        if copied > limit {
            return Err("updater: archive превышает лимит распаковки".to_owned());
        }
        writer
            .write_all(&buffer[..count])
            .map_err(|error| error.to_string())?;
    }
}

fn normalize_archive_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => {
                let part = part.to_string_lossy();
                if part.contains('\\') || part.contains(':') {
                    return None;
                }
                normalized.push(part.as_ref());
            }
            _ => return None,
        }
    }
    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

fn apply_listener_runtime(
    client: &UpdaterHttpClient,
    update: &UpdateCandidate,
    data_dir: &Path,
    progress: &dyn Fn(&str, u8),
) -> Result<(), String> {
    let staging = data_dir.join("update-staging").join("listener-runtime");
    let result = apply_listener_runtime_inner(client, update, data_dir, progress);
    cleanup_failed_staging(&staging, result)
}

fn apply_listener_runtime_inner(
    client: &UpdaterHttpClient,
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

fn cleanup_failed_staging(staging: &Path, result: Result<(), String>) -> Result<(), String> {
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result
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
    let existing = if path.exists() {
        read_bounded_text(&path)?
    } else {
        "{\"components\":[]}".into()
    };
    let mut root =
        serde_json::from_str::<serde_json::Value>(&existing).map_err(|error| error.to_string())?;
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
        let value = serde_json::json!({"id": update.module, "version": update.available, "artifact": artifact, "path": path, "size": size, "sha256": sha256, "dependencies": update.dependencies, "required": true, "protocol": "desktop-ipc-v1", "restart": update.restart});
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
    let manifest = install_dir.join("evohime.components.json");
    let paths = UpdaterBootstrapPaths {
        updater: &updater,
        updater_ui: &updater_ui,
        staged: &staged,
        backup: &backup,
        manifest: &manifest,
        manifest_next,
        manifest_backup: &manifest_backup,
        marker: &marker,
        install_dir,
    };
    let content = updater_bootstrap_script(std::process::id(), &paths);
    fs::write(&script, content).map_err(|error| error.to_string())?;
    if let Err(error) = fs::write(&marker, format!("{}\n", update.available)) {
        cleanup_bootstrap_files(&script, &marker);
        return Err(error.to_string());
    }
    if let Err(error) = Command::new("cmd.exe")
        .current_dir(&state_dir)
        .args([
            "/D",
            "/C",
            script
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        ])
        .spawn()
    {
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
    updater_ui: &'a Path,
    staged: &'a Path,
    backup: &'a Path,
    manifest: &'a Path,
    manifest_next: &'a Path,
    manifest_backup: &'a Path,
    marker: &'a Path,
    install_dir: &'a Path,
}

fn updater_bootstrap_script(pid: u32, paths: &UpdaterBootstrapPaths<'_>) -> String {
    let quote = |path: &Path| format!("\"{}\"", path.display().to_string().replace('%', "%%"));
    format!(
        "@echo off\r\nsetlocal\r\nset \"UPDATER={updater}\"\r\nset \"UPDATER_UI={updater_ui}\"\r\nset \"STAGED={staged}\"\r\nset \"BACKUP={backup}\"\r\nset \"MANIFEST={manifest}\"\r\nset \"MANIFEST_NEXT={manifest_next}\"\r\nset \"MANIFEST_BACKUP={manifest_backup}\"\r\nset \"MARKER={marker}\"\r\nset \"INSTALL_DIR={install_dir}\"\r\n:wait\r\ntasklist /FI \"PID eq {pid}\" 2>NUL | findstr /C:\"{pid}\" >NUL\r\nif not errorlevel 1 (timeout /t 1 /nobreak >NUL & goto wait)\r\nif not exist \"%STAGED%\" goto fail\r\ncopy /Y \"%UPDATER%\" \"%BACKUP%\" >NUL\r\nif errorlevel 1 goto fail\r\nmove /Y \"%STAGED%\" \"%UPDATER%\" >NUL\r\nif errorlevel 1 goto restore\r\ncopy /Y \"%MANIFEST%\" \"%MANIFEST_BACKUP%\" >NUL\r\nif errorlevel 1 goto restore\r\nmove /Y \"%MANIFEST_NEXT%\" \"%MANIFEST%\" >NUL\r\nif errorlevel 1 goto restore_manifest\r\n\"%UPDATER%\" --check --install-dir \"%INSTALL_DIR%\" >NUL 2>NUL\r\nif errorlevel 1 goto restore_manifest\r\ndel /Q \"%BACKUP%\" 2>NUL\r\ndel /Q \"%MANIFEST_BACKUP%\" 2>NUL\r\ndel /Q \"%MARKER%\" 2>NUL\r\nstart \"\" \"%UPDATER_UI%\" --evohime-updater --install-dir \"%INSTALL_DIR%\"\r\ngoto cleanup\r\n:restore_manifest\r\nmove /Y \"%MANIFEST_BACKUP%\" \"%MANIFEST%\" >NUL\r\n:restore\r\nif exist \"%UPDATER%\" del /Q \"%UPDATER%\" 2>NUL\r\nif exist \"%BACKUP%\" move /Y \"%BACKUP%\" \"%UPDATER%\" >NUL\r\n:fail\r\ndel /Q \"%MARKER%\" 2>NUL\r\nstart \"\" \"%UPDATER_UI%\" --evohime-updater --install-dir \"%INSTALL_DIR%\"\r\n:cleanup\r\ndel /Q \"%~f0\" 2>NUL\r\n",
        pid = pid,
        updater = quote(paths.updater),
        updater_ui = quote(paths.updater_ui),
        staged = quote(paths.staged),
        backup = quote(paths.backup),
        manifest = quote(paths.manifest),
        manifest_next = quote(paths.manifest_next),
        manifest_backup = quote(paths.manifest_backup),
        marker = quote(paths.marker),
        install_dir = quote(paths.install_dir),
    )
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
mod tests {
    use super::{
        cleanup_failed_staging, cleanup_worker_copy, copy_reader_bounded, is_github_api_url,
        is_github_release_asset_url, is_trusted_github_url, merge_installed_manifest_to,
        normalize_github_token, parse_json_body, read_installed_module_manifest,
        read_update_config, resolve_github_token_with, stream_file_hash, updater_bootstrap_script,
        updater_first_if_required, updater_http_client, validate_compatible_manifest,
        validate_runtime_manifest, CompatibleComponent, CompatibleManifest, RuntimeReleaseEntry,
        RuntimeReleaseManifest, UpdateCandidate, UpdaterBootstrapPaths, UpdaterRequirement,
    };
    use sha2::Digest;
    use std::{
        fs,
        io::Write,
        path::Path,
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
    fn ignores_a_flag_when_an_argument_value_is_missing() {
        let args = vec![
            "evohime-updater".into(),
            "--manifest".into(),
            "--available".into(),
        ];
        assert!(super::argument_value(&args, "--manifest").is_none());
        assert!(super::argument_value(&args, "--available").is_none());
    }

    #[test]
    fn legacy_manifest_reader_accepts_a_utf8_bom() {
        let path = std::env::temp_dir().join(format!(
            "evohime-available-manifest-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::write(
            &path,
            b"\xef\xbb\xbf[{\"id\":\"core\",\"version\":\"1.0.0\"}]",
        )
        .expect("write BOM-prefixed manifest");

        let available: Vec<super::ModuleRecord> =
            super::read(path.to_str().expect("temporary path is UTF-8"))
                .expect("BOM-prefixed manifest must parse");
        let _ = fs::remove_file(path);
        assert_eq!(available[0].id, "core");
    }

    #[test]
    fn installed_manifest_reader_accepts_a_utf8_bom() {
        let root = std::env::temp_dir().join(format!(
            "evohime-installed-manifest-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temporary install directory");
        fs::write(
            root.join("evohime.components.json"),
            b"\xef\xbb\xbf{\"components\":[{\"id\":\"core\",\"version\":\"1.0.0\",\"dependencies\":null}]}",
        )
        .expect("write BOM-prefixed installed manifest");

        let manifest = read_installed_module_manifest(&root)
            .expect("BOM-prefixed installed manifest must parse");
        let _ = fs::remove_dir_all(root);
        assert_eq!(manifest.components[0].id, "core");
    }

    #[test]
    fn manifest_merge_fails_closed_on_corrupt_existing_manifest() {
        let root = std::env::temp_dir().join(format!(
            "evohime-merge-manifest-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temporary install directory");
        fs::write(root.join("evohime.components.json"), b"not-json")
            .expect("write corrupt manifest");
        let destination = root.join("evohime.components.json.next");

        let error = merge_installed_manifest_to(&root, &[], &destination)
            .expect_err("corrupt manifest must not be replaced with an empty one");
        assert!(!error.is_empty());
        assert!(!destination.exists());
        fs::remove_dir_all(root).expect("remove temporary install directory");
    }

    #[test]
    fn streams_shell_host_hash_and_reports_size() {
        let path = std::env::temp_dir().join(format!(
            "evohime-shell-hash-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::write(&path, b"shell-host").expect("write shell host");
        let (size, hash) = stream_file_hash(&path).expect("hash shell host");
        assert_eq!(size, 10);
        assert_eq!(hash, format!("{:x}", sha2::Sha256::digest(b"shell-host")));
        fs::remove_file(path).expect("remove shell host");
    }

    #[test]
    fn bounded_archive_copy_rejects_expansion_over_limit() {
        let mut output = Vec::new();
        let error = copy_reader_bounded(&mut std::io::Cursor::new(b"1234"), &mut output, 3)
            .expect_err("archive expansion must be bounded");
        assert!(error.contains("лимит распаковки"));
        assert!(output.len() <= 3);
    }

    #[test]
    fn failed_archive_extraction_removes_partial_destination() {
        let root = std::env::temp_dir().join(format!(
            "evohime-archive-cleanup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("destination")).expect("create destination");
        fs::write(root.join("destination/partial"), b"partial").expect("write partial file");
        fs::write(root.join("broken.zip"), b"not a zip").expect("write broken archive");

        assert!(
            super::extract_ui_bundle(&root.join("broken.zip"), &root.join("destination")).is_err()
        );
        assert!(!root.join("destination").exists());
        fs::remove_dir_all(root).expect("remove temporary archive directory");
    }

    #[test]
    fn archive_entry_count_is_bounded() {
        let root = std::env::temp_dir().join(format!(
            "evohime-archive-entries-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temporary archive directory");
        let archive_path = root.join("many-entries.zip");
        let file = fs::File::create(&archive_path).expect("create archive");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for index in 0..=super::MAX_ARCHIVE_ENTRIES {
            archive
                .start_file(format!("entry-{index}"), options)
                .expect("start archive entry");
        }
        archive.finish().expect("finish archive");

        let error = super::extract_ui_bundle(&archive_path, &root.join("destination"))
            .expect_err("archives with too many entries must be rejected");
        assert!(error.contains("слишком много записей"));
        fs::remove_dir_all(root).expect("remove temporary archive directory");
    }

    #[test]
    fn duplicate_archive_paths_are_rejected() {
        let root = std::env::temp_dir().join(format!(
            "evohime-archive-duplicate-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temporary archive directory");
        let archive_path = root.join("duplicate.zip");
        let file = fs::File::create(&archive_path).expect("create archive");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        archive
            .start_file("index.html", options)
            .expect("start first entry");
        archive.write_all(b"first").expect("write first entry");
        archive
            .start_file("./index.html", options)
            .expect("start duplicate entry");
        archive.write_all(b"second").expect("write duplicate entry");
        archive.finish().expect("finish archive");

        let error = super::extract_ui_bundle(&archive_path, &root.join("destination"))
            .expect_err("duplicate archive paths must be rejected");
        assert!(error.contains("повторяющийся путь"));
        fs::remove_dir_all(root).expect("remove temporary archive directory");
    }

    #[test]
    fn archive_paths_reject_windows_only_separators_and_streams() {
        assert!(super::normalize_archive_path(Path::new(r"assets\\app.js")).is_none());
        assert!(super::normalize_archive_path(Path::new("assets/app.js:stream")).is_none());
    }

    #[test]
    fn failed_listener_runtime_update_removes_staging() {
        let root = std::env::temp_dir().join(format!(
            "evohime-runtime-cleanup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        let staging = root.join("update-staging/listener-runtime");
        fs::create_dir_all(&staging).expect("create runtime staging");
        fs::write(staging.join("partial.bin"), b"partial").expect("write partial runtime");

        let result = cleanup_failed_staging(&staging, Err("download failed".to_owned()));
        assert!(result.is_err());
        assert!(!staging.exists());
        fs::remove_dir_all(root).expect("remove temporary runtime directory");
    }

    #[test]
    fn successful_update_cleans_staging_unless_bootstrap_needs_it() {
        let root = std::env::temp_dir().join(format!(
            "evohime-staging-cleanup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        let staging = root.join("update-staging");
        fs::create_dir_all(&staging).expect("create staging");
        super::cleanup_completed_staging(&staging, false);
        assert!(!staging.exists());

        fs::create_dir_all(&staging).expect("recreate staging");
        super::cleanup_completed_staging(&staging, true);
        assert!(staging.exists());
        fs::remove_dir_all(root).expect("remove temporary staging directory");
    }

    #[test]
    fn worker_copy_cleanup_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "evohime-worker-cleanup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create worker state directory");
        let worker = root.join("transaction-worker.exe");
        fs::write(&worker, b"worker").expect("write worker copy");
        cleanup_worker_copy(&worker);
        cleanup_worker_copy(&worker);
        assert!(!worker.exists());
        fs::remove_dir_all(root).expect("remove temporary worker directory");
    }

    #[test]
    fn bootstrap_failure_cleanup_removes_script_and_marker() {
        let root = std::env::temp_dir().join(format!(
            "evohime-bootstrap-cleanup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create bootstrap state directory");
        let script = root.join("updater-bootstrap.cmd");
        let marker = root.join("updater-relaunch.pending");
        fs::write(&script, b"script").expect("write script");
        fs::write(&marker, b"pending").expect("write marker");

        super::cleanup_bootstrap_files(&script, &marker);
        assert!(!script.exists());
        assert!(!marker.exists());
        fs::remove_dir_all(root).expect("remove temporary bootstrap directory");
    }

    #[test]
    fn manifest_merge_removes_temporary_file_after_rename_failure() {
        let root = std::env::temp_dir().join(format!(
            "evohime-merge-cleanup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temporary install directory");
        fs::write(
            root.join("evohime.components.json"),
            br#"{"components":[]}"#,
        )
        .expect("write manifest");
        let destination = root.join("blocked.json");
        fs::create_dir(&destination).expect("create blocking destination");

        assert!(merge_installed_manifest_to(&root, &[], &destination).is_err());
        assert!(!root.join("blocked.json.tmp").exists());
        fs::remove_dir_all(root).expect("remove temporary install directory");
    }

    #[test]
    fn compatible_manifest_accepts_nullable_string_arrays_from_github() {
        let manifest: CompatibleManifest = parse_json_body(
            r#"{
                "schema":"evohime.compatible-set.v1",
                "product":"EvoHime",
                "os":"windows",
                "architecture":"x64",
                "updater":{"minimum_version":"1.0.0","update_first":true},
                "components":[{
                    "id":"listener-runtime",
                    "version":"1.0.0",
                    "release_tag":"module-listener-runtime-v1.0.0",
                    "manifest_asset":"listener-runtime.json",
                    "artifact":null,
                    "size":0,
                    "sha256":"",
                    "dependencies":null,
                    "restart":"listener",
                    "changes":null
                }]
            }"#,
            "манифест совместимого комплекта",
        )
        .expect("nullable arrays must be accepted");

        assert!(manifest.components[0].dependencies.is_empty());
        assert!(manifest.components[0].changes.is_empty());
    }

    #[test]
    fn compatible_manifest_binds_components_to_exact_release_tags() {
        let mut manifest = CompatibleManifest {
            schema: "evohime.compatible-set.v1".into(),
            product: "EvoHime".into(),
            os: "windows".into(),
            architecture: "x64".into(),
            updater: UpdaterRequirement {
                minimum_version: "1.0.0".into(),
                update_first: true,
            },
            components: vec![
                CompatibleComponent {
                    id: "updater".into(),
                    version: "1.1.0".into(),
                    release_tag: "module-updater-v1.1.0".into(),
                    manifest_asset: "updater.manifest.json".into(),
                    artifact: Some("evohime-updater.exe".into()),
                    size: 10,
                    sha256: "a".repeat(64),
                    dependencies: vec![],
                    restart: "updater".into(),
                    protocol: "desktop-ipc-v1".into(),
                    summary: String::new(),
                    changes: vec![],
                },
                CompatibleComponent {
                    id: "core".into(),
                    version: "1.0.0".into(),
                    release_tag: "module-core-v1.0.0".into(),
                    manifest_asset: "core.manifest.json".into(),
                    artifact: Some("evohime-core.exe".into()),
                    size: 10,
                    sha256: "b".repeat(64),
                    dependencies: vec!["updater".into()],
                    restart: "core".into(),
                    protocol: "desktop-ipc-v1".into(),
                    summary: String::new(),
                    changes: vec![],
                },
            ],
        };
        for (index, id) in super::MODULE_IDS.iter().enumerate() {
            if manifest
                .components
                .iter()
                .any(|component| component.id == *id)
            {
                continue;
            }
            let version = format!("1.0.{}", index + 1);
            manifest.components.push(CompatibleComponent {
                id: (*id).into(),
                version: version.clone(),
                release_tag: format!("module-{id}-v{version}"),
                manifest_asset: if *id == "listener-runtime" {
                    "listener-runtime.json".into()
                } else {
                    format!("{id}.manifest.json")
                },
                artifact: (*id != "listener-runtime").then(|| format!("{id}.bin")),
                size: if *id == "listener-runtime" { 0 } else { 10 },
                sha256: if *id == "listener-runtime" {
                    String::new()
                } else {
                    "c".repeat(64)
                },
                dependencies: vec![],
                restart: "module".into(),
                protocol: "desktop-ipc-v1".into(),
                summary: String::new(),
                changes: vec![],
            });
        }
        assert!(validate_compatible_manifest(&manifest).is_ok());

        let dependency_index = manifest
            .components
            .iter()
            .position(|component| component.id == "core")
            .expect("core component");
        manifest.components[dependency_index].dependencies =
            vec!["updater".into(), "updater".into()];
        assert!(validate_compatible_manifest(&manifest).is_err());
        manifest.components[dependency_index].dependencies = vec!["core".into()];
        assert!(validate_compatible_manifest(&manifest).is_err());
        manifest.components[dependency_index].dependencies = vec!["updater".into()];

        manifest.components[0].artifact = Some("core.exe:stream".into());
        assert!(validate_compatible_manifest(&manifest).is_err());
        manifest.components[0].artifact = Some("core.bin".into());
        manifest.components[0].manifest_asset = "core.manifest.json:stream".into();
        assert!(validate_compatible_manifest(&manifest).is_err());
        manifest.components[0].manifest_asset = "core.manifest.json".into();

        manifest.components[0].summary = "x".repeat(super::MAX_COMPATIBLE_SUMMARY_BYTES + 1);
        assert!(validate_compatible_manifest(&manifest).is_err());
        manifest.components[0].summary.clear();
        manifest.components[0].changes = vec!["x".repeat(super::MAX_COMPATIBLE_CHANGE_BYTES + 1)];
        assert!(validate_compatible_manifest(&manifest).is_err());
        manifest.components[0].changes.clear();
        manifest.components[0].dependencies =
            vec!["x".repeat(super::MAX_COMPATIBLE_DEPENDENCY_BYTES + 1)];
        assert!(validate_compatible_manifest(&manifest).is_err());
        manifest.components[0].dependencies.clear();

        manifest.components[0].size = super::MAX_UPDATE_ARTIFACT_BYTES + 1;
        assert!(validate_compatible_manifest(&manifest).is_err());
        manifest.components[0].size = 10;

        let mut invalid = manifest;
        invalid.components[1].release_tag = "module-supervisor-v1.0.0".into();
        assert!(validate_compatible_manifest(&invalid).is_err());
    }

    #[test]
    fn oldest_base_updates_only_the_updater_first() {
        let updates = vec![
            UpdateCandidate {
                module: "core".into(),
                installed: "0.0.0".into(),
                available: "2.0.0".into(),
                summary: String::new(),
                changes: vec![],
                dependencies: vec![],
                restart: "core".into(),
                artifact: "evohime-core.exe".into(),
                size: 1,
                sha256: "a".repeat(64),
                download_url: "https://github.com/example/core".into(),
            },
            UpdateCandidate {
                module: "updater".into(),
                installed: "0.0.0".into(),
                available: "2.0.0".into(),
                summary: String::new(),
                changes: vec![],
                dependencies: vec![],
                restart: "updater".into(),
                artifact: "evohime-updater.exe".into(),
                size: 1,
                sha256: "b".repeat(64),
                download_url: "https://github.com/example/updater".into(),
            },
        ];
        let selected = updater_first_if_required(updates, "0.0.0", "1.0.0", true).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].module, "updater");
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

    #[test]
    fn update_config_rejects_an_oversized_local_json_file() {
        let path = std::env::temp_dir().join(format!(
            "evohime-update-config-large-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        fs::write(&path, vec![b' '; super::MAX_LOCAL_JSON_BYTES + 1])
            .expect("write oversized update config");

        let error = read_update_config(&path).expect_err("oversized config must be rejected");
        let _ = fs::remove_file(&path);

        assert!(error.contains("read limit"));
    }

    #[test]
    fn github_token_resolution_uses_the_documented_precedence() {
        let config = serde_json::json!({"githubToken": "config_token_1234567890"});
        let token = resolve_github_token_with(
            &config,
            |name| match name {
                "EVOHIME_UPDATE_GITHUB_TOKEN" => Some("explicit_token_1234567890".to_owned()),
                "GH_TOKEN" => Some("ambient_token_1234567890".to_owned()),
                _ => None,
            },
            || Some("gh_token_1234567890".to_owned()),
        );
        assert_eq!(token.as_deref(), Some("explicit_token_1234567890"));

        let token =
            resolve_github_token_with(&config, |_| None, || Some("gh_token_1234567890".to_owned()));
        assert_eq!(token.as_deref(), Some("config_token_1234567890"));

        let token = resolve_github_token_with(
            &serde_json::json!({}),
            |name| (name == "GH_TOKEN").then(|| "ambient_token_1234567890".to_owned()),
            || Some("gh_token_1234567890".to_owned()),
        );
        assert_eq!(token.as_deref(), Some("ambient_token_1234567890"));

        let token = resolve_github_token_with(
            &serde_json::json!({}),
            |_| None,
            || Some("gh_token_1234567890".to_owned()),
        );
        assert_eq!(token.as_deref(), Some("gh_token_1234567890"));
    }

    #[test]
    fn github_token_normalization_rejects_invalid_values() {
        assert_eq!(normalize_github_token(Some(" too-short ")), None);
        assert_eq!(
            normalize_github_token(Some("token with spaces 1234567890")),
            None
        );
        assert_eq!(
            normalize_github_token(Some(" valid_token_1234567890 ")).as_deref(),
            Some("valid_token_1234567890")
        );
    }

    #[test]
    fn github_authorization_is_limited_to_api_host() {
        assert!(is_github_api_url(
            "https://api.github.com/repos/example/project/releases"
        ));
        assert!(!is_github_api_url(
            "https://github.com/example/project/releases/download/v1/file.zip"
        ));
        assert!(!is_github_api_url(
            "http://api.github.com/repos/example/project"
        ));

        let client = updater_http_client(Some("token_1234567890".to_owned())).expect("client");
        let api_request = client
            .get("https://api.github.com/repos/example/project/releases")
            .build()
            .expect("API request");
        assert_eq!(
            api_request
                .headers()
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer token_1234567890")
        );

        let asset_request = client
            .get("https://github.com/example/project/releases/download/v1/file.zip")
            .build()
            .expect("asset request");
        assert!(asset_request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .is_none());
    }

    #[test]
    fn release_asset_urls_are_limited_to_github_download_paths() {
        assert!(is_github_release_asset_url(
            "https://github.com/example/project/releases/download/v1/file.zip"
        ));
        assert!(!is_github_release_asset_url(
            "https://example.com/example/project/releases/download/v1/file.zip"
        ));
        assert!(!is_github_release_asset_url(
            "https://github.com/example/project/archive/v1/file.zip"
        ));
        assert!(!is_github_release_asset_url(
            "http://github.com/example/project/releases/download/v1/file.zip"
        ));
        assert!(!is_github_release_asset_url(
            "https://github.com:8443/example/project/releases/download/v1/file.zip"
        ));
        assert!(!is_github_release_asset_url(
            "https://user@github.com/example/project/releases/download/v1/file.zip"
        ));
    }

    #[test]
    fn redirects_are_limited_to_trusted_github_hosts() {
        assert!(is_trusted_github_url(
            &"https://api.github.com/repos/example/project"
                .parse()
                .unwrap()
        ));
        assert!(is_trusted_github_url(
            &"https://release-assets.githubusercontent.com/file"
                .parse()
                .unwrap()
        ));
        assert!(!is_trusted_github_url(
            &"https://github.com.evil.example/file".parse().unwrap()
        ));
        assert!(!is_trusted_github_url(
            &"http://github.com/file".parse().unwrap()
        ));
        assert!(!is_trusted_github_url(
            &"https://api.github.com/repos/example/project?redirect=evil"
                .parse()
                .unwrap()
        ));
    }

    #[test]
    fn runtime_manifest_rejects_duplicate_file_names() {
        let entry = || RuntimeReleaseEntry {
            name: "models/base.bin".into(),
            size: 1,
            sha256: "a".repeat(64),
        };
        let manifest = RuntimeReleaseManifest {
            schema: 1,
            module: "listener-runtime".into(),
            version: "1.0.0".into(),
            abi: serde_json::json!({}),
            files: vec![entry()],
            models: vec![entry()],
        };
        let error = validate_runtime_manifest(&manifest)
            .expect_err("duplicate runtime file names must be rejected");
        assert!(error.contains("повторная запись"));

        let mut safe_manifest = manifest;
        safe_manifest.models[0].name = "models/other.bin".into();
        assert!(validate_runtime_manifest(&safe_manifest).is_ok());
        safe_manifest.files[0].name = "models/base.bin:stream".into();
        assert!(validate_runtime_manifest(&safe_manifest).is_err());
        safe_manifest.files[0].name = "models/".into();
        assert!(validate_runtime_manifest(&safe_manifest).is_err());
    }

    #[test]
    fn updater_bootstrap_validates_new_worker_before_removing_backup() {
        let updater = Path::new(r"C:\Program Files\EvoHime\evohime-updater.exe");
        let updater_ui = Path::new(r"C:\Program Files\EvoHime\EvoHimeUpdater.exe");
        let staged = Path::new(
            r"C:\Users\Roman\AppData\Local\EvoHime\update-staging\evohime-updater.exe.next",
        );
        let backup =
            Path::new(r"C:\Users\Roman\AppData\Local\EvoHime\update-state\updater-previous.exe");
        let manifest = Path::new(r"C:\Program Files\EvoHime\evohime.components.json");
        let manifest_next = Path::new(r"C:\Program Files\EvoHime\evohime.components.json.next");
        let manifest_backup = Path::new(
            r"C:\Users\Roman\AppData\Local\EvoHime\update-state\components-previous.json",
        );
        let marker = Path::new(
            r"C:\Users\Roman\AppData\Local\EvoHime\update-state\updater-relaunch.pending",
        );
        let paths = UpdaterBootstrapPaths {
            updater,
            updater_ui,
            staged,
            backup,
            manifest,
            manifest_next,
            manifest_backup,
            marker,
            install_dir: Path::new(r"C:\Program Files\EvoHime"),
        };
        let script = updater_bootstrap_script(42, &paths);

        let verify = script
            .find("--check --install-dir")
            .expect("new worker check");
        let cleanup_backup = script.find("del /Q \"%BACKUP%\"").expect("backup cleanup");
        assert!(verify < cleanup_backup);
        assert!(script.contains(":restore_manifest"));
        assert!(script.contains("move /Y \"%BACKUP%\" \"%UPDATER%\""));
        assert!(script.contains("Program Files"));
    }
}
