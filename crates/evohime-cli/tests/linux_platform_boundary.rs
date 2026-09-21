#![cfg(not(windows))]

use std::process::Command;

#[test]
fn headless_commands_fail_closed_without_windows_core() {
    let binary = env!("CARGO_BIN_EXE_eva");
    let commands = [
        vec!["doctor", "--json"],
        vec!["run", "--json", "проверь репозиторий"],
        vec!["status", "synthetic-linux-test", "--json"],
    ];

    for args in commands {
        let output = Command::new(binary)
            .args(args)
            .output()
            .expect("eva starts");
        assert_eq!(output.status.code(), Some(7));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("core_unavailable"),
            "unexpected stderr: {stderr}"
        );
    }
}
