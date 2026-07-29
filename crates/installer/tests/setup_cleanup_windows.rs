#![cfg(windows)]

use evohime_installer::{
    clear_dirty_installation, clear_dirty_installation_safely, DirtyCleanupError,
};
use evohime_win_support::{is_process_alive, terminate_process};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static POSTGRES_PORT_LOCK: Mutex<()> = Mutex::new(());

const POSTGRES_STUB_SOURCE: &str = r#"
use std::env;
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    let executable = env::current_exe().unwrap();
    let tool = executable.file_name().unwrap().to_string_lossy();
    let args: Vec<String> = env::args().collect();

    if tool.eq_ignore_ascii_case("pg_ctl.exe") {
        let data_index = args.iter().position(|arg| arg == "-D").unwrap() + 1;
        let data = PathBuf::from(&args[data_index]);
        fs::write(data.join("stop"), b"").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !data.join("exited").exists() {
            assert!(Instant::now() < deadline, "postgres stub did not stop");
            thread::sleep(Duration::from_millis(25));
        }
        thread::sleep(Duration::from_millis(250));
        return;
    }

    let data = PathBuf::from(&args[1]);
    let listener = TcpListener::bind(("127.0.0.1", 55432)).unwrap();
    fs::write(data.join("ready"), b"").unwrap();
    while !data.join("stop").exists() {
        thread::sleep(Duration::from_millis(25));
    }
    drop(listener);
    fs::write(data.join("exited"), b"").unwrap();
}
"#;

const RESIDUAL_PROCESS_SOURCE: &str = r#"
use std::thread;
use std::time::Duration;

fn main() {
    loop {
        thread::sleep(Duration::from_secs(30));
    }
}
"#;

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if is_process_alive(self.0.id()) {
            let _ = terminate_process(self.0.id());
        }
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn removes_dirty_installation_before_continuing() {
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    tokio::fs::create_dir_all(&install_dir).await.unwrap();
    tokio::fs::write(install_dir.join("partial.bin"), b"partial")
        .await
        .unwrap();

    assert!(clear_dirty_installation(&install_dir).await.unwrap());
    assert!(!install_dir.exists());
}

#[tokio::test]
async fn reports_error_when_dirty_installation_cannot_be_removed() {
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    tokio::fs::create_dir_all(&install_dir).await.unwrap();
    let locked_path = install_dir.join("locked.bin");
    let locked = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .share_mode(0)
        .open(&locked_path)
        .unwrap();

    let result = clear_dirty_installation(&install_dir).await;

    assert!(result.is_err());
    assert!(install_dir.exists());
    drop(locked);
}

#[tokio::test]
async fn stops_verified_portable_postgres_before_removing_dirty_installation() {
    let _port_guard = POSTGRES_PORT_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    let pg_bin_dir = install_dir.join("pg16").join("bin");
    let pg_data_dir = install_dir.join("pg16").join("data");
    tokio::fs::create_dir_all(&pg_bin_dir).await.unwrap();
    tokio::fs::create_dir_all(&pg_data_dir).await.unwrap();
    compile_postgres_stubs(&pg_bin_dir);

    let mut postgres = spawn_postgres_stub(&pg_bin_dir, &pg_data_dir);
    wait_until_ready(&pg_data_dir);

    let result = clear_dirty_installation_safely(&install_dir).await;

    assert!(result.unwrap());
    assert!(!install_dir.exists());
    assert!(postgres.wait().unwrap().success());
}

#[tokio::test]
async fn reports_verified_portable_postgres_shutdown_failure() {
    let _port_guard = POSTGRES_PORT_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    let pg_bin_dir = install_dir.join("pg16").join("bin");
    let pg_data_dir = install_dir.join("pg16").join("data");
    tokio::fs::create_dir_all(&pg_bin_dir).await.unwrap();
    tokio::fs::create_dir_all(&pg_data_dir).await.unwrap();
    compile_postgres_stubs(&pg_bin_dir);
    compile_source(
        &pg_bin_dir.join("pg_ctl.exe"),
        r#"fn main() { eprintln!("intentional stop failure"); std::process::exit(9); }"#,
    );

    let mut postgres = spawn_postgres_stub(&pg_bin_dir, &pg_data_dir);
    wait_until_ready(&pg_data_dir);

    let result = clear_dirty_installation_safely(&install_dir).await;

    assert!(matches!(result, Err(DirtyCleanupError::PostgresStop(_))));
    assert!(install_dir.exists());
    postgres.kill().unwrap();
    postgres.wait().unwrap();
}

#[tokio::test]
async fn closes_portless_residual_processes_only_inside_dirty_installation() {
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    let installed_exe = install_dir
        .join("versions")
        .join("current")
        .join("evohime-server.exe");
    std::fs::create_dir_all(installed_exe.parent().unwrap()).unwrap();
    compile_source(&installed_exe, RESIDUAL_PROCESS_SOURCE);

    let external_dir = root.path().join("external");
    let external_exe = external_dir.join("evohime-server.exe");
    std::fs::create_dir_all(&external_dir).unwrap();
    compile_source(&external_exe, RESIDUAL_PROCESS_SOURCE);

    let mut inside = ChildGuard(spawn_residual_process(&installed_exe));
    let mut outside = ChildGuard(spawn_residual_process(&external_exe));
    std::thread::sleep(Duration::from_millis(250));

    let result = clear_dirty_installation_safely(&install_dir).await;

    assert!(result.unwrap());
    assert!(!install_dir.exists());
    assert!(!is_process_alive(inside.0.id()));
    assert!(is_process_alive(outside.0.id()));
    inside.0.wait().unwrap();
    outside.0.kill().unwrap();
    outside.0.wait().unwrap();
}

#[tokio::test]
async fn leaves_processes_in_completed_installation_untouched() {
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    let installed_exe = install_dir
        .join("versions")
        .join("current")
        .join("evohime-server.exe");
    std::fs::create_dir_all(installed_exe.parent().unwrap()).unwrap();
    compile_source(&installed_exe, RESIDUAL_PROCESS_SOURCE);
    let config_dir = install_dir.join("launcher-data");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.json"), b"{}").unwrap();
    std::fs::write(install_dir.join(".setup_complete"), b"").unwrap();
    let mut inside = ChildGuard(spawn_residual_process(&installed_exe));
    std::thread::sleep(Duration::from_millis(250));

    let result = clear_dirty_installation_safely(&install_dir).await;

    assert!(!result.unwrap());
    assert!(install_dir.exists());
    assert!(is_process_alive(inside.0.id()));
    inside.0.kill().unwrap();
    inside.0.wait().unwrap();
}

fn compile_postgres_stubs(pg_bin_dir: &Path) {
    let postgres = pg_bin_dir.join("postgres.exe");
    compile_source(&postgres, POSTGRES_STUB_SOURCE);
    std::fs::copy(postgres, pg_bin_dir.join("pg_ctl.exe")).unwrap();
}

fn compile_source(output: &Path, source: &str) {
    let source_path = output.with_extension("rs");
    std::fs::write(&source_path, source).unwrap();
    let status = Command::new("rustc")
        .arg(&source_path)
        .arg("-o")
        .arg(output)
        .status()
        .unwrap();
    assert!(status.success(), "failed to compile PostgreSQL test stub");
    std::fs::remove_file(source_path).unwrap();
}

fn spawn_postgres_stub(pg_bin_dir: &Path, pg_data_dir: &Path) -> Child {
    Command::new(pg_bin_dir.join("postgres.exe"))
        .arg(pg_data_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn spawn_residual_process(executable: &Path) -> Child {
    Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn wait_until_ready(pg_data_dir: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !pg_data_dir.join("ready").exists() {
        assert!(Instant::now() < deadline, "postgres stub did not start");
        std::thread::sleep(Duration::from_millis(25));
    }
}
