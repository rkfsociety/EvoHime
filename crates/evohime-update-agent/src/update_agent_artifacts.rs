use super::*;

pub(super) fn cleanup_completed_staging(staging: &Path, keep_for_bootstrap: bool) {
    if !keep_for_bootstrap {
        let _ = fs::remove_dir_all(staging);
    }
}

pub(super) fn apply_listener_runtime_if_needed(
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

pub(super) fn progress_percent(done: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    ((done.saturating_mul(100) / total).min(99)) as u8
}

pub(super) fn download_verified_file(
    client: &UpdaterHttpClient,
    update: &UpdateCandidate,
    target: &Path,
    on_progress: impl Fn(u64),
) -> Result<(), String> {
    let mut last_error = String::new();
    for attempt in 1..=3 {
        match download_verified_file_once(client, update, target, &on_progress) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = error;
                let _ = fs::remove_file(target.with_extension("part"));
            }
        }
        if attempt < 3 {
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    Err(format!("{} (после 3 попыток)", last_error))
}

pub(super) fn download_verified_file_once(
    client: &UpdaterHttpClient,
    update: &UpdateCandidate,
    target: &Path,
    on_progress: &dyn Fn(u64),
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

pub(super) fn extract_ui_bundle(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let result = extract_ui_bundle_inner(archive_path, destination);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

pub(super) fn extract_ui_bundle_inner(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), String> {
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

pub(super) fn extract_shell_host(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let result = extract_shell_host_inner(archive_path, destination);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

pub(super) fn extract_shell_host_inner(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), String> {
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

pub(super) fn extract_updater_package(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), String> {
    let result = extract_updater_package_inner(archive_path, destination);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

pub(super) fn extract_updater_package_inner(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), String> {
    let archive_file = fs::File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(archive_file).map_err(|error| error.to_string())?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("updater: updater package содержит слишком много записей".to_owned());
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
            return Err("updater: updater package содержит symlink".to_owned());
        }
        let relative = entry
            .enclosed_name()
            .and_then(|path| normalize_archive_path(&path))
            .ok_or_else(|| "updater: updater package содержит небезопасный путь".to_owned())?;
        if !extracted_paths.insert(relative.to_owned()) {
            return Err("updater: updater package содержит повторяющийся путь".to_owned());
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
                .ok_or_else(|| "updater: updater package превышает лимит распаковки".to_owned())?;
            if entry.size() > remaining {
                return Err("updater: updater package превышает лимит распаковки".to_owned());
            }
            let written = copy_reader_bounded(&mut entry, &mut file, remaining)?;
            if written != entry.size() {
                return Err("updater: updater package содержит усечённый файл".to_owned());
            }
            extracted_bytes = extracted_bytes
                .checked_add(written)
                .ok_or_else(|| "updater: размер распаковки переполнен".to_owned())?;
        }
    }
    if !destination.join("evohime-updater.exe").is_file()
        || !destination
            .join("updater")
            .join("EvoHimeUpdater.exe")
            .is_file()
        || !destination
            .join("updater")
            .join("resources")
            .join("app.asar")
            .is_file()
    {
        return Err("updater: updater package не содержит полный worker и Electron UI".to_owned());
    }
    validate_pe_image(&destination.join("evohime-updater.exe"))
        .map_err(|error| format!("updater: worker PE validation failed: {error}"))?;
    Ok(())
}

pub(super) const MAX_ARCHIVE_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;
pub(super) const MAX_ARCHIVE_ENTRIES: usize = 4096;

pub(super) fn copy_reader_bounded<R: Read, W: Write>(
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

pub(super) fn normalize_archive_path(path: &Path) -> Option<PathBuf> {
    if path.to_string_lossy().contains('\\') {
        return None;
    }
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

pub(super) fn apply_listener_runtime(
    client: &UpdaterHttpClient,
    update: &UpdateCandidate,
    data_dir: &Path,
    progress: &dyn Fn(&str, u8),
) -> Result<(), String> {
    let staging = data_dir.join("update-staging").join("listener-runtime");
    let result = apply_listener_runtime_inner(client, update, data_dir, progress);
    cleanup_failed_staging(&staging, result)
}

pub(super) fn apply_listener_runtime_inner(
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

pub(super) fn cleanup_failed_staging(
    staging: &Path,
    result: Result<(), String>,
) -> Result<(), String> {
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

pub(super) fn safe_runtime_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
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
