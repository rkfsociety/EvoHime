#![cfg(windows)]

use evohime_win_support::{
    is_process_alive, processes_in_directory, terminate_and_wait, terminate_process,
};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if is_process_alive(self.0.id()) {
            let _ = terminate_process(self.0.id());
        }
        let _ = self.0.wait();
    }
}

#[test]
fn finds_and_terminates_only_processes_executing_inside_install_tree() {
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    let bin_dir = install_dir.join("versions").join("current");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let system_root = std::env::var_os("SystemRoot").unwrap();
    let system_cmd = std::path::PathBuf::from(system_root)
        .join("System32")
        .join("cmd.exe");
    let installed_cmd = bin_dir.join("evohime-server.exe");
    std::fs::copy(&system_cmd, &installed_cmd).unwrap();

    let mut inside = ChildGuard(spawn_waiting_cmd(&installed_cmd));
    let mut outside = ChildGuard(spawn_waiting_cmd(&system_cmd));
    std::thread::sleep(Duration::from_millis(250));

    let found = processes_in_directory(&install_dir, std::process::id()).unwrap();

    assert!(found.iter().any(|process| process.pid == inside.0.id()));
    assert!(!found.iter().any(|process| process.pid == outside.0.id()));

    let excluded = processes_in_directory(&install_dir, inside.0.id()).unwrap();
    assert!(!excluded.iter().any(|process| process.pid == inside.0.id()));

    terminate_and_wait(&found, Duration::from_secs(5)).unwrap();

    assert!(!is_process_alive(inside.0.id()));
    assert!(is_process_alive(outside.0.id()));
    inside.0.wait().unwrap();
    outside.0.kill().unwrap();
    outside.0.wait().unwrap();
}

fn spawn_waiting_cmd(executable: &std::path::Path) -> Child {
    Command::new(executable)
        .args(["/d", "/s", "/c", "ping 127.0.0.1 -n 30 >nul"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}
