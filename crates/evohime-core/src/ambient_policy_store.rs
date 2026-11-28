//! Файлы политики и намерения слушания в data dir ядра (план 04.2).
//!
//! Сама политика и её валидация — контракт `evohime-listener-contract`;
//! здесь только хранилище: ограниченное чтение, атомарная запись с
//! owner-only ACL и правила отказа (fail-safe в пользу тишины). SQL
//! ambient-хранилища живёт в `evohime_local_storage::ambient_store`, а
//! retention-правила — в `evohime_listener_contract::clock`.

use std::{
    io::Read,
    path::{Path, PathBuf},
};

use evohime_listener_contract::{
    retention_days_from_env, AmbientControl, AmbientPolicy, DEFAULT_RETENTION_DAYS,
    MAX_RETENTION_DAYS,
};

/// Имя файла политики в data dir.
pub const POLICY_FILE_NAME: &str = "ambient-policy.json";

const MAX_POLICY_BYTES: usize = 64 * 1024;

/// Returns the persistent ambient-policy file under the application data directory.
pub fn policy_path(data_dir: &Path) -> PathBuf {
    data_dir.join(POLICY_FILE_NAME)
}

/// Политика по умолчанию: слушать не запрещено политикой, retention — из
/// окружения. Микрофон при этом всё равно закрыт, пока `microphone_listen`
/// не переведён в `allow`.
pub fn default_policy() -> AmbientPolicy {
    AmbientPolicy {
        retention_days: retention_days_from_env(),
        ..AmbientPolicy::default()
    }
}

/// Fail-safe в пользу тишины: политику, которую не удалось прочитать,
/// нельзя молча заменить на «слушать всё».
pub fn paused_default_policy() -> AmbientPolicy {
    AmbientPolicy {
        paused: true,
        ..default_policy()
    }
}

/// Читает политику из data dir.
///
/// Отсутствующий файл означает «ещё не настраивали» — дефолт. Повреждённый
/// или невалидный файл означает «неизвестно, чего хотел пользователь» — тот
/// же дефолт, но с включённой паузой.
pub fn load_policy(data_dir: &Path) -> AmbientPolicy {
    let path = policy_path(data_dir);
    let bytes = match read_bounded_policy(&path) {
        Ok(bytes) => bytes,
        Err(_) if path.is_file() => return paused_default_policy(),
        Err(_) => return default_policy(),
    };
    let Ok(policy) = serde_json::from_slice::<AmbientPolicy>(&bytes) else {
        return paused_default_policy();
    };
    if policy.validate().is_err() {
        return paused_default_policy();
    }
    policy
}

fn read_bounded_policy(path: &Path) -> std::io::Result<Vec<u8>> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_POLICY_BYTES as u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "ambient policy exceeds the read limit",
        ));
    }
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(MAX_POLICY_BYTES.min(16 * 1024));
    file.take((MAX_POLICY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_POLICY_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "ambient policy exceeds the read limit",
        ));
    }
    Ok(bytes)
}

/// Пишет политику атомарно: временный файл, `sync_all`, owner-only ACL,
/// `rename`, ACL ещё раз (rename не переносит дескриптор, если целевой файл
/// уже существовал).
///
/// Невалидная политика не сохраняется: иначе следующий старт прочитал бы её
/// как повреждённую и молча встал на паузу.
pub fn save_policy(data_dir: &Path, policy: &AmbientPolicy) -> Result<(), String> {
    policy.validate().map_err(|error| error.to_string())?;
    let path = policy_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(policy).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    if let Err(error) = harden(&temporary) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = replace_file(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    harden(&path)?;
    if let Some(parent) = path.parent() {
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    Ok(())
}

/// Имя файла с намерением пользователя по слушанию.
pub const CONTROL_FILE_NAME: &str = "ambient-control.json";

/// Returns the persistent listening-control file under the application data directory.
pub fn control_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CONTROL_FILE_NAME)
}

/// Читает намерение. Отсутствующий или повреждённый файл означает
/// «выключено»: молчание безопаснее догадки.
pub fn load_control(data_dir: &Path) -> AmbientControl {
    std::fs::read(control_path(data_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<AmbientControl>(&bytes).ok())
        .unwrap_or_default()
}

/// Пишет намерение тем же атомарным путём, что и политику: временный файл,
/// `sync_all`, owner-only ACL, замена.
pub fn save_control(data_dir: &Path, control: &AmbientControl) -> Result<(), String> {
    let path = control_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(control).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    if let Err(error) = harden(&temporary) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = replace_file(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    harden(&path)
}

/// Каталог данных Core: тот же, из которого поднимается всё остальное
/// ambient-состояние.
pub fn data_dir() -> PathBuf {
    crate::get_data_directory()
}

#[cfg(windows)]
fn harden(path: &Path) -> Result<(), String> {
    evohime_desktop_ipc::windows_security::harden_file_owner_only(path)
        .map_err(|error| error.to_string())
}

/// Atomically replaces the destination on every supported platform.
///
/// `std::fs::rename` replaces an existing destination on Unix, but Windows
/// rejects that case. `MoveFileExW(REPLACE_EXISTING)` preserves the atomic
/// replacement contract needed for a policy update.
#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::rename(from, to).map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn harden(_: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_listener_contract::QuietHours;

    #[test]
    fn policy_round_trips_through_the_data_directory() {
        let directory = tempfile::tempdir().expect("temp dir");
        assert_eq!(load_policy(directory.path()), default_policy());
        let policy = AmbientPolicy {
            paused: false,
            quiet_hours: vec![QuietHours::new(23 * 60, 7 * 60).unwrap()],
            process_blocklist: vec!["bank*.exe".to_owned()],
            window_title_blocklist: Vec::new(),
            retention_days: 14,
            voice_commands: true,
            voice_commands_autorun: false,
        };
        save_policy(directory.path(), &policy).expect("policy saves");
        assert_eq!(load_policy(directory.path()), policy);

        // Обновление существующего файла должно сохранять атомарность и на
        // Windows, где обычный std::fs::rename не заменяет destination.
        let updated = AmbientPolicy {
            paused: true,
            ..policy
        };
        save_policy(directory.path(), &updated).expect("policy updates");
        assert_eq!(load_policy(directory.path()), updated);
    }

    #[test]
    fn a_corrupted_policy_reads_back_as_a_paused_default() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(policy_path(directory.path()), b"{ not json").expect("corrupt file");
        let loaded = load_policy(directory.path());
        assert!(
            loaded.paused,
            "повреждённая политика обязана вставать на паузу"
        );
        assert_eq!(loaded, paused_default_policy());

        // Синтаксически валидный, но запрещённый контрактом файл — тот же
        // случай: неизвестно, чего хотел пользователь.
        std::fs::write(
            policy_path(directory.path()),
            br#"{"paused":false,"retention_days":4000}"#,
        )
        .expect("invalid policy writes");
        assert!(load_policy(directory.path()).paused);
    }

    #[test]
    fn an_oversized_policy_fails_safe_without_full_read() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            policy_path(directory.path()),
            vec![b'x'; MAX_POLICY_BYTES + 1],
        )
        .expect("oversized policy writes");

        assert_eq!(load_policy(directory.path()), paused_default_policy());
    }

    #[test]
    fn an_invalid_policy_is_never_persisted() {
        let directory = tempfile::tempdir().expect("temp dir");
        let policy = AmbientPolicy {
            retention_days: MAX_RETENTION_DAYS + 1,
            ..AmbientPolicy::default()
        };
        assert!(save_policy(directory.path(), &policy).is_err());
        assert!(!policy_path(directory.path()).exists());
    }

    #[cfg(windows)]
    #[test]
    fn the_written_policy_file_is_owner_only() {
        let directory = tempfile::tempdir().expect("temp dir");
        save_policy(directory.path(), &default_policy()).expect("policy saves");
        let sid = evohime_desktop_ipc::windows_security::current_user_sid().expect("sid");
        let dacl =
            evohime_desktop_ipc::windows_security::file_dacl_sddl(&policy_path(directory.path()))
                .expect("DACL reads");
        let expected = evohime_desktop_ipc::windows_security::normalize_dacl_sddl(&format!(
            "D:P(A;;FA;;;{sid})(A;;FA;;;SY)"
        ))
        .expect("expected DACL normalizes");
        assert_eq!(dacl, expected);
    }
}
