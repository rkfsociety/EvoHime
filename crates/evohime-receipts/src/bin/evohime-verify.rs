use evohime_receipts::chain::{verify_chain, ChainRow, ChainStatus, ExportedCheckpoint};
use evohime_receipts::key_lifecycle::{
    public_key_bytes, verify_checkpoint as verify_key_history_checkpoint, HistoryManifest,
    KeyHistoryCheckpoint, KeyTransition,
};
use evohime_receipts::Envelope;
use std::{env, fs, path::Path, process::ExitCode};

struct Args {
    receipts: String,
    history: String,
    trust: Option<String>,
    checkpoint: Option<String>,
    receipt_checkpoints: Option<String>,
    format: String,
}

fn main() -> ExitCode {
    if env::args().nth(1).as_deref() == Some("provenance") {
        return verify_provenance_bundle();
    }
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
    let mut history: Vec<KeyTransition> = Vec::new();
    for line in raw_history
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        match serde_json::from_slice::<KeyTransition>(line) {
            Ok(item) => history.push(item),
            Err(_) => return fail(&args.format, "broken", "key.history_incomplete", 2),
        }
    }
    let key_status = match evohime_receipts::key_lifecycle::verify_transitions(
        &history,
        args.trust.as_deref(),
    ) {
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
        if let Err(error) =
            verify_key_history_checkpoint(&checkpoint, &history, args.trust.as_deref())
        {
            return fail(&args.format, "broken", &error.to_string(), 2);
        }
    }
    if let Err(code) = verify_manifest(&args.history, &history) {
        return fail(&args.format, "broken", code, 2);
    }

    let public_keys: std::collections::HashMap<String, Vec<u8>> = history
        .iter()
        .filter_map(|item| {
            public_key_bytes(&item.new_public_key)
                .ok()
                .map(|key| (item.new_key_id.clone(), key))
        })
        .collect();

    // receipts.jsonl carries the 01.4 export-record wrapper (sequence,
    // receipt_hash, base64url canonical_envelope), not a bare envelope per
    // line; the wrapper's own fields are display-only and never trusted
    // over the receipt_hash recomputed from the decoded envelope.
    let raw_receipts = match fs::read(&args.receipts) {
        Ok(value) => value,
        Err(_) => return fail(&args.format, "unsupported", "input.unreadable", 4),
    };
    if !raw_receipts.is_empty() && !raw_receipts.ends_with(b"\n") {
        return fail(&args.format, "broken", "receipts.non_canonical", 2);
    }
    let mut rows: Vec<ChainRow> = Vec::new();
    for line in raw_receipts
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let record: serde_json::Value = match serde_json::from_slice(line) {
            Ok(value) => value,
            Err(_) => return fail(&args.format, "broken", "receipts.invalid_json", 2),
        };
        if record.get("record_version").and_then(|v| v.as_u64()) != Some(1) {
            return fail(
                &args.format,
                "unsupported",
                "receipts.unsupported_version",
                4,
            );
        }
        let Some(sequence) = record
            .get("sequence")
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<i64>().ok())
        else {
            return fail(&args.format, "broken", "receipts.invalid_json", 2);
        };
        let Some(receipt_hash) = record.get("receipt_hash").and_then(|v| v.as_str()) else {
            return fail(&args.format, "broken", "receipts.invalid_json", 2);
        };
        let Some(envelope_b64) = record.get("canonical_envelope").and_then(|v| v.as_str()) else {
            return fail(&args.format, "broken", "receipts.invalid_json", 2);
        };
        let Some(envelope_bytes) = decode_base64url(envelope_b64) else {
            return fail(&args.format, "broken", "receipts.non_canonical", 2);
        };
        let envelope: Envelope = match serde_json::from_slice(&envelope_bytes) {
            Ok(value) => value,
            Err(_) => return fail(&args.format, "broken", "receipts.invalid_json", 2),
        };
        let payload = &envelope.payload;
        let (Some(action_id), Some(receipt_kind), Some(action_status)) = (
            payload.get("action_id").and_then(|v| v.as_str()),
            payload.get("receipt_kind").and_then(|v| v.as_str()),
            payload.get("action_status").and_then(|v| v.as_str()),
        ) else {
            return fail(&args.format, "broken", "receipts.non_canonical", 2);
        };
        let receipt_id = payload
            .get("receipt_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let previous_receipt_hash = payload
            .get("previous_receipt_hash")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let approval_id = payload
            .get("approval_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        rows.push(ChainRow {
            sequence,
            receipt_id: receipt_id.to_string(),
            action_id: action_id.to_string(),
            receipt_kind: receipt_kind.to_string(),
            action_status: action_status.to_string(),
            key_id: envelope.key_id.clone(),
            receipt_hash: receipt_hash.to_string(),
            previous_receipt_hash,
            approval_id,
            approval_call_hash: None,
            envelope,
        });
    }
    rows.sort_by_key(|row| row.sequence);

    // checkpoints.jsonl is optional; when present, its signature is
    // verified against signed key history before it can authorize any
    // pruned-prefix boundary in verify_chain.
    let mut checkpoint_prefix_hash: Option<String> = None;
    if let Some(path) = args.receipt_checkpoints.as_deref() {
        let raw = match fs::read(path) {
            Ok(value) => value,
            Err(_) => return fail(&args.format, "unsupported", "input.unreadable", 4),
        };
        for line in raw
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            let checkpoint: ExportedCheckpoint = match serde_json::from_slice(line) {
                Ok(value) => value,
                Err(_) => return fail(&args.format, "broken", "receipts.invalid_json", 2),
            };
            let Some(signer_key) = public_keys.get(&checkpoint.signed_by_key_id) else {
                return fail(&args.format, "broken", "receipts.key_unknown", 2);
            };
            if !evohime_receipts::chain::verify_checkpoint_signature(&checkpoint, signer_key) {
                return fail(&args.format, "broken", "receipts.signature_invalid", 2);
            }
            if let Some(first) = rows.first() {
                if first.key_id == checkpoint.key_id
                    && first.receipt_hash == checkpoint.first_retained_hash
                    && first.sequence.to_string() == checkpoint.cutoff_sequence
                {
                    checkpoint_prefix_hash = Some(checkpoint.prefix_last_hash.clone());
                }
            }
        }
    }

    let verification = verify_chain(
        &rows,
        &history,
        args.trust.as_deref(),
        checkpoint_prefix_hash.as_deref(),
    );
    let (name, code) = match verification.status {
        ChainStatus::Verified => (
            "verified",
            if matches!(
                key_status,
                evohime_receipts::key_lifecycle::VerificationStatus::Verified
            ) {
                0
            } else {
                3
            },
        ),
        ChainStatus::VerifiedPruned => (
            "verified_pruned",
            if matches!(
                key_status,
                evohime_receipts::key_lifecycle::VerificationStatus::Verified
            ) {
                0
            } else {
                3
            },
        ),
        ChainStatus::Pending => ("pending", 6),
        ChainStatus::StaleKey => ("stale_key", 5),
        ChainStatus::Unverified => ("unverified", 3),
        ChainStatus::Broken => ("broken", 2),
    };
    emit(&args.format, name, verification.code.unwrap_or(""));
    ExitCode::from(code)
}

/// Минимальный offline boundary для `evohime-provenance-export-v1`: verifier
/// читает только bundle, проверяет allow-list, размеры и file hashes. Он не
/// открывает SQLite/Core/workspace и не подставляет текущее состояние.
fn verify_provenance_bundle() -> ExitCode {
    let mut args = env::args().skip(2);
    let mut bundle = None;
    let mut format = "text".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bundle" => bundle = args.next(),
            "--format" => format = args.next().unwrap_or_else(|| "text".into()),
            _ => return fail(&format, "unsupported", "EXPORT_MANIFEST_MISMATCH", 4),
        }
    }
    let Some(bundle) = bundle else { return fail(&format, "unsupported", "input.unreadable", 4); };
    let root = Path::new(&bundle);
    let manifest: serde_json::Value = match fs::read(root.join("manifest.json")).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()) {
        Some(value) => value,
        None => return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2),
    };
    if manifest["bundle_schema_version"] != 1 { return fail(&format, "unsupported", "EXPORT_MANIFEST_MISMATCH", 4); }
    let Some(files) = manifest["files"].as_object() else { return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2); };
    let mut digest_input = b"evohime-provenance-bundle-v1\0".to_vec();
    for (relative, expected) in files {
        let path = Path::new(relative);
        if path.is_absolute() || path.components().any(|component| component == std::path::Component::ParentDir) {
            return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
        }
        let bytes = match fs::read(root.join(path)) { Ok(value) if value.len() <= 16 * 1024 * 1024 => value, _ => return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2) };
        let actual = evohime_receipts::sha256_hex(&bytes);
        if expected.as_str() != Some(actual.as_str()) { return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2); }
        digest_input.extend(relative.as_bytes()); digest_input.push(0); digest_input.extend(actual.as_bytes()); digest_input.push(b'\n');
    }
    if manifest["bundle_content_sha256"].as_str() != Some(evohime_receipts::sha256_hex(&digest_input).as_str()) {
        return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
    }
    let state = manifest["request_states"].as_array().and_then(|items| items.first()).and_then(|item| item["verification_state"].as_str()).unwrap_or("damaged");
    if format == "json" { println!("{{\"status\":\"verified\",\"verification_state\":\"{}\"}}", state); } else { println!("verified: {state}"); }
    ExitCode::SUCCESS
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
    let mut receipt_checkpoints = None;
    let mut format = "text".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--receipts" => receipts = Some(args.next()?),
            "--key-history" => history = Some(args.next()?),
            "--trust-key" => trust = Some(args.next()?),
            "--checkpoint" => checkpoint = Some(args.next()?),
            "--receipt-checkpoints" => receipt_checkpoints = Some(args.next()?),
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
        receipt_checkpoints,
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

fn decode_base64url(value: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .ok()
}

fn usage() {
    eprintln!("usage: evohime-verify verify --receipts <path> --key-history <path> [--trust-key <key-id>] [--checkpoint <key-history-checkpoint-path>] [--receipt-checkpoints <checkpoints.jsonl-path>] [--format text|json]");
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
