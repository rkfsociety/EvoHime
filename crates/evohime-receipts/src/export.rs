//! Read-only receipt listing, chain verification and JSONL export bundle for
//! stage 01.4. Everything here reads durable SQLite state; nothing mutates
//! `receipt_records`/`receipt_actions`. SQLite remains the sole mutable
//! source of truth — a JSONL bundle produced here is an immutable snapshot,
//! never re-imported.

use crate::chain::{verify_chain, ChainRow, ChainVerification};
use crate::key_lifecycle::KeyTransition;
use crate::Envelope;
use rusqlite::{Connection, OptionalExtension, ToSql};
use serde::Serialize;
use serde_json::json;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ExportError {
    #[error("receipts.db_unavailable")]
    DbUnavailable,
    #[error("receipts.invalid_filter")]
    InvalidFilter,
    #[error("receipts.limit_exceeded")]
    LimitExceeded,
    #[error("receipts.empty_range")]
    EmptyRange,
    #[error("receipts.export_exists")]
    ExportExists,
    #[error("receipts.export_io")]
    ExportIo,
    #[error("receipts.not_found")]
    NotFound,
}

impl From<rusqlite::Error> for ExportError {
    fn from(_: rusqlite::Error) -> Self {
        ExportError::DbUnavailable
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReceiptFilter {
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub action_id: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiptSummary {
    pub receipt_id: String,
    pub sequence: i64,
    pub action_id: String,
    pub receipt_kind: String,
    pub action_status: String,
    pub task_id: String,
    pub run_id: String,
    pub key_id: String,
    pub created_at_ms: i64,
    pub receipt_hash: String,
    pub previous_receipt_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListResult {
    pub snapshot_last_sequence: i64,
    pub rows: Vec<ReceiptSummary>,
}

fn validate_filter(filter: &ReceiptFilter) -> Result<(), ExportError> {
    for value in [&filter.task_id, &filter.run_id, &filter.action_id]
        .into_iter()
        .flatten()
    {
        if !crate::validate_typed_identifier(value) {
            return Err(ExportError::InvalidFilter);
        }
    }
    if let (Some(from), Some(to)) = (filter.from_ms, filter.to_ms) {
        if from > to {
            return Err(ExportError::InvalidFilter);
        }
    }
    Ok(())
}

fn build_where(filter: &ReceiptFilter) -> (String, Vec<Box<dyn ToSql>>) {
    let mut clauses = vec!["source = 'signed'".to_string()];
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();
    if let Some(task_id) = &filter.task_id {
        clauses.push(format!("task_id = ?{}", params.len() + 1));
        params.push(Box::new(task_id.clone()));
    }
    if let Some(run_id) = &filter.run_id {
        clauses.push(format!("run_id = ?{}", params.len() + 1));
        params.push(Box::new(run_id.clone()));
    }
    if let Some(action_id) = &filter.action_id {
        clauses.push(format!("action_id = ?{}", params.len() + 1));
        params.push(Box::new(action_id.clone()));
    }
    if let Some(from_ms) = filter.from_ms {
        clauses.push(format!("created_at_ms >= ?{}", params.len() + 1));
        params.push(Box::new(from_ms));
    }
    if let Some(to_ms) = filter.to_ms {
        clauses.push(format!("created_at_ms < ?{}", params.len() + 1));
        params.push(Box::new(to_ms));
    }
    (clauses.join(" AND "), params)
}

fn snapshot_last_sequence(connection: &Connection) -> Result<i64, ExportError> {
    Ok(connection
        .query_row(
            "SELECT COALESCE(MAX(rowid), 0) FROM receipt_records WHERE source = 'signed'",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

/// `ListReceipts`: bounded, filtered summaries. Does not expand to chain
/// closure — that expansion is specific to verify/export, where the
/// predecessor chain itself is the thing being checked.
pub fn list_receipts(
    connection: &Connection,
    filter: &ReceiptFilter,
    limit: i64,
) -> Result<ListResult, ExportError> {
    validate_filter(filter)?;
    if !(1..=500).contains(&limit) {
        return Err(ExportError::InvalidFilter);
    }
    let snapshot_last_sequence = snapshot_last_sequence(connection)?;
    let (where_clause, mut params) = build_where(filter);
    let sql = format!(
        "SELECT rowid, receipt_id, action_id, receipt_kind, action_status, task_id, run_id, key_id, created_at_ms, receipt_hash, previous_receipt_hash \
         FROM receipt_records WHERE {where_clause} ORDER BY rowid DESC LIMIT ?{}",
        params.len() + 1
    );
    params.push(Box::new(limit));
    let refs: Vec<&dyn ToSql> = params.iter().map(|value| value.as_ref()).collect();
    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map(refs.as_slice(), |row| {
            Ok(ReceiptSummary {
                sequence: row.get(0)?,
                receipt_id: row.get(1)?,
                action_id: row.get(2)?,
                receipt_kind: row.get(3)?,
                action_status: row.get(4)?,
                task_id: row.get(5)?,
                run_id: row.get(6)?,
                key_id: row.get(7)?,
                created_at_ms: row.get(8)?,
                receipt_hash: row.get(9)?,
                previous_receipt_hash: row.get(10)?,
            })
        })?
        .filter_map(|row| row.ok())
        .collect();
    Ok(ListResult {
        snapshot_last_sequence,
        rows,
    })
}

/// Loads the chain-closure range `[min(matched sequence), max(matched
/// sequence)]` for the filtered rows — the minimal contiguous set that lets
/// the verifier walk every `previous_receipt_hash` link inside it, per the
/// 01.4 "filters cannot hide a predecessor" rule.
fn load_closure_rows(
    connection: &Connection,
    filter: &ReceiptFilter,
    limit: i64,
) -> Result<(Vec<ChainRow>, i64, i64), ExportError> {
    validate_filter(filter)?;
    let (where_clause, params) = build_where(filter);
    let refs: Vec<&dyn ToSql> = params.iter().map(|value| value.as_ref()).collect();
    let bounds_sql = format!(
        "SELECT MIN(rowid), MAX(rowid), COUNT(*) FROM receipt_records WHERE {where_clause}"
    );
    let (min_seq, max_seq, requested_count): (Option<i64>, Option<i64>, i64) = connection
        .query_row(&bounds_sql, refs.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    let (Some(min_seq), Some(max_seq)) = (min_seq, max_seq) else {
        return Ok((Vec::new(), 0, 0));
    };
    let closure_sql = "SELECT receipt_records.rowid, receipt_records.receipt_id, receipt_records.action_id, receipt_records.receipt_kind, receipt_records.action_status, receipt_records.key_id, receipt_records.receipt_hash, receipt_records.previous_receipt_hash, receipt_actions.approval_id, receipt_actions.approval_call_hash, receipt_records.canonical_envelope \
         FROM receipt_records LEFT JOIN receipt_actions ON receipt_actions.action_id = receipt_records.action_id \
         WHERE receipt_records.source = 'signed' AND receipt_records.rowid BETWEEN ?1 AND ?2 ORDER BY receipt_records.rowid ASC";
    let mut statement = connection.prepare(closure_sql)?;
    let mut selected_count = 0i64;
    let rows = statement
        .query_map([min_seq, max_seq], |row| {
            let envelope_bytes: Vec<u8> = row.get(10)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                envelope_bytes,
            ))
        })?
        .filter_map(|row| row.ok())
        .map(
            |(
                sequence,
                receipt_id,
                action_id,
                receipt_kind,
                action_status,
                key_id,
                receipt_hash,
                previous_receipt_hash,
                approval_id,
                approval_call_hash,
                envelope_bytes,
            )| {
                selected_count += 1;
                let envelope: Envelope =
                    serde_json::from_slice(&envelope_bytes).unwrap_or(Envelope {
                        payload: serde_json::Value::Null,
                        key_id: String::new(),
                        signature_algorithm: String::new(),
                        signature: String::new(),
                    });
                ChainRow {
                    sequence,
                    receipt_id,
                    action_id,
                    receipt_kind,
                    action_status,
                    key_id,
                    receipt_hash,
                    previous_receipt_hash,
                    approval_id,
                    approval_call_hash,
                    envelope,
                }
            },
        )
        .collect::<Vec<_>>();
    if rows.len() as i64 > limit {
        return Err(ExportError::LimitExceeded);
    }
    Ok((rows, requested_count, selected_count))
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub verification: ChainVerification,
    pub requested_count: i64,
    pub selected_count: i64,
}

/// `VerifyReceipts`. `key_history` and `trust_key` come from the caller's
/// loaded `ReceiptKeyManager` state, never from the export bundle itself.
pub fn verify_receipts(
    connection: &Connection,
    key_history: &[KeyTransition],
    trust_key: Option<&str>,
    filter: &ReceiptFilter,
    limit: i64,
) -> Result<VerifyResult, ExportError> {
    if !(1..=2000).contains(&limit) {
        return Err(ExportError::InvalidFilter);
    }
    let (rows, requested_count, selected_count) = load_closure_rows(connection, filter, limit)?;
    let checkpoint_prefix_hash = active_checkpoint_prefix_for_first_row(connection, &rows)?;
    let verification = verify_chain(
        &rows,
        key_history,
        trust_key,
        checkpoint_prefix_hash.as_deref(),
    );
    Ok(VerifyResult {
        verification,
        requested_count,
        selected_count,
    })
}

/// If the closure's first row is exactly the row an active checkpoint's
/// `cutoff_sequence`/`first_retained_hash` points at (the retained suffix's
/// boundary), returns that checkpoint's `prefix_last_hash` — the hash of
/// the deleted predecessor the surviving row's own `previous_receipt_hash`
/// still carries — so `verify_chain` can authorize that link and classify
/// the result `verified_pruned` instead of `broken`. Any other relationship
/// between the closure and a checkpoint is not treated as pruned — a
/// closure that starts mid-chain for another reason is not this case.
fn active_checkpoint_prefix_for_first_row(
    connection: &Connection,
    rows: &[ChainRow],
) -> Result<Option<String>, ExportError> {
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let checkpoint: Option<(i64, String, String)> = connection
        .query_row(
            "SELECT cutoff_sequence, first_retained_hash, prefix_last_hash FROM receipt_checkpoints WHERE key_id=?1 AND status='active'",
            [&first.key_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    Ok(
        checkpoint.and_then(|(cutoff_sequence, first_retained_hash, prefix_last_hash)| {
            (cutoff_sequence == first.sequence && first_retained_hash == first.receipt_hash)
                .then_some(prefix_last_hash)
        }),
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportManifestFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportManifest {
    pub manifest_version: u8,
    pub export_id: String,
    pub created_at: String,
    pub snapshot_last_sequence: i64,
    pub requested_count: i64,
    pub selected_count: i64,
    pub record_count: i64,
    pub actual_exported_count: i64,
    pub first_receipt_hash: Option<String>,
    pub last_receipt_hash: Option<String>,
    pub files: Vec<ExportManifestFile>,
}

fn write_jsonl_line(
    file: &mut std::fs::File,
    value: &serde_json::Value,
) -> Result<(), ExportError> {
    let bytes =
        crate::canonicalize_json(&serde_json::to_vec(value).map_err(|_| ExportError::ExportIo)?)
            .map_err(|_| ExportError::ExportIo)?;
    file.write_all(&bytes).map_err(|_| ExportError::ExportIo)?;
    file.write_all(b"\n").map_err(|_| ExportError::ExportIo)?;
    Ok(())
}

fn finalize_file(path: &Path) -> Result<ExportManifestFile, ExportError> {
    let bytes = std::fs::read(path).map_err(|_| ExportError::ExportIo)?;
    let sha256 = crate::sha256_hex(&bytes);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    Ok(ExportManifestFile {
        name,
        bytes: bytes.len() as u64,
        sha256,
    })
}

/// Canonicalizes and rejects a destination that is relative, escapes the
/// export root via `..`, or already exists (v1 never overwrites).
fn canonical_destination(destination: &Path) -> Result<PathBuf, ExportError> {
    if !destination.is_absolute() {
        return Err(ExportError::InvalidFilter);
    }
    if destination
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ExportError::InvalidFilter);
    }
    if destination.exists() {
        return Err(ExportError::ExportExists);
    }
    Ok(destination.to_path_buf())
}

/// `ExportReceipts`. Writes an atomic staging directory, then renames it to
/// `destination`. `key_history_jsonl` is the caller's already-loaded, signed
/// `KeyTransition` history (from `ReceiptKeyManager::load_history`).
pub fn export_receipts(
    connection: &Connection,
    key_history: &[KeyTransition],
    destination: &Path,
    filter: &ReceiptFilter,
    limit: i64,
) -> Result<ExportManifest, ExportError> {
    if !(1..=100_000).contains(&limit) {
        return Err(ExportError::InvalidFilter);
    }
    let destination = canonical_destination(destination)?;
    let snapshot_last_sequence = snapshot_last_sequence(connection)?;
    let (rows, requested_count, selected_count) = load_closure_rows(connection, filter, limit)?;
    if rows.is_empty() {
        return Err(ExportError::EmptyRange);
    }

    let staging_parent = destination
        .parent()
        .ok_or(ExportError::ExportIo)?
        .to_path_buf();
    std::fs::create_dir_all(&staging_parent).map_err(|_| ExportError::ExportIo)?;
    let staging = staging_parent.join(format!(".evohime-export-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&staging).map_err(|_| ExportError::ExportIo)?;
    let cleanup = |staging: &Path| {
        let _ = std::fs::remove_dir_all(staging);
    };

    let mut receipts_file = match std::fs::File::create(staging.join("receipts.jsonl")) {
        Ok(file) => file,
        Err(_) => {
            cleanup(&staging);
            return Err(ExportError::ExportIo);
        }
    };
    for row in &rows {
        let envelope_bytes = match crate::canonicalize_json(
            &serde_json::to_vec(&row.envelope).map_err(|_| ExportError::ExportIo)?,
        ) {
            Ok(bytes) => bytes,
            Err(_) => {
                cleanup(&staging);
                return Err(ExportError::ExportIo);
            }
        };
        let record = json!({
            "record_version": 1,
            "record_kind": "receipt",
            "sequence": row.sequence.to_string(),
            "receipt_hash": row.receipt_hash,
            "canonical_envelope": base64_url_unpadded(&envelope_bytes),
        });
        if write_jsonl_line(&mut receipts_file, &record).is_err() {
            cleanup(&staging);
            return Err(ExportError::ExportIo);
        }
    }
    drop(receipts_file);

    let mut key_history_file = match std::fs::File::create(staging.join("key-history.jsonl")) {
        Ok(file) => file,
        Err(_) => {
            cleanup(&staging);
            return Err(ExportError::ExportIo);
        }
    };
    for transition in key_history {
        let value = match serde_json::to_value(transition) {
            Ok(value) => value,
            Err(_) => {
                cleanup(&staging);
                return Err(ExportError::ExportIo);
            }
        };
        if write_jsonl_line(&mut key_history_file, &value).is_err() {
            cleanup(&staging);
            return Err(ExportError::ExportIo);
        }
    }
    drop(key_history_file);

    let closure_key_ids: std::collections::HashSet<&str> =
        rows.iter().map(|row| row.key_id.as_str()).collect();
    let mut checkpoint_records: Vec<serde_json::Value> = Vec::new();
    for key_id in &closure_key_ids {
        // checkpoint.schema.json's top-level object: the signed canonical
        // bytes travel base64url-encoded under `canonical_checkpoint`
        // alongside the detached `signature`, exactly like a receipt
        // envelope — the offline verifier trusts neither the duplicated
        // display columns nor an unsigned re-serialization.
        let row: Option<(String, String, i64, String, String, String, String, String, Vec<u8>, String, String, String)> =
            match connection.query_row(
                "SELECT checkpoint_id, key_id, cutoff_sequence, first_retained_hash, prefix_last_hash, last_deleted_receipt_hash, head_receipt_hash, created_at, canonical_checkpoint, signed_by_key_id, signature, status \
                 FROM receipt_checkpoints WHERE key_id=?1 AND status='active'",
                [key_id],
                |row| Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                    row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
                )),
            ) {
                Ok(value) => Some(value),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(_) => {
                    cleanup(&staging);
                    return Err(ExportError::ExportIo);
                }
            };
        if let Some((
            checkpoint_id,
            key_id,
            cutoff_sequence,
            first_retained_hash,
            prefix_last_hash,
            last_deleted_receipt_hash,
            head_receipt_hash,
            created_at,
            canonical_checkpoint,
            signed_by_key_id,
            signature,
            status,
        )) = row
        {
            checkpoint_records.push(json!({
                "checkpoint_id": checkpoint_id,
                "key_id": key_id,
                "cutoff_sequence": cutoff_sequence.to_string(),
                "first_retained_hash": first_retained_hash,
                "prefix_last_hash": prefix_last_hash,
                "last_deleted_receipt_hash": last_deleted_receipt_hash,
                "head_receipt_hash": head_receipt_hash,
                "created_at": created_at,
                "canonical_checkpoint": base64_url_unpadded(&canonical_checkpoint),
                "signed_by_key_id": signed_by_key_id,
                "signature": signature,
                "status": status,
            }));
        }
    }
    if !checkpoint_records.is_empty() {
        let mut checkpoints_file = match std::fs::File::create(staging.join("checkpoints.jsonl")) {
            Ok(file) => file,
            Err(_) => {
                cleanup(&staging);
                return Err(ExportError::ExportIo);
            }
        };
        for record in &checkpoint_records {
            if write_jsonl_line(&mut checkpoints_file, record).is_err() {
                cleanup(&staging);
                return Err(ExportError::ExportIo);
            }
        }
        drop(checkpoints_file);
    }

    let has_action_rows = rows
        .iter()
        .any(|row| row.approval_id.is_some() || row.approval_call_hash.is_some());
    if has_action_rows {
        // Bounded projection is written from the same closure rows already
        // loaded; no raw input/result ever enters this file.
        let mut actions_file = match std::fs::File::create(staging.join("actions.jsonl")) {
            Ok(file) => file,
            Err(_) => {
                cleanup(&staging);
                return Err(ExportError::ExportIo);
            }
        };
        let mut seen = std::collections::HashSet::new();
        for row in &rows {
            if !seen.insert(row.action_id.clone()) {
                continue;
            }
            let (recovery_code, requires_reconciliation): (Option<String>, bool) = connection
                .query_row(
                    "SELECT recovery_code, state='pending_recovery' FROM receipt_actions WHERE action_id=?1",
                    [&row.action_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or((None, false));
            let record = json!({
                "record_version": 1,
                "record_kind": "action",
                "action_id": row.action_id,
                "approval_id": row.approval_id,
                "recovery_code": recovery_code,
                "requires_reconciliation": requires_reconciliation,
            });
            if write_jsonl_line(&mut actions_file, &record).is_err() {
                cleanup(&staging);
                return Err(ExportError::ExportIo);
            }
        }
        drop(actions_file);
    }

    let mut files = Vec::new();
    for name in [
        "receipts.jsonl",
        "key-history.jsonl",
        "actions.jsonl",
        "checkpoints.jsonl",
    ] {
        let path = staging.join(name);
        if path.exists() {
            match finalize_file(&path) {
                Ok(entry) => files.push(entry),
                Err(error) => {
                    cleanup(&staging);
                    return Err(error);
                }
            }
        }
    }

    let manifest = ExportManifest {
        manifest_version: 1,
        export_id: uuid::Uuid::now_v7().to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        snapshot_last_sequence,
        requested_count,
        selected_count,
        record_count: rows.len() as i64,
        actual_exported_count: rows.len() as i64,
        first_receipt_hash: rows.first().map(|row| row.receipt_hash.clone()),
        last_receipt_hash: rows.last().map(|row| row.receipt_hash.clone()),
        files,
    };
    let manifest_value = match serde_json::to_value(&manifest) {
        Ok(value) => value,
        Err(_) => {
            cleanup(&staging);
            return Err(ExportError::ExportIo);
        }
    };
    let manifest_bytes = match crate::canonicalize_json(
        &serde_json::to_vec(&manifest_value).map_err(|_| ExportError::ExportIo)?,
    ) {
        Ok(bytes) => bytes,
        Err(_) => {
            cleanup(&staging);
            return Err(ExportError::ExportIo);
        }
    };
    if std::fs::write(staging.join("manifest.json"), &manifest_bytes).is_err() {
        cleanup(&staging);
        return Err(ExportError::ExportIo);
    }

    if std::fs::rename(&staging, &destination).is_err() {
        cleanup(&staging);
        return Err(ExportError::ExportIo);
    }
    Ok(manifest)
}

fn base64_url_unpadded(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_lifecycle::ReceiptKeyManager;
    use crate::runtime::{ActionRequest, PolicyDecision, ReceiptRuntime, ReceiptSigner};
    use rusqlite::Connection;

    struct TestSigner(std::sync::Arc<ReceiptKeyManager>);
    impl ReceiptSigner for TestSigner {
        fn key_id(&self) -> Result<String, crate::runtime::RuntimeError> {
            self.0
                .load_signer()
                .map(|(metadata, _)| metadata.key_id)
                .map_err(|_| crate::runtime::RuntimeError::SignerUnavailable)
        }
        fn sign_payload_hash(
            &self,
            payload_hash: &str,
        ) -> Result<String, crate::runtime::RuntimeError> {
            self.0
                .sign_payload_hash(payload_hash)
                .map(|(_, signature)| signature)
                .map_err(|_| crate::runtime::RuntimeError::SignerUnavailable)
        }
    }

    fn make_signer() -> (TestSigner, KeyTransition) {
        let root =
            std::env::temp_dir().join(format!("evohime-export-test-{}", uuid::Uuid::now_v7()));
        let manager = std::sync::Arc::new(ReceiptKeyManager::new(&root));
        manager.initialize().unwrap();
        let genesis = manager.load_history().unwrap().into_iter().next().unwrap();
        (TestSigner(manager), genesis)
    }

    fn seed_chain(db: &mut Connection, signer: &TestSigner) {
        crate::runtime::install_schema(db).unwrap();
        let mut runtime = ReceiptRuntime::new(db, signer).unwrap();
        for index in 0..3 {
            let action_id = uuid::Uuid::now_v7();
            let request = ActionRequest {
                action_id,
                task_id: "task-01".into(),
                run_id: "run-01".into(),
                tool_name: format!("tool.read.{index}"),
                policy_id: "policy-01".into(),
                normalized_scope: "scope".into(),
                input: serde_json::json!({"x": index}),
                policy_decision: PolicyDecision::Allow,
                approval_id: None,
                parent_approval_ref: None,
                preview: "preview".into(),
            };
            runtime.prepare(request.clone()).unwrap();
            runtime.mark_started(action_id).unwrap();
            runtime
                .complete(&request, "succeeded", &"a".repeat(64), None)
                .unwrap();
        }
    }

    #[test]
    fn list_returns_bounded_rows_newest_first() {
        let (signer, _genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        seed_chain(&mut db, &signer);
        let result = list_receipts(&db, &ReceiptFilter::default(), 10).unwrap();
        assert_eq!(result.rows.len(), 6);
        assert!(result.rows[0].sequence > result.rows[1].sequence);
    }

    #[test]
    fn verify_reports_verified_chain() {
        let (signer, genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        seed_chain(&mut db, &signer);
        let result =
            verify_receipts(&db, &[genesis], None, &ReceiptFilter::default(), 500).unwrap();
        assert_eq!(
            result.verification.status,
            crate::chain::ChainStatus::Verified
        );
        assert_eq!(result.verification.actual_verified_count, 6);
    }

    #[test]
    fn compacted_prefix_reports_verified_pruned_and_exports_checkpoint() {
        let (signer, genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        seed_chain(&mut db, &signer); // 3 actions, 6 rows (pre+terminal each)
        let key_id = genesis.new_key_id.clone();
        let public_key = crate::key_lifecycle::public_key_bytes(&genesis.new_public_key).unwrap();
        {
            let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
            // Compact the first two actions (rows 1..4); retain the third.
            runtime.compact_chain(&key_id, 5).unwrap();
        }

        let result = verify_receipts(
            &db,
            &[genesis.clone()],
            None,
            &ReceiptFilter::default(),
            500,
        )
        .unwrap();
        assert_eq!(
            result.verification.status,
            crate::chain::ChainStatus::VerifiedPruned,
            "{:?}",
            result.verification
        );
        assert_eq!(result.verification.actual_verified_count, 2);

        let destination =
            std::env::temp_dir().join(format!("evohime-export-pruned-{}", uuid::Uuid::now_v7()));
        let manifest =
            export_receipts(&db, &[], &destination, &ReceiptFilter::default(), 1000).unwrap();
        assert_eq!(manifest.actual_exported_count, 2);
        assert!(
            destination.join("checkpoints.jsonl").exists(),
            "an active checkpoint must be exported alongside the retained suffix"
        );
        let checkpoints_content =
            std::fs::read_to_string(destination.join("checkpoints.jsonl")).unwrap();
        let checkpoint: crate::chain::ExportedCheckpoint =
            serde_json::from_str(checkpoints_content.trim_end()).unwrap();
        assert_eq!(checkpoint.key_id, key_id);
        assert_eq!(
            checkpoint.signed_by_key_id, key_id,
            "same signer/chain key in this single-key test"
        );
        assert!(
            crate::chain::verify_checkpoint_signature(&checkpoint, &public_key),
            "the exported checkpoint envelope must verify against the signing key's public key"
        );
        std::fs::remove_dir_all(&destination).unwrap();
    }

    #[test]
    fn export_writes_atomic_bundle() {
        let (signer, _genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        seed_chain(&mut db, &signer);
        let destination =
            std::env::temp_dir().join(format!("evohime-export-bundle-{}", uuid::Uuid::now_v7()));
        let manifest =
            export_receipts(&db, &[], &destination, &ReceiptFilter::default(), 1000).unwrap();
        assert_eq!(manifest.actual_exported_count, 6);
        assert!(destination.join("manifest.json").exists());
        assert!(destination.join("receipts.jsonl").exists());
        std::fs::remove_dir_all(&destination).unwrap();
    }

    #[test]
    fn export_rejects_existing_destination() {
        let (signer, _genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        seed_chain(&mut db, &signer);
        let destination =
            std::env::temp_dir().join(format!("evohime-export-exists-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&destination).unwrap();
        let result = export_receipts(&db, &[], &destination, &ReceiptFilter::default(), 1000);
        assert_eq!(result.unwrap_err(), ExportError::ExportExists);
        std::fs::remove_dir_all(&destination).unwrap();
    }

    /// Not run by default (`cargo test -- --ignored`). Regenerates the shared
    /// fixture behind `contracts/receipts/v1/export-vectors.json` from a real
    /// signed bundle so the vectors never drift from the actual byte shapes
    /// `export_receipts` produces.
    #[test]
    #[ignore]
    fn dump_export_vectors_for_contract() {
        let (signer, genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        seed_chain_single(&mut db, &signer);
        let destination =
            std::env::temp_dir().join(format!("evohime-export-vectors-{}", uuid::Uuid::now_v7()));
        let manifest = export_receipts(
            &db,
            &[genesis],
            &destination,
            &ReceiptFilter::default(),
            1000,
        )
        .unwrap();
        let receipts_jsonl = std::fs::read_to_string(destination.join("receipts.jsonl")).unwrap();
        let key_history_jsonl =
            std::fs::read_to_string(destination.join("key-history.jsonl")).unwrap();
        let manifest_json = std::fs::read_to_string(destination.join("manifest.json")).unwrap();
        println!(
            "{}",
            json!({
                "manifest_bytes": manifest_json,
                "receipts_jsonl_bytes": receipts_jsonl,
                "key_history_jsonl_bytes": key_history_jsonl,
                "actual_exported_count": manifest.actual_exported_count,
            })
        );
        std::fs::remove_dir_all(&destination).unwrap();
    }

    fn seed_chain_single(db: &mut Connection, signer: &TestSigner) {
        crate::runtime::install_schema(db).unwrap();
        let mut runtime = ReceiptRuntime::new(db, signer).unwrap();
        let action_id = uuid::Uuid::now_v7();
        let request = ActionRequest {
            action_id,
            task_id: "task-01".into(),
            run_id: "run-01".into(),
            tool_name: "tool.read".into(),
            policy_id: "policy-01".into(),
            normalized_scope: "scope".into(),
            input: serde_json::json!({"x": 1}),
            policy_decision: PolicyDecision::Allow,
            approval_id: None,
            parent_approval_ref: None,
            preview: "preview".into(),
        };
        runtime.prepare(request.clone()).unwrap();
        runtime.mark_started(action_id).unwrap();
        runtime
            .complete(&request, "succeeded", &"a".repeat(64), None)
            .unwrap();
    }
}
