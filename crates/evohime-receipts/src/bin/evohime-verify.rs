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
    let mut trust_key = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bundle" => bundle = args.next(),
            "--format" => format = args.next().unwrap_or_else(|| "text".into()),
            "--trust-key" => trust_key = args.next(),
            _ => return fail(&format, "unsupported", "EXPORT_MANIFEST_MISMATCH", 4),
        }
    }
    let Some(bundle) = bundle else {
        return fail(&format, "unsupported", "input.unreadable", 4);
    };
    let root = Path::new(&bundle);
    let manifest_bytes = match fs::read(root.join("manifest.json")) {
        Ok(bytes) => bytes,
        Err(_) => return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2),
    };
    let manifest: serde_json::Value = match serde_json::from_slice(&manifest_bytes) {
        Ok(value) => value,
        Err(_) => return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2),
    };
    let canonical_manifest = match evohime_receipts::canonicalize_json(&manifest_bytes) {
        Ok(bytes) => bytes,
        Err(_) => return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2),
    };
    if canonical_manifest != manifest_bytes {
        return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
    }
    if manifest["bundle_schema_version"] != 1 {
        return fail(&format, "unsupported", "EXPORT_MANIFEST_MISMATCH", 4);
    }
    let Some(files) = manifest["files"].as_object() else {
        return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
    };
    let required = [
        "key-history.jsonl",
        "checkpoints.jsonl",
        "receipt_records/records.jsonl",
        "context_ledger/entries.jsonl",
        "request_snapshots/route_policy.jsonl",
        "model_requests/requests.jsonl",
        "model_requests/block_refs.jsonl",
        "model_responses/responses.jsonl",
        "tool_intents/intents.jsonl",
        "tool_intents/receipt_links.jsonl",
        "context_evidence/sources.jsonl",
        "context_evidence/blocks.jsonl",
        "context_shadowed_originals/records.jsonl",
        "context_shadow_source_refs/refs.jsonl",
        "context_shadow_blocks/blocks.jsonl",
        "provenance_tombstones/tombstones.jsonl",
    ];
    if required.iter().any(|path| !files.contains_key(*path)) {
        return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
    }
    if !bundle_tree_matches_manifest(root, files) {
        return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
    }
    let mut digest_input = b"evohime-provenance-bundle-v1\0".to_vec();
    let mut total_bytes = 0u64;
    for (relative, expected) in files {
        let path = Path::new(relative);
        if path.is_absolute()
            || path
                .components()
                .any(|component| component == std::path::Component::ParentDir)
        {
            return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
        }
        let bytes = match fs::read(root.join(path)) {
            Ok(value) if value.len() <= 16 * 1024 * 1024 => value,
            _ => return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2),
        };
        total_bytes = total_bytes.saturating_add(bytes.len() as u64);
        if total_bytes > 256 * 1024 * 1024 {
            return fail(&format, "broken", "EXPORT_BUNDLE_TOO_LARGE", 2);
        }
        let actual = evohime_receipts::sha256_hex(&bytes);
        if expected.as_str() != Some(actual.as_str()) {
            return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
        }
        if let Some(size) = manifest["file_sizes"]
            .get(relative)
            .and_then(|value| value.as_u64())
        {
            if size != bytes.len() as u64 {
                return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
            }
        }
        digest_input.extend(relative.as_bytes());
        digest_input.push(0);
        digest_input.extend(actual.as_bytes());
        digest_input.push(b'\n');
    }
    if manifest["bundle_content_sha256"].as_str()
        != Some(evohime_receipts::sha256_hex(&digest_input).as_str())
    {
        return fail(&format, "broken", "EXPORT_MANIFEST_MISMATCH", 2);
    }
    let Some(public_key_hex) = manifest["signer"]["public_key_hex"].as_str() else {
        return fail(&format, "unverified", "EXPORT_SIGNATURE_INVALID", 3);
    };
    let Some(public_key) = decode_hex(public_key_hex) else {
        return fail(&format, "unverified", "EXPORT_SIGNATURE_INVALID", 3);
    };
    if public_key.len() != 32 {
        return fail(&format, "unverified", "EXPORT_SIGNATURE_INVALID", 3);
    }
    let expected_key_id = format!("ed25519:{}", evohime_receipts::sha256_hex(&public_key));
    if manifest["signer"]["key_id"].as_str() != Some(expected_key_id.as_str()) {
        return fail(&format, "unverified", "EXPORT_SIGNATURE_INVALID", 3);
    }
    let Some(trust_key) = trust_key else {
        return fail(&format, "unverified", "EXPORT_SIGNATURE_KEY_UNKNOWN", 3);
    };
    if trust_key != expected_key_id && trust_key != public_key_hex {
        return fail(&format, "unverified", "EXPORT_SIGNATURE_KEY_UNKNOWN", 3);
    }
    let signature = match fs::read(root.join("bundle.sig")) {
        Ok(value) => value,
        Err(_) => return fail(&format, "unverified", "EXPORT_SIGNATURE_INVALID", 3),
    };
    let manifest_digest = evohime_receipts::sha256_digest(
        &[
            b"evohime-provenance-manifest-v1\0".as_slice(),
            manifest_bytes.as_slice(),
        ]
        .concat(),
    );
    if ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &public_key)
        .verify(&manifest_digest, &signature)
        .is_err()
    {
        return fail(&format, "unverified", "EXPORT_SIGNATURE_INVALID", 3);
    }
    if let Err(code) = verify_provenance_records(root, &public_key) {
        return fail(&format, "broken", code, 2);
    }
    let state = manifest["request_states"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["verification_state"].as_str())
        .unwrap_or("damaged");
    if format == "json" {
        println!(
            "{{\"status\":\"verified\",\"verification_state\":\"{}\"}}",
            state
        );
    } else {
        println!("verified: {state}");
    }
    ExitCode::SUCCESS
}

fn verify_provenance_records(root: &Path, public_key: &[u8]) -> Result<(), &'static str> {
    let requests_path = root.join("model_requests/requests.jsonl");
    let request_bytes = fs::read(requests_path).map_err(|_| "EXPORT_CLOSURE_MISMATCH")?;
    let mut request_ids = std::collections::HashSet::new();
    for line in request_bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: serde_json::Value =
            serde_json::from_slice(line).map_err(|_| "EXPORT_CLOSURE_MISMATCH")?;
        let request_id = value["request_id"]
            .as_str()
            .ok_or("EXPORT_CLOSURE_MISMATCH")?;
        request_ids.insert(request_id.to_owned());
    }

    let response_bytes = fs::read(root.join("model_responses/responses.jsonl"))
        .map_err(|_| "EXPORT_CLOSURE_MISMATCH")?;
    let mut response_ids = std::collections::HashSet::new();
    for line in response_bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: serde_json::Value =
            serde_json::from_slice(line).map_err(|_| "EXPORT_CLOSURE_MISMATCH")?;
        if let Some(response_id) = value["response_id"].as_str() {
            response_ids.insert(response_id.to_owned());
        }
    }

    let receipt_bytes = fs::read(root.join("receipt_records/records.jsonl"))
        .map_err(|_| "EXPORT_CLOSURE_MISMATCH")?;
    for line in receipt_bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: serde_json::Value =
            serde_json::from_slice(line).map_err(|_| "EXPORT_RECEIPT_INVALID")?;
        let envelope_text = value["canonical_envelope"]
            .as_str()
            .ok_or("EXPORT_RECEIPT_INVALID")?;
        let envelope_bytes = envelope_text.as_bytes();
        let canonical = evohime_receipts::canonicalize_json(envelope_bytes)
            .map_err(|_| "EXPORT_RECEIPT_INVALID")?;
        if canonical != envelope_bytes {
            return Err("EXPORT_RECEIPT_INVALID");
        }
        let envelope: Envelope =
            serde_json::from_slice(envelope_bytes).map_err(|_| "EXPORT_RECEIPT_INVALID")?;
        if evohime_receipts::receipt_hash(&envelope).map_err(|_| "EXPORT_RECEIPT_INVALID")?
            != value["receipt_hash"].as_str().unwrap_or_default()
        {
            return Err("EXPORT_RECEIPT_INVALID");
        }
        evohime_receipts::verify_ed25519(&envelope, public_key)
            .map_err(|_| "EXPORT_RECEIPT_SIGNATURE_INVALID")?;
        let payload = &envelope.payload;
        if payload["receipt_domain"].as_str() == Some("model_request") {
            let request_id = payload["request_id"]
                .as_str()
                .ok_or("EXPORT_CLOSURE_MISMATCH")?;
            if !request_ids.contains(request_id) {
                return Err("EXPORT_CLOSURE_MISMATCH");
            }
        } else if let Some(action_id) = payload["action_id"].as_str() {
            if action_id.is_empty() {
                return Err("EXPORT_RECEIPT_INVALID");
            }
        }
    }

    let intent_bytes =
        fs::read(root.join("tool_intents/intents.jsonl")).map_err(|_| "EXPORT_CLOSURE_MISMATCH")?;
    for line in intent_bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: serde_json::Value =
            serde_json::from_slice(line).map_err(|_| "EXPORT_CLOSURE_MISMATCH")?;
        if !request_ids.contains(value["origin_request_id"].as_str().unwrap_or_default())
            || value["response_id"]
                .as_str()
                .is_some_and(|id| !response_ids.contains(id))
        {
            return Err("EXPORT_CLOSURE_MISMATCH");
        }
    }
    Ok(())
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

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

fn bundle_tree_matches_manifest(
    root: &Path,
    files: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    fn walk(
        root: &Path,
        current: &Path,
        files: &serde_json::Map<String, serde_json::Value>,
    ) -> bool {
        let Ok(entries) = fs::read_dir(current) else {
            return false;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = fs::symlink_metadata(&path) else {
                return false;
            };
            if kind.file_type().is_symlink() {
                return false;
            }
            if kind.is_dir() {
                if !walk(root, &path, files) {
                    return false;
                }
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if relative != "manifest.json"
                && relative != "bundle.sig"
                && !files.contains_key(&relative)
            {
                return false;
            }
        }
        true
    }
    walk(root, root, files)
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
