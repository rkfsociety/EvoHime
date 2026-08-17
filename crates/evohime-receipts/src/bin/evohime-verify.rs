use evohime_receipts::key_lifecycle::{verify_transitions, KeyTransition, VerificationStatus};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 || args.get(1).map(String::as_str) != Some("verify") {
        eprintln!("usage: evohime-verify verify --key-history <path> --trust-key <key-id> [--format text|json]");
        return ExitCode::from(4);
    }
    let mut history = None;
    let mut trust = None;
    let mut format = "text";
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--key-history" => {
                index += 1;
                history = args.get(index);
            }
            "--trust-key" => {
                index += 1;
                trust = args.get(index);
            }
            "--format" => {
                index += 1;
                format = args.get(index).map(String::as_str).unwrap_or("");
            }
            "--receipts" => {
                index += 1;
            }
            _ => return ExitCode::from(4),
        }
        index += 1;
    }
    let Some(path) = history else {
        return ExitCode::from(4);
    };
    if !matches!(format, "text" | "json") {
        return ExitCode::from(4);
    }
    let raw = match fs::read(path) {
        Ok(value) => value,
        Err(_) => return ExitCode::from(4),
    };
    if !raw.ends_with(b"\n") {
        return ExitCode::from(2);
    }
    let mut items = Vec::new();
    for line in raw
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        match serde_json::from_slice::<KeyTransition>(line) {
            Ok(item) => items.push(item),
            Err(_) => return ExitCode::from(2),
        }
    }
    let result = match verify_transitions(&items, trust.map(String::as_str)) {
        Ok(status) => status,
        Err(error) => {
            emit(format, "broken", &error.to_string());
            return ExitCode::from(2);
        }
    };
    let (name, code) = match result {
        VerificationStatus::Verified => ("verified", 0),
        VerificationStatus::Untrusted => ("untrusted", 3),
        VerificationStatus::Broken => ("broken", 2),
        VerificationStatus::Unsupported => ("unsupported", 4),
    };
    emit(format, name, "");
    ExitCode::from(code)
}

fn emit(format: &str, status: &str, error: &str) {
    if format == "json" {
        println!(
            "{{\"status\":\"{}\"{} }}",
            status,
            if error.is_empty() {
                String::new()
            } else {
                format!(",\"error\":\"{}\"", error.replace('"', "\\\""))
            }
        );
    } else if error.is_empty() {
        println!("{}", status);
    } else {
        eprintln!("{}: {}", status, error);
    }
}
