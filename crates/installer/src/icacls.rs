//! Фикс прав на `data\` перед `initdb` (раздел III/VI плана): `initdb`
//! требует отсутствия наследуемых прав на каталог данных, иначе падает с
//! ошибкой "insecure permissions" на Windows.

use std::path::Path;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum IcaclsError {
    #[error("icacls {args} exited with status {status}: {stderr}")]
    CommandFailed {
        args: String,
        status: std::process::ExitStatus,
        stderr: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Отключает наследование прав для `dir` и выдаёт текущему пользователю
/// наследуемый полный доступ к файлам и каталогам (`icacls dir /inheritance:r`
/// затем `icacls dir /grant:r "%USERNAME%":(OI)(CI)F`).
pub async fn restrict_to_current_user(dir: &Path) -> Result<(), IcaclsError> {
    run_icacls(dir, &["/inheritance:r"]).await?;

    let username = std::env::var("USERNAME").unwrap_or_else(|_| "%USERNAME%".to_string());
    let grant_arg = format!("{username}:(OI)(CI)F");
    run_icacls(dir, &["/grant:r", &grant_arg]).await?;

    Ok(())
}

async fn run_icacls(dir: &Path, extra_args: &[&str]) -> Result<(), IcaclsError> {
    let dir_str = dir.display().to_string();
    let output = Command::new("icacls")
        .arg(&dir_str)
        .args(extra_args)
        .output()
        .await?;

    if !output.status.success() {
        return Err(IcaclsError::CommandFailed {
            args: format!("{dir_str} {}", extra_args.join(" ")),
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

