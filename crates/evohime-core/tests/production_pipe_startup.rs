#![cfg(windows)]

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Exercise the real entry point in an isolated directory, without touching
/// the installed client or mutating the test runner's environment.
fn assert_startup_rejected(context: Option<&str>, dev_mode: Option<&str>) {
    let directory = tempfile::tempdir().unwrap();
    let context_path = directory.path().join("session.json");
    let mut command = Command::new(env!("CARGO_BIN_EXE_evohime-core"));
    command
        .env("EVOHIME_DATA_DIR", directory.path())
        .env_remove("EVOHIME_LAUNCH_CONTEXT")
        .env_remove("EVOHIME_DEV_MODE")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(context) = context {
        std::fs::write(&context_path, context).unwrap();
        command.env("EVOHIME_LAUNCH_CONTEXT", &context_path);
    }
    if let Some(value) = dev_mode {
        command.env("EVOHIME_DEV_MODE", value);
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Core did not reject unsafe pipe startup within 10 seconds");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("launch context failed"));
    // Storage is opened before the pipe: its absence proves refusal happened
    // before either runtime initialization or pipe creation.
    assert!(!directory.path().join("events.db").exists());
}

#[test]
fn production_without_context_never_starts_pipe() {
    for dev_mode in [None, Some(""), Some("0"), Some("true"), Some("1 ")] {
        assert_startup_rejected(None, dev_mode);
    }
}

#[test]
fn production_with_unbound_context_never_starts_pipe() {
    let context =
        evohime_desktop_ipc::session::LaunchContext::generate(String::new(), String::new(), 0)
            .unwrap();
    assert_startup_rejected(Some(&serde_json::to_string(&context).unwrap()), None);
}

#[test]
fn malformed_explicit_context_never_falls_back_to_developer_pipe() {
    for dev_mode in [None, Some("1")] {
        assert_startup_rejected(Some("{}"), dev_mode);
    }
}
