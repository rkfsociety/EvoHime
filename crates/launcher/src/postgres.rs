//! Управление портативным PostgreSQL, распакованным Installer'ом в
//! `install_dir/pg16` (`bin/`, `lib/`, `share/`, `data/`) — тонкая обёртка
//! над `initdb.exe`/`pg_ctl.exe`, переиспользуемая и Installer'ом
//! (первичная инициализация), и Launcher'ом (запуск при каждом старте
//! приложения, остановка по команде "Stop").
//!
//! Портативная сборка (zonky embedded-postgres-binaries) содержит только
//! `postgres.exe`/`initdb.exe`/`pg_ctl.exe` — без клиентских утилит
//! (`createdb`, `psql`, `pg_dump`), поэтому создание базы данных здесь
//! делается SQL-запросом через `sqlx`, а не вызовом `createdb.exe`.
//!
//! `pg_ctl start -w` сам дожидается готовности сервера принимать
//! подключения — отдельный TCP-поллинг здесь не нужен. `postgres.exe`,
//! которого запускает `pg_ctl`, не является дочерним процессом вызывающего
//! (ни Installer'а, ни Launcher'а) — он не будет ни убит вместе с ними, ни
//! удерживать их от завершения.

use std::path::Path;
use tokio::fs;
use tokio::process::Command;

/// Порт нашего портативного кластера — не 5432, чтобы не конфликтовать с
/// уже установленным у пользователя системным PostgreSQL (если есть).
pub const PG_PORT: u16 = 55432;

#[derive(Debug, thiserror::Error)]
pub enum PgError {
    #[error("{tool} {args} exited with status {status}: {stderr}")]
    CommandFailed {
        tool: String,
        args: String,
        status: std::process::ExitStatus,
        stderr: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// Инициализирует новый кластер в `data_dir` (должен быть пуст/не
/// существовать) под пользователем `user` с паролем `password`.
///
/// `initdb` на Windows создаёт суперпользователя с именем, совпадающим с
/// именем текущего пользователя Windows, а не `postgres` — вызывающая
/// сторона должна передавать именно его (см. `pg_auth.rs`).
pub async fn initdb(
    pg_bin_dir: &Path,
    data_dir: &Path,
    user: &str,
    password: &str,
) -> Result<(), PgError> {
    let pwfile = pg16_root(pg_bin_dir).join(".initdb-pwfile.tmp");
    fs::write(&pwfile, password).await?;

    let result = run_pg_tool(
        pg_bin_dir,
        "initdb",
        &[
            "-D",
            &data_dir.display().to_string(),
            "-U",
            user,
            "--pwfile",
            &pwfile.display().to_string(),
            "-E",
            "UTF8",
        ],
    )
    .await;

    let _ = fs::remove_file(&pwfile).await;
    result
}

/// Запускает `postgres.exe` через `pg_ctl start -w` и дожидается, пока он
/// начнёт принимать подключения (или вернёт ошибку по таймауту).
pub async fn start(pg_bin_dir: &Path, data_dir: &Path, port: u16) -> Result<(), PgError> {
    let log_path = pg16_root(pg_bin_dir).join("postgres.log");
    run_pg_tool(
        pg_bin_dir,
        "pg_ctl",
        &[
            "start",
            "-D",
            &data_dir.display().to_string(),
            "-w",
            "-t",
            "30",
            "-l",
            &log_path.display().to_string(),
            "-o",
            &format!("-p {port}"),
        ],
    )
    .await
}

/// Останавливает кластер (`pg_ctl stop -m fast -w`).
pub async fn stop(pg_bin_dir: &Path, data_dir: &Path) -> Result<(), PgError> {
    run_pg_tool(
        pg_bin_dir,
        "pg_ctl",
        &[
            "stop",
            "-D",
            &data_dir.display().to_string(),
            "-m",
            "fast",
            "-w",
            "-t",
            "30",
        ],
    )
    .await
}

/// Создаёт базу `db_name`, если её ещё нет — подключается к служебной базе
/// `postgres` (та всегда существует после `initdb`) и выполняет
/// `CREATE DATABASE` напрямую, поскольку портативная сборка не включает
/// `createdb.exe`. Код ошибки `duplicate_database` (`42501`... на самом
/// деле `42P04`) считается успехом — гонки здесь нет, но повторный запуск
/// Installer'а (переустановка) не должен падать на уже существующей базе.
pub async fn create_database_if_missing(
    user: &str,
    password: &str,
    port: u16,
    db_name: &str,
) -> Result<(), PgError> {
    let admin_dsn = crate::dsn::build_dsn(user, password, "127.0.0.1", port, "postgres");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&admin_dsn)
        .await?;

    let quoted = db_name.replace('"', "\"\"");
    let query = format!("CREATE DATABASE \"{quoted}\"");
    match sqlx::query(&query).execute(&pool).await {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("42P04") => Ok(()),
        Err(err) => Err(PgError::Sqlx(err)),
    }
}

/// `true`, если что-то слушает `port` и это действительно наш
/// `postgres.exe` из `pg_bin_dir` (а не случайно занявший порт чужой
/// процесс) — использует уже существующий win-support резолвер PID/порт.
#[cfg(windows)]
pub fn is_running(pg_bin_dir: &Path, port: u16) -> bool {
    let Some(pid) = evohime_win_support::find_pid_listening_on_port(port) else {
        return false;
    };
    let Some(exe_path) = evohime_win_support::resolve_process_exe_path(pid) else {
        return false;
    };
    is_expected_postgres_executable(&exe_path, pg_bin_dir)
}

#[cfg(windows)]
fn is_expected_postgres_executable(exe_path: &Path, pg_bin_dir: &Path) -> bool {
    let expected = pg_bin_dir.join("postgres.exe");
    exe_path
        .canonicalize()
        .ok()
        .zip(expected.canonicalize().ok())
        .is_some_and(|(actual, expected)| actual == expected)
}

#[cfg(not(windows))]
pub fn is_running(_pg_bin_dir: &Path, _port: u16) -> bool {
    false
}

/// Запускает кластер, только если он ещё не поднят — вызывается
/// Launcher'ом при каждом старте приложения.
pub async fn ensure_started(pg_bin_dir: &Path, data_dir: &Path, port: u16) -> Result<(), PgError> {
    if is_running(pg_bin_dir, port) {
        return Ok(());
    }
    start(pg_bin_dir, data_dir, port).await
}

fn pg16_root(pg_bin_dir: &Path) -> std::path::PathBuf {
    pg_bin_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| pg_bin_dir.to_path_buf())
}

async fn run_pg_tool(pg_bin_dir: &Path, tool: &str, args: &[&str]) -> Result<(), PgError> {
    let exe = pg_bin_dir.join(format!("{tool}.exe"));
    let output = Command::from(build_pg_command(&exe, args)).output().await?;

    if !output.status.success() {
        return Err(PgError::CommandFailed {
            tool: tool.to_string(),
            args: args.join(" "),
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

fn build_pg_command(exe: &Path, args: &[&str]) -> std::process::Command {
    let mut command = std::process::Command::new(exe);
    command.env("LC_ALL", "C").args(args);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg16_root_is_bin_parent() {
        let bin = Path::new(r"C:\EvoHime\pg16\bin");
        assert_eq!(pg16_root(bin), Path::new(r"C:\EvoHime\pg16"));
    }

    #[test]
    fn is_running_false_when_nothing_listens() {
        // Port 1 is a reserved low port nothing in CI binds to.
        assert!(!is_running(Path::new(r"C:\nonexistent\pg16\bin"), 1));
    }

    #[test]
    fn expected_postgres_executable_requires_exact_bin_directory() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("EvoHime").join("pg16").join("bin");
        let foreign_bin = root.path().join("foreign").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&foreign_bin).unwrap();
        std::fs::write(bin.join("postgres.exe"), b"expected").unwrap();
        std::fs::write(foreign_bin.join("postgres.exe"), b"foreign").unwrap();

        assert!(is_expected_postgres_executable(
            &bin.join("postgres.exe"),
            &bin
        ));
        assert!(!is_expected_postgres_executable(
            &foreign_bin.join("postgres.exe"),
            &bin
        ));
    }

    #[test]
    fn pg_tool_command_sets_c_locale_without_changing_parent_environment() {
        let parent_locale = std::env::var_os("LC_ALL");
        let command = build_pg_command(Path::new(r"C:\EvoHime\pg16\bin\initdb.exe"), &[]);

        let locale = command
            .get_envs()
            .find(|(key, _)| key.to_string_lossy() == "LC_ALL")
            .and_then(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()));

        assert_eq!(locale.as_deref(), Some("C"));
        assert_eq!(std::env::var_os("LC_ALL"), parent_locale);
    }
}
