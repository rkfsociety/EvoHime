//! Фикс прав на `data\` перед `initdb` (раздел III/VI плана): `initdb`
//! требует отсутствия наследуемых прав на каталог данных, иначе падает с
//! ошибкой "insecure permissions" на Windows.

use std::path::Path;
use tokio::process::Command;

use evohime_launcher::observed_command::{run_observed_command, CommandEvent};

#[derive(Debug, thiserror::Error)]
pub enum IcaclsError {
    #[error("icacls {args} exited with status {status}; stdout: {stdout}; stderr: {stderr}")]
    CommandFailed {
        args: String,
        status: std::process::ExitStatus,
        stdout: String,
        stderr: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
/// Отключает наследование прав для `dir` и выдаёт текущему пользователю
/// наследуемый полный доступ к файлам и каталогам (`icacls dir /inheritance:r`
/// затем `icacls dir /grant:r "%USERNAME%":(OI)(CI)F`).
pub async fn restrict_to_current_user(dir: &Path) -> Result<(), IcaclsError> {
    restrict_to_current_user_observed(dir, &mut |_| {}).await
}

pub async fn restrict_to_current_user_observed<F>(
    dir: &Path,
    observer: &mut F,
) -> Result<(), IcaclsError>
where
    F: FnMut(CommandEvent),
{
    run_icacls(dir, &["/inheritance:r"], observer).await?;

    let username = std::env::var("USERNAME").unwrap_or_else(|_| "%USERNAME%".to_string());
    let grant_arg = format!("{username}:(OI)(CI)F");
    run_icacls(dir, &["/grant:r", &grant_arg], observer).await?;

    Ok(())
}

/// Возвращает наследуемые разрешения дереву незавершённой установки,
/// которое будет немедленно удалено. Для завершённой установки эта
/// функция вызываться не должна.
pub async fn restore_deletable_permissions(dir: &Path) -> Result<(), IcaclsError> {
    restore_deletable_permissions_observed(dir, &mut |_| {}).await
}

pub async fn restore_deletable_permissions_observed<F>(
    dir: &Path,
    observer: &mut F,
) -> Result<(), IcaclsError>
where
    F: FnMut(CommandEvent),
{
    let username = std::env::var("USERNAME").unwrap_or_else(|_| "%USERNAME%".to_string());
    let grant_arg = format!("{username}:(OI)(CI)F");
    run_icacls(dir, &["/grant:r", &grant_arg, "/T", "/C", "/Q"], observer).await?;
    run_icacls(dir, &["/reset", "/T", "/C", "/Q"], observer).await?;
    Ok(())
}

async fn run_icacls<F>(dir: &Path, extra_args: &[&str], observer: &mut F) -> Result<(), IcaclsError>
where
    F: FnMut(CommandEvent),
{
    let dir_str = dir.display().to_string();
    let mut command = Command::new("icacls");
    command.arg(&dir_str).args(extra_args);
    let safe_display = format!(
        "icacls.exe {:?} {}",
        dir_str,
        extra_args
            .iter()
            .map(|arg| format!("{arg:?}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let output = run_observed_command(command, safe_display, observer).await?;

    if !output.status.success() {
        return Err(IcaclsError::CommandFailed {
            args: format!("{dir_str} {}", extra_args.join(" ")),
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        });
    }
    Ok(())
}
