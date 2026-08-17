use evohime_receipts::key_lifecycle::{
    public_key_bytes, verify_checkpoint, verify_transitions, HistoryManifest, KeyHistoryCheckpoint,
    KeyTransition, VerificationStatus,
};
use evohime_receipts::{verify_ed25519, Envelope};
use std::{collections::HashMap, env, fs, path::Path, process::ExitCode};

struct Args {
    receipts: String,
    history: String,
    trust: Option<String>,
    checkpoint: Option<String>,
    format: String,
}

fn main() -> ExitCode {
    let Some(args) = parse_args() else {
        usage();
        return ExitCode::from(4);
    };
    let raw_history = match fs::read(&args.history) {
        Ok(value) => value,
        Err(_) => return fail(&args.format, "unsupported", "input.unreadable", 4),
    };
    if !raw_history.ends_with(b"\n") {
        return fail(&args.format, "broken", "key.history_incomplete", 2);
    }
    let mut history = Vec::new();
    for line in raw_history
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        match serde_json::from_slice::<KeyTransition>(line) {
            Ok(item) => history.push(item),
            Err(_) => return fail(&args.format, "broken", "key.history_incomplete", 2),
        }
    }
    let status = match verify_transitions(&history, args.trust.as_deref()) {
        Ok(status) => status,
        Err(error) => return fail(&args.format, "broken", &error.to_string(), 2),
    };
    if let Some(path) = args.checkpoint.as_deref() {
        let checkpoint: KeyHistoryCheckpoint = match fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        {
            Some(value) => value,
            None => return fail(&args.format, "broken", "key.history_incomplete", 2),
        };
        if let Err(error) = verify_checkpoint(&checkpoint, &history, args.trust.as_deref()) {
            return fail(&args.format, "broken", &error.to_string(), 2);
        }
    }
    if let Err(code) = verify_manifest(&args.history, &history) {
        return fail(&args.format, "broken", code, 2);
    }
    let stale_keys: std::collections::HashSet<String> = history
        .iter()
        .filter(|item| matches!(item.continuity.as_str(), "compromised" | "broken"))
        .filter_map(|item| item.previous_key_id.clone())
        .collect();
    let keys: HashMap<String, Vec<u8>> = history
        .iter()
        .filter_map(|item| {
            public_key_bytes(&item.new_public_key)
                .ok()
                .map(|key| (item.new_key_id.clone(), key))
        })
        .collect();
    let raw_receipts = match fs::read(&args.receipts) {
        Ok(value) => value,
        Err(_) => return fail(&args.format, "unsupported", "input.unreadable", 4),
    };
    if !raw_receipts.is_empty() && !raw_receipts.ends_with(b"\n") {
        return fail(&args.format, "broken", "receipt.history_incomplete", 2);
    }
    for line in raw_receipts
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let envelope: Envelope = match serde_json::from_slice(line) {
            Ok(value) => value,
            Err(_) => return fail(&args.format, "broken", "receipt.invalid_json", 2),
        };
        let Some(public) = keys.get(&envelope.key_id) else {
            return fail(&args.format, "broken", "receipt.key_unknown", 2);
        };
        if stale_keys.contains(&envelope.key_id) {
            return fail(&args.format, "stale_key", "key.stale_key", 5);
        }
        if verify_ed25519(&envelope, public).is_err() {
            return fail(&args.format, "broken", "receipt.signature_invalid", 2);
        }
    }
    let (name, code) = match status {
        VerificationStatus::Verified => ("verified", 0),
        VerificationStatus::Untrusted => ("untrusted", 3),
        VerificationStatus::Broken => ("broken", 2),
        VerificationStatus::Unsupported => ("unsupported", 4),
    };
    emit(&args.format, name, "");
    ExitCode::from(code)
}

fn parse_args() -> Option<Args> {
    let mut args = env::args().skip(1);
    if args.next()?.as_str() != "verify" {
        return None;
    }
    let mut receipts = None;
    let mut history = None;
    let mut trust = None;
    let mut checkpoint = None;
    let mut format = "text".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--receipts" => receipts = Some(args.next()?),
            "--key-history" => history = Some(args.next()?),
            "--trust-key" => trust = Some(args.next()?),
            "--checkpoint" => checkpoint = Some(args.next()?),
            "--format" => format = args.next()?,
            _ => return None,
        }
    }
    if !matches!(format.as_str(), "text" | "json") {
        return None;
    }
    Some(Args {
        receipts: receipts?,
        history: history?,
        trust,
        checkpoint,
        format,
    })
}

fn verify_manifest(history: &str, items: &[KeyTransition]) -> Result<(), &'static str> {
    let manifest_path = Path::new(history).with_file_name("public-history-v1.manifest.json");
    let bytes = fs::read(manifest_path).map_err(|_| "key.history_incomplete")?;
    let manifest: HistoryManifest =
        serde_json::from_slice(&bytes).map_err(|_| "key.history_incomplete")?;
    if manifest.manifest_version != 1
        || manifest.status != "current"
        || manifest.exported_transition_count != items.len()
        || manifest.active_key_id
            != items
                .last()
                .map(|item| item.new_key_id.as_str())
                .unwrap_or("")
    {
        return Err(if manifest.status == "key.history_export_failed" {
            "key.history_export_failed"
        } else {
            "key.history_incomplete"
        });
    }
    Ok(())
}

fn usage() {
    eprintln!("usage: evohime-verify verify --receipts <path> --key-history <path> [--trust-key <key-id>] [--checkpoint <path>] [--format text|json]");
}
fn fail(format: &str, status: &str, error: &str, code: u8) -> ExitCode {
    emit(format, status, error);
    ExitCode::from(code)
}
fn emit(format: &str, status: &str, error: &str) {
    if format == "json" {
        println!(
            "{{\"status\":\"{}\",\"error\":\"{}\"}}",
            status,
            error.replace('"', "\\\"")
        );
    } else if error.is_empty() {
        println!("{status}");
    } else {
        eprintln!("{status}: {error}");
    }
}
