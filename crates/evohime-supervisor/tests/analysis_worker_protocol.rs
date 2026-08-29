use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn worker() -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_evohime-analysis-worker"))
        .args(["--protocol-version=1", "--runtime=trusted-local-1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("analysis worker starts as a separate process")
}

#[test]
fn real_worker_process_executes_pure_operation_and_denies_effect() {
    let mut child = worker();
    let mut stdin = child.stdin.take().expect("worker stdin");
    let stdout = child.stdout.take().expect("worker stdout");
    let mut stdout = BufReader::new(stdout);
    stdin
        .write_all(
            br#"{"request_id":"pure","operation":"json_select","args":{"value":{"x":7},"path":["x"]}}
"#,
        )
        .unwrap();
    stdin
        .write_all(
            br#"{"request_id":"effect","operation":"filesystem","args":{}}
"#,
        )
        .unwrap();
    stdin.flush().unwrap();

    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.contains(r#""request_id":"pure""#));
    assert!(line.contains(r#""status":"ok""#));
    assert!(line.contains(r#""result":7"#));
    line.clear();
    stdout.read_line(&mut line).unwrap();
    assert!(line.contains(r#""request_id":"effect""#));
    assert!(line.contains(r#""error_class":"host_request_required""#));

    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn real_worker_process_rejects_an_oversized_line() {
    let mut child = worker();
    let mut stdin = child.stdin.take().expect("worker stdin");
    let stdout = child.stdout.take().expect("worker stdout");
    let mut stdout = BufReader::new(stdout);
    let oversized = format!(
        r#"{{"request_id":"large","operation":"json_parse","args":"{}"}}"#,
        "x".repeat(16 * 1024)
    );
    writeln!(stdin, "{oversized}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.contains(r#""error_class":"request_too_large""#));
    drop(stdin);
    assert!(child.wait().unwrap().success());
}
