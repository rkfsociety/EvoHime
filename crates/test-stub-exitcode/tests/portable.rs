use std::process::Command;

// Native tests used POSIX `true` and `false` at these call sites before this
// fixture was added. Keep the replacement explicit: no shell, Bash, cmd.exe,
// WSL, or PATH lookup is involved.
#[test]
fn exits_with_requested_code() {
    let stub = env!("CARGO_BIN_EXE_test-stub-exitcode");

    for expected in [0_u8, 1, 17, 255] {
        let status = Command::new(stub)
            .arg(expected.to_string())
            .status()
            .expect("test-stub-exitcode starts by explicit path");
        assert_eq!(status.code(), Some(i32::from(expected)));
    }
}

#[test]
fn invalid_argument_returns_controlled_usage_code() {
    let stub = env!("CARGO_BIN_EXE_test-stub-exitcode");
    let status = Command::new(stub)
        .arg("not-an-exit-code")
        .status()
        .expect("test-stub-exitcode starts by explicit path");
    assert_eq!(status.code(), Some(2));
}
