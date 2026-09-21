use crate::runtime::{increment_metric_tx, now_ms, ActionRequest, ReceiptSigner, RuntimeError};
use crate::runtime_payload::build_payload;
use crate::{canonicalize_json, receipt_hash, Envelope, ReceiptError};
use rusqlite::{params, Connection, OptionalExtension};
use std::time::Instant;

pub(crate) struct SignedReceiptInput<'a> {
    pub(crate) request: &'a ActionRequest,
    pub(crate) kind: &'a str,
    pub(crate) status: &'a str,
    pub(crate) args_hash: &'a str,
    pub(crate) result: Option<&'a str>,
    pub(crate) refusal: Option<&'a str>,
}

pub(crate) fn signed_receipt(
    tx: &Connection,
    signer: &dyn ReceiptSigner,
    input: SignedReceiptInput<'_>,
) -> Result<(String, String), RuntimeError> {
    let SignedReceiptInput {
        request,
        kind,
        status,
        args_hash,
        result,
        refusal,
    } = input;
    let append_started = Instant::now();
    let key_id = signer.key_id()?;
    let previous: Option<String> = tx
        .query_row(
            "SELECT receipt_hash FROM receipt_chain_heads WHERE key_id=?1",
            [&key_id],
            |r| r.get(0),
        )
        .optional()?;
    let last: Option<String> = tx.query_row("SELECT receipt_hash FROM receipt_records WHERE key_id=?1 ORDER BY created_at_ms DESC, rowid DESC LIMIT 1", [&key_id], |r| r.get(0)).optional()?;
    if previous != last {
        return Err(RuntimeError::Code("schema_violation"));
    }
    let payload = build_payload(
        request,
        kind,
        status,
        args_hash,
        previous.as_deref(),
        result,
        refusal,
    );
    crate::validate_payload_v1(&payload)?;
    let payload_bytes = crate::payload_bytes(&payload)?;
    let payload_hash = crate::sha256_hex(&payload_bytes);
    let signature = signer.sign_payload_hash(&payload_hash)?;
    let envelope = Envelope {
        payload: payload.clone(),
        key_id: key_id.clone(),
        signature_algorithm: "Ed25519".into(),
        signature,
    };
    let envelope_bytes =
        canonicalize_json(&serde_json::to_vec(&envelope).map_err(|_| ReceiptError::InvalidJson)?)?;
    let hash = receipt_hash(&envelope)?;
    let object = payload.as_object().unwrap();
    tx.execute("INSERT INTO receipt_records(schema_version,receipt_id,action_id,receipt_kind,action_status,task_id,run_id,key_id,canonical_payload,canonical_envelope,receipt_hash,previous_receipt_hash,created_at_ms) VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![object["receipt_id"].as_str(), request.action_id.to_string(), kind, status, request.task_id, request.run_id, key_id, payload_bytes, envelope_bytes, hash, previous, now_ms()])?;
    tx.execute("INSERT INTO receipt_chain_heads(key_id,receipt_hash,updated_at_ms) VALUES(?1,?2,?3) ON CONFLICT(key_id) DO UPDATE SET receipt_hash=excluded.receipt_hash,updated_at_ms=excluded.updated_at_ms", params![key_id, hash, now_ms()])?;
    increment_metric_tx(tx, "receipt_append_count", 1)?;
    let elapsed = append_started.elapsed().as_millis().min(i64::MAX as u128) as i64;
    increment_metric_tx(tx, "receipt_append_latency_ms", elapsed)?;
    increment_metric_tx(
        tx,
        if kind == "pre_action" {
            "receipt_pre_latency_ms"
        } else if kind == "post_action" {
            "receipt_post_latency_ms"
        } else {
            "receipt_refusal_latency_ms"
        },
        elapsed,
    )?;
    if kind == "pre_action"
        && matches!(
            request.tool_name.as_str(),
            "filesystem.read"
                | "filesystem.list"
                | "git.status"
                | "git.diff"
                | "git.log"
                | "git.show"
                | "git.blame"
                | "git.changed_files"
                | "workspace.list"
                | "workspace.read"
                | "workspace.search"
        )
    {
        increment_metric_tx(tx, "read_only_sampled_count", 1)?;
    }
    Ok((hash, payload_hash))
}
