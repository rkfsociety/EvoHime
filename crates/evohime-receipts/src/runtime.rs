//! Core-owned execution journal for Signed Receipt v1.
//!
//! This module deliberately keeps tool execution outside SQLite transactions:
//! callers prepare/claim a mutation, dispatch it, and then commit exactly one
//! terminal receipt.  The durable rows are the recovery source of truth.

use crate::{canonicalize_json, receipt_hash, result_hash, Envelope, ReceiptError};
use chrono::Utc;
use ring::{aead, rand::{SecureRandom, SystemRandom}};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use thiserror::Error;
use uuid::Uuid;

pub const APPROVAL_TTL_MS: i64 = 600_000;
pub const MAX_PENDING_ACTIONS: i64 = 1024;
pub const MAX_PREVIEW_BYTES: usize = crate::CONTRACT_MAX_PREVIEW_BYTES;
pub const MAX_CALL_INPUT_BYTES: usize = 262_144;
pub const MAX_PROTECTED_ROW_BYTES: usize = 512;
const BOUNDED_METRICS: &[&str] = &[
    "receipt_pre_latency_ms", "receipt_post_latency_ms", "receipt_append_latency_ms",
    "receipt_append_busy_retries", "receipt_chain_conflicts", "receipt_schema_violations",
    "approval_pending_count", "pending_recovery_count", "quarantined_count",
    "approval_gc_deleted_count", "recovery_duration_ms", "recovery_safe_mode",
    "read_only_sampled_count", "read_only_unsampled_count", "receipt_append_count",
];
static PROCESS_BOOT: OnceLock<Instant> = OnceLock::new();
static BOOT_ID: OnceLock<String> = OnceLock::new();

fn monotonic_ms() -> i64 { PROCESS_BOOT.get_or_init(Instant::now).elapsed().as_millis() as i64 }
fn boot_id() -> &'static str {
    BOOT_ID.get_or_init(|| {
        #[cfg(windows)]
        {
            // GetTickCount64 is monotonic from the Windows boot and therefore
            // distinguishes a reboot without relying on wall-clock changes.
            return format!("windows-boot-{}", unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() });
        }
        #[cfg(not(windows))]
        {
            // The production target is Windows; this fallback keeps non-Windows
            // contract tests process-local and still prevents stale deadlines
            // from being compared against a fresh monotonic clock.
            format!("process-boot-{:p}", &PROCESS_BOOT as *const _)
        }
    })
}

/// Runs before Core accepts any new mutation. This API intentionally does not
/// require a signer: recovery may inspect and expire state even when signing
/// is unavailable, while all writes to the chain remain blocked by the guard.
pub fn recover_database(connection: &mut Connection) -> Result<i64, RuntimeError> {
    let recovery_started = Instant::now();
    install_schema(connection)?;
    // Recovery must not wait indefinitely behind another writer. SQLite still
    // reports a busy/error condition; the guard remains non-ready on failure.
    connection.busy_timeout(Duration::from_secs(2))?;
    let tx = connection.unchecked_transaction()?;
    tx.execute("UPDATE receipt_runtime_guard SET phase='recovery_in_progress',generation=generation+1,updated_at_ms=?1 WHERE id=1", [now_ms()])?;
    let quick: String = tx.query_row("PRAGMA quick_check(100)", [], |r| r.get(0))?;
    if quick != "ok" {
        increment_metric_tx(&tx, "recovery_safe_mode", 1)?;
        tx.execute("UPDATE receipt_runtime_guard SET phase='read_only_recovery',updated_at_ms=?1 WHERE id=1", [now_ms()])?;
        tx.commit()?;
        return Err(RuntimeError::Code("schema_violation"));
    }
    let wall_now = now_ms(); let mono_now = monotonic_ms();
    tx.execute("UPDATE receipt_approval_intents SET state='expired' WHERE state IN ('pending','granted') AND ((clock_boot_id=?1 AND deadline_monotonic_ms<=?2) OR (clock_boot_id<>?1 AND expires_at_ms<=?3))", params![boot_id(), mono_now, wall_now])?;
    // A terminal post/refusal may have committed before the action-index
    // update. Reconcile only that durable receipt; never synthesize a result.
    tx.execute(
        "UPDATE receipt_actions SET
            state=(SELECT CASE WHEN r.receipt_kind='refusal' THEN 'refused' ELSE r.action_status END FROM receipt_records r WHERE r.receipt_hash=(SELECT r2.receipt_hash FROM receipt_records r2 WHERE r2.action_id=receipt_actions.action_id AND r2.receipt_kind IN ('post_action','refusal') ORDER BY r2.rowid DESC LIMIT 1)),
            dispatch_state='returned',
            terminal_receipt_hash=(SELECT r.receipt_hash FROM receipt_records r WHERE r.action_id=receipt_actions.action_id AND r.receipt_kind IN ('post_action','refusal') ORDER BY r.rowid DESC LIMIT 1),
            recovery_code=NULL
         WHERE state IN ('awaiting_approval','prepared','pending_recovery') AND terminal_receipt_hash IS NULL
           AND EXISTS (SELECT 1 FROM receipt_records r WHERE r.action_id=receipt_actions.action_id AND ((r.receipt_kind='post_action' AND receipt_actions.pre_receipt_hash IS NOT NULL) OR (r.receipt_kind='refusal' AND (receipt_actions.pre_receipt_hash IS NOT NULL OR receipt_actions.policy_decision IN ('deny','approval_required')))))",
        [],
    )?;
    let invariant_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM receipt_actions WHERE state IN ('prepared','pending_recovery') AND pre_receipt_hash IS NULL AND dispatch_state IN ('started','returned')",
        [], |row| row.get(0),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO receipt_runtime_diagnostics(code,action_id,detail_code,created_at_ms)
         SELECT 'receipt.schema_violation',action_id,'started_without_pre',?1 FROM receipt_actions
         WHERE state IN ('prepared','pending_recovery') AND pre_receipt_hash IS NULL AND dispatch_state IN ('started','returned')",
        [now_ms()],
    )?;
    // A dispatch transition is valid only after a durable pre receipt. These
    // rows are invariant violations, not recoverable executions.
    tx.execute("UPDATE receipt_actions SET state='quarantined',recovery_code='unknown' WHERE state IN ('prepared','pending_recovery') AND dispatch_state IN ('started','returned') AND pre_receipt_hash IS NULL", [])?;
    // A durable pre with no dispatch transition cannot be retried after a
    // restart: preserve it as an unknown recovery fact instead.
    tx.execute("UPDATE receipt_actions SET state='pending_recovery',recovery_code='unknown' WHERE state='prepared' AND dispatch_state='not_started' AND pre_receipt_hash IS NOT NULL AND terminal_receipt_hash IS NULL", [])?;
    // A crash after the pre transaction but before/around dispatch must never
    // cause an automatic retry. Preserve the action as an unknown result.
    tx.execute("UPDATE receipt_actions SET state='pending_recovery',recovery_code='unknown' WHERE state='prepared' AND dispatch_state IN ('started','returned') AND pre_receipt_hash IS NOT NULL AND terminal_receipt_hash IS NULL", [])?;
    let orphan_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM receipt_records r LEFT JOIN receipt_actions a ON a.action_id=r.action_id WHERE r.receipt_kind IN ('post_action','refusal') AND a.action_id IS NULL",
        [], |row| row.get(0),
    )?;
    let terminal_without_pre_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM receipt_records r JOIN receipt_actions a ON a.action_id=r.action_id WHERE r.receipt_kind='post_action' AND a.pre_receipt_hash IS NULL",
        [], |row| row.get(0),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO receipt_runtime_diagnostics(code,action_id,detail_code,created_at_ms)
         SELECT 'receipt.schema_violation',r.action_id,'orphan_terminal_receipt',?1 FROM receipt_records r
         LEFT JOIN receipt_actions a ON a.action_id=r.action_id
         WHERE r.receipt_kind IN ('post_action','refusal') AND a.action_id IS NULL LIMIT 128",
        [now_ms()],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO receipt_runtime_diagnostics(code,action_id,detail_code,created_at_ms)
         SELECT 'receipt.schema_violation',r.action_id,'terminal_without_pre',?1 FROM receipt_records r
         JOIN receipt_actions a ON a.action_id=r.action_id
         WHERE r.receipt_kind='post_action' AND a.pre_receipt_hash IS NULL LIMIT 128",
        [now_ms()],
    )?;
    if invariant_count > 0 || orphan_count > 0 || terminal_without_pre_count > 0 {
        increment_metric_tx(&tx, "receipt_schema_violations", invariant_count.saturating_add(orphan_count).saturating_add(terminal_without_pre_count))?;
        if invariant_count > 0 { increment_metric_tx(&tx, "quarantined_count", invariant_count)?; }
        increment_metric_tx(&tx, "recovery_safe_mode", 1)?;
        tx.execute("UPDATE receipt_runtime_guard SET phase='read_only_recovery',updated_at_ms=?1", [now_ms()])?;
        tx.commit()?;
        return Err(RuntimeError::Code("schema_violation"));
    }
    let pending: i64 = tx.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state IN ('prepared','pending_recovery','quarantined')", [], |r| r.get(0))?;
    increment_metric_tx(&tx, "recovery_duration_ms", recovery_started.elapsed().as_millis().min(i64::MAX as u128) as i64)?;
    tx.execute("UPDATE receipt_runtime_guard SET phase='ready',updated_at_ms=?1 WHERE id=1", [now_ms()])?;
    tx.commit()?;
    Ok(pending)
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("receipt.{0}")]
    Code(&'static str),
    #[error("receipt.sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("receipt.contract: {0}")]
    Contract(#[from] ReceiptError),
    #[error("receipt.signer_unavailable")]
    SignerUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProtectedActionRow {
    pub schema_version: u8,
    pub action_id: String,
    pub pre_receipt_hash: String,
    pub tool_args_hash: String,
    pub result_status: String,
    pub result_hash: String,
    pub recovery_code: String,
    pub created_at_ms: i64,
    pub key_id: String,
}

fn valid_recovery_code(value: &str) -> bool { matches!(value, "signature_failed" | "external_error" | "unknown") }

/// Encrypts the bounded recovery projection. The nonce and GCM tag are part
/// of the 512-byte envelope; truncation is never permitted.
pub fn protect_action_row(row: &ProtectedActionRow, key: &[u8; 32]) -> Result<Vec<u8>, RuntimeError> {
    if row.schema_version != 1 || !valid_recovery_code(&row.recovery_code) ||
        !matches!(row.result_status.as_str(), "succeeded" | "failed" | "cancelled") { return Err(RuntimeError::Code("schema_violation")); }
    let plaintext = canonicalize_json(&serde_json::to_vec(row).map_err(|_| ReceiptError::InvalidJson)?)?;
    if plaintext.len() + 28 > MAX_PROTECTED_ROW_BYTES { return Err(RuntimeError::Code("pending_recovery")); }
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key).map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    let key = aead::LessSafeKey::new(unbound);
    let mut nonce = [0u8; 12];
    SystemRandom::new().fill(&mut nonce).map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    let nonce_value = aead::Nonce::assume_unique_for_key(nonce);
    let mut ciphertext = plaintext;
    key.seal_in_place_append_tag(nonce_value, aead::Aad::empty(), &mut ciphertext).map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    let mut output = nonce.to_vec();
    output.extend_from_slice(&ciphertext);
    if output.len() > MAX_PROTECTED_ROW_BYTES { return Err(RuntimeError::Code("pending_recovery")); }
    Ok(output)
}

pub fn unprotect_action_row(envelope: &[u8], key: &[u8; 32]) -> Result<ProtectedActionRow, RuntimeError> {
    if envelope.len() < 28 || envelope.len() > MAX_PROTECTED_ROW_BYTES { return Err(RuntimeError::Code("pending_recovery")); }
    let mut nonce = [0u8; 12]; nonce.copy_from_slice(&envelope[..12]);
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key).map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    let key = aead::LessSafeKey::new(unbound);
    let mut ciphertext = envelope[12..].to_vec();
    let plaintext = key.open_in_place(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::empty(), &mut ciphertext)
        .map_err(|_| RuntimeError::Code("pending_recovery"))?;
    let value: Value = serde_json::from_slice(plaintext).map_err(|_| RuntimeError::Code("pending_recovery"))?;
    let row: ProtectedActionRow = serde_json::from_value(value).map_err(|_| RuntimeError::Code("pending_recovery"))?;
    if row.schema_version != 1 || !valid_recovery_code(&row.recovery_code) { return Err(RuntimeError::Code("pending_recovery")); }
    Ok(row)
}

pub fn sampled_read_only(action_id: &str, tool_name: &str, rate: u8) -> bool {
    if rate == 0 { return false; }
    let mut bytes = b"evohime-sample-v1\0".to_vec(); bytes.extend_from_slice(action_id.as_bytes()); bytes.push(0); bytes.extend_from_slice(tool_name.as_bytes());
    let digest = Sha256::digest(bytes);
    u16::from_be_bytes([digest[0], digest[1]]) % 100 < rate as u16
}

pub fn bounded_result_marker(status: &str, hash: &str, error_category: Option<&str>, returned_at_ms: i64, output_present: bool) -> Result<Vec<u8>, RuntimeError> {
    if !matches!(status, "succeeded" | "failed" | "cancelled") || hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { return Err(RuntimeError::Code("schema_violation")); }
    let value = json!({"schema_version":1,"result_status":status,"result_hash":hash,"error_category":error_category,"returned_at_ms":returned_at_ms,"output_present":output_present});
    let bytes = canonicalize_json(&serde_json::to_vec(&value).map_err(|_| ReceiptError::InvalidJson)?)?;
    if bytes.len() > 256 { return Err(RuntimeError::Code("pending_recovery")); }
    Ok(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Deny,
    ApprovalRequired,
}

impl PolicyDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::ApprovalRequired => "approval_required",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActionRequest {
    pub action_id: Uuid,
    pub task_id: String,
    pub run_id: String,
    pub tool_name: String,
    pub policy_id: String,
    pub normalized_scope: String,
    pub input: Value,
    pub policy_decision: PolicyDecision,
    pub approval_id: Option<Uuid>,
    pub parent_approval_ref: Option<String>,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareOutcome {
    Prepared { action_id: Uuid, receipt_hash: String },
    ApprovalRequired { action_id: Uuid, approval_id: Uuid, expires_at_ms: i64 },
    Refused { action_id: Uuid, receipt_hash: String, code: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionState {
    pub action_id: String,
    pub state: String,
    pub dispatch_state: String,
    pub pre_receipt_hash: Option<String>,
    pub terminal_receipt_hash: Option<String>,
    pub tool_args_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCounts {
    pub pending: i64,
    pub pending_recovery: i64,
    pub quarantined: i64,
    pub approval_pending: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    pub counters: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageRotationJob {
    pub job_id: String,
    pub old_key_id: String,
    pub new_key_id: String,
    pub cursor: String,
    pub generation: i64,
    pub state: String,
}

/// Signing boundary.  The signer receives the SHA-256 digest of canonical
/// payload bytes, never raw tool input or a mutable JSON representation.
pub trait ReceiptSigner: Send + Sync {
    fn key_id(&self) -> Result<String, RuntimeError>;
    fn sign_payload_hash(&self, payload_hash: &str) -> Result<String, RuntimeError>;
}

pub fn install_schema(connection: &Connection) -> Result<(), RuntimeError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS receipt_records (
           schema_version INTEGER NOT NULL DEFAULT 1,
           receipt_id TEXT PRIMARY KEY NOT NULL,
           action_id TEXT NOT NULL,
           receipt_kind TEXT NOT NULL CHECK(receipt_kind IN ('pre_action','post_action','refusal')),
           action_status TEXT NOT NULL,
           task_id TEXT NOT NULL,
           run_id TEXT NOT NULL,
           key_id TEXT NOT NULL,
           canonical_payload BLOB NOT NULL,
           canonical_envelope BLOB NOT NULL,
           receipt_hash TEXT NOT NULL UNIQUE,
           previous_receipt_hash TEXT,
           created_at_ms INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_receipt_records_action ON receipt_records(action_id);
         CREATE TABLE IF NOT EXISTS receipt_actions (
           schema_version INTEGER NOT NULL DEFAULT 1,
           action_id TEXT PRIMARY KEY NOT NULL,
           task_id TEXT NOT NULL,
           run_id TEXT NOT NULL,
           tool_name TEXT NOT NULL,
           normalized_scope TEXT NOT NULL DEFAULT '',
           fingerprint_input_version INTEGER NOT NULL DEFAULT 1,
           tool_args_hash TEXT NOT NULL,
           policy_id TEXT NOT NULL,
           policy_decision TEXT NOT NULL CHECK(policy_decision IN ('allow','deny','approval_required')),
           state TEXT NOT NULL CHECK(state IN ('awaiting_approval','prepared','refused','succeeded','failed','cancelled','pending_recovery','quarantined')),
           dispatch_state TEXT NOT NULL CHECK(dispatch_state IN ('not_started','started','returned')),
           approval_id TEXT,
           approval_call_hash TEXT,
           parent_approval_ref TEXT,
           legacy_approval_ref TEXT,
           pre_receipt_hash TEXT,
           terminal_receipt_hash TEXT,
           recovery_code TEXT,
           result_hash TEXT,
           result_marker BLOB,
           reconciliation_action_id TEXT,
           reconciles_action_id TEXT,
           completion_source TEXT NOT NULL DEFAULT 'execution' CHECK(completion_source IN ('execution','reconciliation')),
           tool_started_at_ms INTEGER,
           UNIQUE(action_id)
         );
         CREATE TABLE IF NOT EXISTS receipt_approval_intents (
           schema_version INTEGER NOT NULL DEFAULT 1,
           approval_id TEXT PRIMARY KEY NOT NULL,
           action_id TEXT NOT NULL UNIQUE REFERENCES receipt_actions(action_id),
           task_id TEXT NOT NULL,
           run_id TEXT NOT NULL,
           tool_name TEXT NOT NULL,
           normalized_scope TEXT NOT NULL,
           call_hash TEXT NOT NULL,
           preview TEXT NOT NULL,
           state TEXT NOT NULL CHECK(state IN ('pending','granted','denied','expired','claimed','lost')),
           created_wall_at_ms INTEGER NOT NULL,
           expires_at_ms INTEGER NOT NULL,
           clock_boot_id TEXT NOT NULL DEFAULT 'runtime-boot-v1',
           created_monotonic_ms INTEGER NOT NULL,
           deadline_monotonic_ms INTEGER NOT NULL,
           legacy_approval_ref TEXT
         );
         CREATE TABLE IF NOT EXISTS receipt_chain_heads (
           key_id TEXT PRIMARY KEY NOT NULL,
           receipt_hash TEXT NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_runtime_guard (
           id INTEGER PRIMARY KEY CHECK(id=1),
           phase TEXT NOT NULL CHECK(phase IN ('recovery_in_progress','ready','read_only_recovery')),
           generation INTEGER NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_protected_actions (
           action_id TEXT PRIMARY KEY NOT NULL REFERENCES receipt_actions(action_id),
           key_id TEXT NOT NULL,
           envelope BLOB NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_runtime_config (
           id INTEGER PRIMARY KEY CHECK(id=1),
           audit_sampling_rate INTEGER NOT NULL CHECK(audit_sampling_rate BETWEEN 0 AND 100),
           sampling_policy_version INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_audit_markers (
           action_id TEXT PRIMARY KEY NOT NULL,
           tool_name TEXT NOT NULL,
           call_hash TEXT NOT NULL,
           sampled INTEGER NOT NULL CHECK(sampled=0),
           sampling_policy_version INTEGER NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_sampling_changes (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           old_rate INTEGER NOT NULL,
           new_rate INTEGER NOT NULL,
           sampling_policy_version INTEGER NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_runtime_metrics (
           metric TEXT PRIMARY KEY NOT NULL,
           value INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_runtime_diagnostics (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           code TEXT NOT NULL,
           action_id TEXT NOT NULL,
           detail_code TEXT NOT NULL,
           created_at_ms INTEGER NOT NULL,
           UNIQUE(code,action_id,detail_code)
         );
         CREATE TABLE IF NOT EXISTS receipt_storage_rotation (
           id INTEGER PRIMARY KEY CHECK(id=1),
           job_id TEXT NOT NULL,
           old_key_id TEXT NOT NULL,
           new_key_id TEXT NOT NULL,
           cursor TEXT NOT NULL DEFAULT '',
           generation INTEGER NOT NULL,
           state TEXT NOT NULL CHECK(state IN ('running','completed','failed')),
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_storage_rotation_audit (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           job_id TEXT NOT NULL,
           cursor TEXT NOT NULL,
           old_key_id TEXT NOT NULL,
           new_key_id TEXT NOT NULL,
           processed_count INTEGER NOT NULL,
           outcome TEXT NOT NULL CHECK(outcome IN ('batch_committed','completed','failed')),
           created_at_ms INTEGER NOT NULL
         );
         INSERT OR IGNORE INTO receipt_runtime_guard(id,phase,generation,updated_at_ms)
           VALUES(1,'ready',0,0);",
    )?;
    let marker_column: Option<String> = connection.query_row(
        "SELECT name FROM pragma_table_info('receipt_actions') WHERE name='result_marker'",
        [], |row| row.get(0),
    ).optional()?;
    if marker_column.is_none() { connection.execute("ALTER TABLE receipt_actions ADD COLUMN result_marker BLOB", [])?; }
    let boot_column: Option<String> = connection.query_row("SELECT name FROM pragma_table_info('receipt_approval_intents') WHERE name='clock_boot_id'", [], |row| row.get(0)).optional()?;
    if boot_column.is_none() { connection.execute("ALTER TABLE receipt_approval_intents ADD COLUMN clock_boot_id TEXT NOT NULL DEFAULT 'legacy'", [])?; }
    connection.execute("INSERT OR IGNORE INTO receipt_runtime_config(id,audit_sampling_rate,sampling_policy_version) VALUES(1,?1,?2)", params![crate::DEFAULT_READ_ONLY_SAMPLING_RATE as i64, crate::SAMPLING_POLICY_VERSION as i64])?;
    for (table, column, definition) in [
        ("receipt_records", "schema_version", "INTEGER NOT NULL DEFAULT 1"),
        ("receipt_actions", "schema_version", "INTEGER NOT NULL DEFAULT 1"),
        ("receipt_actions", "normalized_scope", "TEXT NOT NULL DEFAULT ''"),
        ("receipt_actions", "fingerprint_input_version", "INTEGER NOT NULL DEFAULT 1"),
        ("receipt_approval_intents", "schema_version", "INTEGER NOT NULL DEFAULT 1"),
        ("receipt_approval_intents", "legacy_approval_ref", "TEXT"),
        ("receipt_actions", "reconciliation_action_id", "TEXT"),
        ("receipt_actions", "reconciles_action_id", "TEXT"),
        ("receipt_actions", "completion_source", "TEXT NOT NULL DEFAULT 'execution'"),
        ("receipt_actions", "parent_approval_ref", "TEXT"),
        ("receipt_actions", "legacy_approval_ref", "TEXT"),
    ] {
        let exists: Option<String> = connection.query_row(
            &format!("SELECT name FROM pragma_table_info('{table}') WHERE name='{column}'"),
            [], |row| row.get(0),
        ).optional()?;
        if exists.is_none() { connection.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"), [])?; }
    }
    Ok(())
}

pub fn canonical_call_hash(tool_name: &str, normalized_scope: &str, input: &Value) -> Result<String, RuntimeError> {
    if tool_name.contains('\n') || normalized_scope.contains('\n') {
        return Err(RuntimeError::Code("schema_violation"));
    }
    let fingerprint = evohime_permissions::fingerprint_input(input);
    if fingerprint.len() > MAX_CALL_INPUT_BYTES { return Err(RuntimeError::Code("call_input_too_large")); }
    Ok(evohime_permissions::canonical_call_hash(tool_name, normalized_scope, input))
}

fn now_ms() -> i64 { Utc::now().timestamp_millis() }

fn increment_metric_tx(tx: &Transaction<'_>, metric: &str, amount: i64) -> Result<(), RuntimeError> {
    if metric.is_empty() || metric.len() > 64 || !metric.bytes().all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'.') { return Err(RuntimeError::Code("schema_violation")); }
    tx.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES(?1,?2) ON CONFLICT(metric) DO UPDATE SET value=value+excluded.value", params![metric, amount])?;
    Ok(())
}

fn bounded_preview(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if out.len() + ch.len_utf8() > MAX_PREVIEW_BYTES.saturating_sub(11) { break; }
        out.push(ch);
    }
    if out.len() < value.len() { out.push_str("[truncated]"); }
    out
}

fn build_payload(request: &ActionRequest, kind: &str, status: &str, args_hash: &str,
                 previous: Option<&str>, result: Option<&str>, refusal: Option<&str>) -> Value {
    let mut payload = json!({
        "receipt_version": 1, "receipt_id": Uuid::now_v7().to_string(),
        "action_id": request.action_id.to_string(), "receipt_kind": kind,
        "action_status": status, "timestamp": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "task_id": request.task_id.clone(), "run_id": request.run_id.clone(), "tool_name": request.tool_name.clone(),
        "tool_args_hash": args_hash, "policy_id": request.policy_id.clone(),
        "policy_decision": request.policy_decision.as_str()
    });
    let object = payload.as_object_mut().expect("receipt payload is object");
    if let Some(value) = previous { object.insert("previous_receipt_hash".into(), Value::String(value.into())); }
    if let Some(value) = result { object.insert("result_hash".into(), Value::String(value.into())); }
    if let Some(value) = refusal { object.insert("refusal_code".into(), Value::String(value.into())); }
    if let Some(id) = request.approval_id { object.insert("approval_id".into(), Value::String(id.to_string())); }
    if kind != "post_action" {
        if let Some(parent) = request.parent_approval_ref.as_deref() { object.insert("parent_approval_ref".into(), Value::String(parent.into())); }
    }
    payload
}

fn signed_receipt(tx: &Transaction<'_>, signer: &dyn ReceiptSigner, request: &ActionRequest,
                  kind: &str, status: &str, args_hash: &str, result: Option<&str>, refusal: Option<&str>)
                  -> Result<(String, String), RuntimeError> {
    let append_started = Instant::now();
    let key_id = signer.key_id()?;
    let previous: Option<String> = tx.query_row("SELECT receipt_hash FROM receipt_chain_heads WHERE key_id=?1", [&key_id], |r| r.get(0)).optional()?;
    let last: Option<String> = tx.query_row("SELECT receipt_hash FROM receipt_records WHERE key_id=?1 ORDER BY created_at_ms DESC, rowid DESC LIMIT 1", [&key_id], |r| r.get(0)).optional()?;
    if previous != last { return Err(RuntimeError::Code("schema_violation")); }
    let payload = build_payload(request, kind, status, args_hash, previous.as_deref(), result, refusal);
    crate::validate_payload_v1(&payload)?;
    let payload_bytes = crate::payload_bytes(&payload)?;
    let payload_hash = crate::sha256_hex(&payload_bytes);
    let signature = signer.sign_payload_hash(&payload_hash)?;
    let envelope = Envelope { payload: payload.clone(), key_id: key_id.clone(), signature_algorithm: "Ed25519".into(), signature };
    let envelope_bytes = canonicalize_json(&serde_json::to_vec(&envelope).map_err(|_| ReceiptError::InvalidJson)?)?;
    let hash = receipt_hash(&envelope)?;
    let object = payload.as_object().unwrap();
    tx.execute("INSERT INTO receipt_records(schema_version,receipt_id,action_id,receipt_kind,action_status,task_id,run_id,key_id,canonical_payload,canonical_envelope,receipt_hash,previous_receipt_hash,created_at_ms) VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![object["receipt_id"].as_str(), request.action_id.to_string(), kind, status, request.task_id, request.run_id, key_id, payload_bytes, envelope_bytes, hash, previous, now_ms()])?;
    tx.execute("INSERT INTO receipt_chain_heads(key_id,receipt_hash,updated_at_ms) VALUES(?1,?2,?3) ON CONFLICT(key_id) DO UPDATE SET receipt_hash=excluded.receipt_hash,updated_at_ms=excluded.updated_at_ms", params![key_id, hash, now_ms()])?;
    increment_metric_tx(tx, "receipt_append_count", 1)?;
    let elapsed = append_started.elapsed().as_millis().min(i64::MAX as u128) as i64;
    increment_metric_tx(tx, "receipt_append_latency_ms", elapsed)?;
    increment_metric_tx(tx, if kind == "pre_action" { "receipt_pre_latency_ms" } else if kind == "post_action" { "receipt_post_latency_ms" } else { "receipt_refusal_latency_ms" }, elapsed)?;
    if kind == "pre_action" && matches!(request.tool_name.as_str(), "filesystem.read" | "filesystem.list" | "git.status" | "git.diff" | "workspace.list" | "workspace.read" | "workspace.search") {
        increment_metric_tx(tx, "read_only_sampled_count", 1)?;
    }
    Ok((hash, payload_hash))
}

pub struct ReceiptRuntime<'a> {
    connection: &'a mut Connection,
    signer: &'a dyn ReceiptSigner,
}

impl<'a> ReceiptRuntime<'a> {
    pub fn new(connection: &'a mut Connection, signer: &'a dyn ReceiptSigner) -> Result<Self, RuntimeError> {
        install_schema(connection)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        Ok(Self { connection, signer })
    }

    pub fn prepare(&self, request: ActionRequest) -> Result<PrepareOutcome, RuntimeError> {
        self.prepare_inner(request, false)
    }

    /// Imports an already Core-created approval id from the PermissionEngine.
    /// This is only for the compatibility approval producer; the renderer
    /// still cannot choose an id.
    pub fn prepare_existing_approval(&self, request: ActionRequest) -> Result<PrepareOutcome, RuntimeError> {
        self.prepare_inner(request, true)
    }

    /// Imports a legacy in-memory approval as a new pending Core approval.
    /// The legacy identifier is audit-only and cannot authorize the new claim.
    pub fn import_legacy_approval(&self, legacy_ref: &str, request: ActionRequest) -> Result<PrepareOutcome, RuntimeError> {
        if legacy_ref.is_empty() || legacy_ref.len() > 128 { return Err(RuntimeError::Code("schema_violation")); }
        if !matches!(request.policy_decision, PolicyDecision::ApprovalRequired) || request.approval_id.is_some() { return Err(RuntimeError::Code("approval_stale")); }
        let outcome = self.prepare(request)?;
        let PrepareOutcome::ApprovalRequired { action_id, approval_id, expires_at_ms } = outcome else { return Err(RuntimeError::Code("schema_violation")); };
        self.connection.execute("UPDATE receipt_actions SET legacy_approval_ref=?2 WHERE action_id=?1", params![action_id.to_string(), legacy_ref])?;
        self.connection.execute("UPDATE receipt_approval_intents SET legacy_approval_ref=?2 WHERE approval_id=?1", params![approval_id.to_string(), legacy_ref])?;
        Ok(PrepareOutcome::ApprovalRequired { action_id, approval_id, expires_at_ms })
    }

    fn prepare_inner(&self, mut request: ActionRequest, existing_approval: bool) -> Result<PrepareOutcome, RuntimeError> {
        let phase: String = self.connection.query_row("SELECT phase FROM receipt_runtime_guard WHERE id=1", [], |row| row.get(0))?;
        if phase != "ready" { return Err(RuntimeError::Code("pending_recovery")); }
        if request.action_id.get_version_num() != 7 { return Err(RuntimeError::Code("schema_violation")); }
        request.preview = bounded_preview(&request.preview);
        if request.parent_approval_ref.as_ref().is_some_and(|value| value.is_empty() || value.len() > 256) { return Err(RuntimeError::Code("schema_violation")); }
        let args_hash = canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input)?;
        let tx = self.connection.unchecked_transaction()?;
        if tx.query_row("SELECT 1 FROM receipt_actions WHERE action_id=?1", [request.action_id.to_string()], |_| Ok(1)).optional()?.is_some() {
            return Err(RuntimeError::Code("action_id_conflict"));
        }
        let pending: i64 = tx.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state IN ('prepared','pending_recovery') AND task_id=?1", [&request.task_id], |r| r.get(0))?;
        if pending >= MAX_PENDING_ACTIONS { return Err(RuntimeError::Code("pending_limit")); }
        let action = request.action_id.to_string();
        let initial_state = if matches!(request.policy_decision, PolicyDecision::ApprovalRequired) { "awaiting_approval" } else { "prepared" };
        tx.execute("INSERT INTO receipt_actions(schema_version,action_id,task_id,run_id,tool_name,normalized_scope,fingerprint_input_version,tool_args_hash,policy_id,policy_decision,state,dispatch_state,approval_id,approval_call_hash,parent_approval_ref) VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'not_started',?11,?7,?12)", params![action, request.task_id, request.run_id, request.tool_name, request.normalized_scope, crate::FINGERPRINT_INPUT_VERSION, args_hash, request.policy_id, request.policy_decision.as_str(), initial_state, request.approval_id.map(|v|v.to_string()), request.parent_approval_ref])?;
        match request.policy_decision {
            PolicyDecision::Deny => {
                let (hash, _) = signed_receipt(&tx, self.signer, &request, "refusal", "refused", &args_hash, None, Some("policy_denied"))?;
                tx.execute("UPDATE receipt_actions SET state='refused',terminal_receipt_hash=?2 WHERE action_id=?1", params![action, hash])?;
                tx.commit()?;
                Ok(PrepareOutcome::Refused { action_id: request.action_id, receipt_hash: hash, code: "policy_denied".into() })
            }
            PolicyDecision::ApprovalRequired => {
                if request.approval_id.is_some() && !existing_approval {
                    return Err(RuntimeError::Code("approval_stale"));
                }
                let approval_id = request.approval_id.unwrap_or_else(Uuid::now_v7);
                if approval_id.get_version_num() != 7 { return Err(RuntimeError::Code("schema_violation")); }
                let created = now_ms();
                let created_monotonic = monotonic_ms();
                let expires = created + APPROVAL_TTL_MS;
                let deadline = created_monotonic + APPROVAL_TTL_MS;
                tx.execute("INSERT INTO receipt_approval_intents(approval_id,action_id,task_id,run_id,tool_name,normalized_scope,call_hash,preview,state,created_wall_at_ms,expires_at_ms,clock_boot_id,created_monotonic_ms,deadline_monotonic_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'pending',?9,?10,?11,?12,?13)", params![approval_id.to_string(), action, request.task_id, request.run_id, request.tool_name, request.normalized_scope, args_hash, request.preview, created, expires, boot_id(), created_monotonic, deadline])?;
                increment_metric_tx(&tx, "approval_pending_count", 1)?;
                tx.commit()?;
                Ok(PrepareOutcome::ApprovalRequired { action_id: request.action_id, approval_id, expires_at_ms: expires })
            }
            PolicyDecision::Allow => {
                let (hash, _) = signed_receipt(&tx, self.signer, &request, "pre_action", "prepared", &args_hash, None, None)?;
                tx.execute("UPDATE receipt_actions SET pre_receipt_hash=?2,state='prepared' WHERE action_id=?1", params![action, hash])?;
                tx.commit()?;
                Ok(PrepareOutcome::Prepared { action_id: request.action_id, receipt_hash: hash })
            }
        }
    }

    pub fn mark_started(&self, action_id: Uuid) -> Result<(), RuntimeError> {
        let changed = self.connection.execute("UPDATE receipt_actions SET dispatch_state='started',tool_started_at_ms=?2 WHERE action_id=?1 AND state='prepared' AND dispatch_state='not_started'", params![action_id.to_string(), now_ms()])?;
        if changed != 1 { return Err(RuntimeError::Code("action_id_conflict")); }
        Ok(())
    }

    pub fn mark_returned(&self, action_id: Uuid) -> Result<(), RuntimeError> {
        let changed = self.connection.execute(
            "UPDATE receipt_actions SET dispatch_state='returned' WHERE action_id=?1 AND state='prepared' AND dispatch_state='started'",
            [action_id.to_string()],
        )?;
        if changed != 1 { return Err(RuntimeError::Code("action_id_conflict")); }
        Ok(())
    }

    /// Recovery never synthesizes a successful result.  It only expires
    /// in-flight approvals and leaves started actions available for an
    /// authenticated reconciliation path.
    pub fn recover_on_startup(&mut self) -> Result<i64, RuntimeError> {
        recover_database(self.connection)
    }

    /// Records the UI decision without holding the IPC request open.  A
    /// decision is one-way and does not itself authorize dispatch.
    pub fn grant_approval(&self, approval_id: Uuid) -> Result<(), RuntimeError> {
        let deadline: Option<(String, i64, i64)> = self.connection.query_row(
            "SELECT clock_boot_id,expires_at_ms,deadline_monotonic_ms FROM receipt_approval_intents WHERE approval_id=?1 AND state='pending'",
            [approval_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?;
        let valid = deadline.is_some_and(|(boot, wall, mono)| if boot == boot_id() { monotonic_ms() < mono } else { now_ms() < wall });
        let changed = if valid { self.connection.execute("UPDATE receipt_approval_intents SET state='granted' WHERE approval_id=?1 AND state='pending'", [approval_id.to_string()])? } else { self.connection.execute("UPDATE receipt_approval_intents SET state='expired' WHERE approval_id=?1 AND state='pending'", [approval_id.to_string()])?; 0 };
        if changed != 1 { return Err(RuntimeError::Code("approval_expired")); }
        Ok(())
    }

    /// Claims a granted intent and appends the pre receipt atomically.  The
    /// caller must provide the same call fields that produced the intent.
    pub fn claim_approval(&mut self, request: &ActionRequest, approval_id: Uuid) -> Result<PrepareOutcome, RuntimeError> {
        let args_hash = canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input)?;
        let tx = self.connection.unchecked_transaction()?;
        let row: Option<(String, String, i64)> = tx.query_row(
            "SELECT action_id,state,expires_at_ms FROM receipt_approval_intents WHERE approval_id=?1",
            [approval_id.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).optional()?;
        let Some((action_id, state, expires)) = row else { return Err(RuntimeError::Code("approval_stale")); };
        if state != "granted" || expires <= now_ms() { return Err(RuntimeError::Code("approval_expired")); }
        let stored: String = tx.query_row("SELECT tool_args_hash FROM receipt_actions WHERE action_id=?1", [&action_id], |r| r.get(0))?;
        if stored != args_hash || request.action_id.to_string() != action_id { return Err(RuntimeError::Code("call_changed")); }
        let mut bound = request.clone();
        bound.approval_id = Some(approval_id);
        let (hash, _) = signed_receipt(&tx, self.signer, &bound, "pre_action", "prepared", &stored, None, None)?;
        tx.execute("UPDATE receipt_approval_intents SET state='claimed' WHERE approval_id=?1 AND state='granted'", [approval_id.to_string()])?;
        tx.execute("UPDATE receipt_actions SET state='prepared',approval_id=?2,approval_call_hash=?3,pre_receipt_hash=?4 WHERE action_id=?1", params![action_id, approval_id.to_string(), stored, hash])?;
        tx.commit()?;
        Ok(PrepareOutcome::Prepared { action_id: request.action_id, receipt_hash: hash })
    }

    pub fn complete(&self, request: &ActionRequest, status: &str, output_digest: &str, error_category: Option<&str>) -> Result<String, RuntimeError> {
        self.complete_inner(request, status, output_digest, error_category, None)
    }

    /// Completes a separately authorized read-only reconciliation action and
    /// links it to the historical pending action in the same transaction.
    pub fn complete_reconciliation(&self, request: &ActionRequest, old_action_id: Uuid, status: &str, output_digest: &str, error_category: Option<&str>) -> Result<String, RuntimeError> {
        if old_action_id == request.action_id { return Err(RuntimeError::Code("schema_violation")); }
        self.complete_inner(request, status, output_digest, error_category, Some(old_action_id))
    }

    fn complete_inner(&self, request: &ActionRequest, status: &str, output_digest: &str, error_category: Option<&str>, reconciliation_old_action: Option<Uuid>) -> Result<String, RuntimeError> {
        if !matches!(status, "succeeded"|"failed"|"cancelled") { return Err(RuntimeError::Code("schema_violation")); }
        let args_hash = canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input)?;
        if output_digest.len() != 64 || !output_digest.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { return Err(RuntimeError::Code("schema_violation")); }
        let projection = if status == "succeeded" { json!({"status":"succeeded","output_digest":output_digest}) } else { json!({"status":status,"error_category":error_category.ok_or(RuntimeError::Code("schema_violation"))?}) };
        let result = result_hash(&projection)?;
        let marker = bounded_result_marker(status, &result, error_category, now_ms(), status == "succeeded")?;
        let tx = self.connection.unchecked_transaction()?;
        let (state, dispatch, stored_hash, task_id, run_id, tool_name, normalized_scope, policy_id, policy_decision, fingerprint_version, stored_approval, stored_parent, terminal_hash): (String,String,String,String,String,String,String,String,String,i64,Option<String>,Option<String>,Option<String>) = tx.query_row(
            "SELECT state,dispatch_state,tool_args_hash,task_id,run_id,tool_name,normalized_scope,policy_id,policy_decision,fingerprint_input_version,approval_id,parent_approval_ref,terminal_receipt_hash FROM receipt_actions WHERE action_id=?1",
            [request.action_id.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?,r.get(12)?)))?;
        let binding_matches = state == "prepared"
            || state == "pending_recovery";
        let identity_matches = request.task_id == task_id
            && request.run_id == run_id
            && request.tool_name == tool_name
            && request.normalized_scope == normalized_scope
            && request.policy_id == policy_id
            && request.policy_decision.as_str() == policy_decision
            && fingerprint_version == crate::FINGERPRINT_INPUT_VERSION as i64
            && request.approval_id.map(|value| value.to_string()) == stored_approval
            && request.parent_approval_ref == stored_parent
            && stored_hash == args_hash;
        if terminal_hash.is_some() { return Err(RuntimeError::Code("action_id_conflict")); }
        if !binding_matches || dispatch != "started" && dispatch != "returned" || !identity_matches {
            if binding_matches {
                tx.execute("UPDATE receipt_actions SET state='quarantined',recovery_code='unknown' WHERE action_id=?1 AND state IN ('prepared','pending_recovery')", [request.action_id.to_string()])?;
                tx.commit()?;
            }
            return Err(RuntimeError::Code("schema_violation"));
        }
        if let Some(old_action_id) = reconciliation_old_action {
            let old_state: String = tx.query_row("SELECT state FROM receipt_actions WHERE action_id=?1", [old_action_id.to_string()], |row| row.get(0))?;
            if old_state != "pending_recovery" {
                return Err(RuntimeError::Code("pending_recovery"));
            }
        }
        let (hash, _) = signed_receipt(&tx, self.signer, request, "post_action", status, &stored_hash, Some(&result), None)?;
        tx.execute("UPDATE receipt_actions SET state=?2,dispatch_state='returned',result_hash=?3,result_marker=?4,terminal_receipt_hash=?5 WHERE action_id=?1", params![request.action_id.to_string(), status, result, marker, hash])?;
        if let Some(old_action_id) = reconciliation_old_action {
            let old_linked = tx.execute("UPDATE receipt_actions SET state='succeeded',reconciliation_action_id=?2,completion_source='reconciliation',terminal_receipt_hash=?3 WHERE action_id=?1 AND state='pending_recovery' AND reconciliation_action_id IS NULL", params![old_action_id.to_string(), request.action_id.to_string(), hash])?;
            let new_linked = tx.execute("UPDATE receipt_actions SET reconciles_action_id=?2,completion_source='reconciliation' WHERE action_id=?1 AND state=?3 AND reconciles_action_id IS NULL", params![request.action_id.to_string(), old_action_id.to_string(), status])?;
            if old_linked != 1 || new_linked != 1 { return Err(RuntimeError::Code("pending_recovery")); }
            tx.execute("DELETE FROM receipt_protected_actions WHERE action_id=?1", [old_action_id.to_string()])?;
        }
        tx.commit()?;
        Ok(hash)
    }

    pub fn refuse(&self, request: &ActionRequest, code: &str) -> Result<String, RuntimeError> {
        if !matches!(code, "policy_denied"|"approval_denied"|"approval_expired"|"approval_stale"|"call_changed"|"key_untrusted"|"recovery_pending") { return Err(RuntimeError::Code("schema_violation")); }
        let args_hash = canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input)?;
        let tx = self.connection.unchecked_transaction()?;
        let binding: (String,String,String,String,String,String,i64,Option<String>,Option<String>,String,Option<String>) = tx.query_row(
            "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,policy_decision,fingerprint_input_version,approval_id,parent_approval_ref,state,terminal_receipt_hash FROM receipt_actions WHERE action_id=?1",
            [request.action_id.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?)))?;
        let identity_matches = request.task_id == binding.0
            && request.run_id == binding.1
            && request.tool_name == binding.2
            && request.normalized_scope == binding.3
            && request.policy_id == binding.4
            && request.policy_decision.as_str() == binding.5
            && binding.6 == crate::FINGERPRINT_INPUT_VERSION as i64
            && request.approval_id.map(|value| value.to_string()) == binding.7
            && request.parent_approval_ref == binding.8;
        if binding.10.is_some() { return Err(RuntimeError::Code("action_id_conflict")); }
        if !identity_matches || !matches!(binding.9.as_str(), "awaiting_approval" | "prepared" | "pending_recovery") {
            if matches!(binding.9.as_str(), "prepared" | "pending_recovery") {
                tx.execute("UPDATE receipt_actions SET state='quarantined',recovery_code='unknown' WHERE action_id=?1", [request.action_id.to_string()])?;
                tx.commit()?;
            }
            return Err(RuntimeError::Code("schema_violation"));
        }
        let stored_hash: String = tx.query_row("SELECT tool_args_hash FROM receipt_actions WHERE action_id=?1", [request.action_id.to_string()], |r| r.get(0))?;
        if stored_hash != args_hash {
            if matches!(binding.9.as_str(), "prepared" | "pending_recovery") {
                tx.execute("UPDATE receipt_actions SET state='quarantined',recovery_code='unknown' WHERE action_id=?1", [request.action_id.to_string()])?;
                tx.commit()?;
            }
            return Err(RuntimeError::Code("schema_violation"));
        }
        let (hash, _) = signed_receipt(&tx, self.signer, request, "refusal", "refused", &stored_hash, None, Some(code))?;
        let source = if code == "recovery_pending" { "reconciliation" } else { "execution" };
        tx.execute("UPDATE receipt_actions SET state='refused',completion_source=?3,terminal_receipt_hash=?2 WHERE action_id=?1", params![request.action_id.to_string(), hash, source])?;
        let approval_state = match code { "approval_expired" => "expired", "approval_denied" => "denied", _ => "lost" };
        tx.execute("UPDATE receipt_approval_intents SET state=?2 WHERE action_id=?1 AND state IN ('pending','granted')", params![request.action_id.to_string(), approval_state])?;
        tx.execute("DELETE FROM receipt_protected_actions WHERE action_id=?1", [request.action_id.to_string()])?;
        tx.commit()?;
        Ok(hash)
    }

    pub fn mark_pending_recovery(&self, action_id: Uuid, code: &str) -> Result<(), RuntimeError> {
        if !valid_recovery_code(code) { return Err(RuntimeError::Code("schema_violation")); }
        let changed = self.connection.execute("UPDATE receipt_actions SET state='pending_recovery',recovery_code=?2 WHERE action_id=?1 AND dispatch_state IN ('started','returned')", params![action_id.to_string(), code])?;
        if changed == 1 {
            self.connection.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES('pending_recovery_count',1) ON CONFLICT(metric) DO UPDATE SET value=value+1", [])?;
        }
        Ok(())
    }

    /// Links a new, separately authorized read-only reconciliation action to a
    /// pending historical action. The original action is never dispatched by
    /// this operation and remains visible in its original audit state.
    pub fn link_reconciliation(&self, old_action_id: Uuid, new_action_id: Uuid) -> Result<(), RuntimeError> {
        if old_action_id == new_action_id { return Err(RuntimeError::Code("schema_violation")); }
        let tx = self.connection.unchecked_transaction()?;
        let old_state: String = tx.query_row("SELECT state FROM receipt_actions WHERE action_id=?1", [old_action_id.to_string()], |row| row.get(0))?;
        let new_state: String = tx.query_row("SELECT state FROM receipt_actions WHERE action_id=?1", [new_action_id.to_string()], |row| row.get(0))?;
        if old_state != "pending_recovery" || !matches!(new_state.as_str(), "prepared"|"succeeded"|"failed"|"cancelled") {
            return Err(RuntimeError::Code("pending_recovery"));
        }
        tx.execute("UPDATE receipt_actions SET reconciliation_action_id=?2 WHERE action_id=?1 AND reconciliation_action_id IS NULL", params![old_action_id.to_string(), new_action_id.to_string()])?;
        tx.execute("UPDATE receipt_actions SET reconciles_action_id=?2,completion_source='reconciliation' WHERE action_id=?1 AND reconciles_action_id IS NULL", params![new_action_id.to_string(), old_action_id.to_string()])?;
        tx.commit()?;
        Ok(())
    }

    pub fn approval_gc(&self, now_ms_value: i64) -> Result<i64, RuntimeError> {
        let phase: String = self.connection.query_row("SELECT phase FROM receipt_runtime_guard WHERE id=1", [], |row| row.get(0))?;
        if phase != "ready" { return Err(RuntimeError::Code("pending_recovery")); }
        let cutoff = now_ms_value.saturating_sub(APPROVAL_TTL_MS);
        let deleted = self.connection.execute(
            "DELETE FROM receipt_approval_intents WHERE state IN ('expired','lost','claimed') AND expires_at_ms<=?1 AND NOT EXISTS (SELECT 1 FROM receipt_actions a WHERE a.action_id=receipt_approval_intents.action_id AND a.state='pending_recovery')",
            [cutoff],
        )?;
        self.connection.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES('approval_gc_deleted_count',?1) ON CONFLICT(metric) DO UPDATE SET value=value+excluded.value", [deleted as i64])?;
        Ok(deleted as i64)
    }

    pub fn store_protected_action(&self, row: &ProtectedActionRow, key: &[u8; 32]) -> Result<(), RuntimeError> {
        let envelope = protect_action_row(row, key)?;
        self.connection.execute(
            "INSERT INTO receipt_protected_actions(action_id,key_id,envelope,created_at_ms) VALUES(?1,?2,?3,?4) ON CONFLICT(action_id) DO UPDATE SET key_id=excluded.key_id,envelope=excluded.envelope",
            params![row.action_id, row.key_id, envelope, row.created_at_ms],
        )?;
        Ok(())
    }

    pub fn store_protected_envelope(&self, row: &ProtectedActionRow, envelope: Vec<u8>) -> Result<(), RuntimeError> {
        if envelope.len() > MAX_PROTECTED_ROW_BYTES { return Err(RuntimeError::Code("storage_key_unavailable")); }
        self.connection.execute(
            "INSERT INTO receipt_protected_actions(action_id,key_id,envelope,created_at_ms) VALUES(?1,?2,?3,?4) ON CONFLICT(action_id) DO UPDATE SET key_id=excluded.key_id,envelope=excluded.envelope",
            params![row.action_id, row.key_id, envelope, row.created_at_ms],
        )?;
        Ok(())
    }

    /// Rewraps one bounded batch of protected rows. The callback performs the
    /// key-manager operation outside SQLite; the durable cursor is advanced
    /// only in the same short transaction as the envelope replacement.
    pub fn rewrap_protected_batch<F>(
        &mut self,
        job_id: &str,
        old_key_id: &str,
        new_key_id: &str,
        generation: i64,
        batch_size: usize,
        rewrap: F,
    ) -> Result<bool, RuntimeError>
    where
        F: Fn(&[u8]) -> Result<Vec<u8>, RuntimeError>,
    {
        if job_id.is_empty() || job_id.len() > 128 || old_key_id.is_empty() || old_key_id.len() > 128
            || new_key_id.is_empty() || new_key_id.len() > 128 || batch_size == 0 || batch_size > 64
        {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let current: Option<(String, String, String, i64, String)> = self.connection.query_row(
            "SELECT job_id,old_key_id,new_key_id,generation,state FROM receipt_storage_rotation WHERE id=1",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        ).optional()?;
        if let Some((current_job, current_old, current_new, current_generation, state)) = current {
            if state == "completed" {
                self.connection.execute("UPDATE receipt_storage_rotation SET job_id=?1,old_key_id=?2,new_key_id=?3,cursor='',generation=?4,state='running',updated_at_ms=?5 WHERE id=1", params![job_id, old_key_id, new_key_id, generation, now_ms()])?;
            } else if current_job != job_id || current_old != old_key_id || current_new != new_key_id || current_generation != generation {
                return Err(RuntimeError::Code("chain_conflict"));
            }
        } else {
            self.connection.execute(
                "INSERT INTO receipt_storage_rotation(id,job_id,old_key_id,new_key_id,cursor,generation,state,updated_at_ms) VALUES(1,?1,?2,?3,'',?4,'running',?5)",
                params![job_id, old_key_id, new_key_id, generation, now_ms()],
            )?;
        }
        let cursor: String = self.connection.query_row("SELECT cursor FROM receipt_storage_rotation WHERE id=1", [], |row| row.get(0))?;
        let rows: Vec<(String, Vec<u8>)> = {
            let mut statement = self.connection.prepare(
                "SELECT action_id,envelope FROM receipt_protected_actions WHERE action_id>?1 ORDER BY action_id LIMIT ?2",
            )?;
            let mapped = statement.query_map(params![cursor, batch_size as i64], |row| Ok((row.get(0)?, row.get(1)?)))?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        if rows.is_empty() {
            self.connection.execute("UPDATE receipt_storage_rotation SET state='completed',updated_at_ms=?1 WHERE id=1", [now_ms()])?;
            self.connection.execute("INSERT INTO receipt_storage_rotation_audit(job_id,cursor,old_key_id,new_key_id,processed_count,outcome,created_at_ms) VALUES(?1,?2,?3,?4,0,'completed',?5)", params![job_id, cursor, old_key_id, new_key_id, now_ms()])?;
            return Ok(false);
        }
        let mut replacement = Vec::with_capacity(rows.len());
        for (action_id, envelope) in rows {
            let next = match rewrap(&envelope) {
                Ok(value) => value,
                Err(error) => {
                    self.connection.execute("UPDATE receipt_storage_rotation SET state='failed',updated_at_ms=?1 WHERE id=1", [now_ms()])?;
                    self.connection.execute("INSERT INTO receipt_storage_rotation_audit(job_id,cursor,old_key_id,new_key_id,processed_count,outcome,created_at_ms) VALUES(?1,?2,?3,?4,0,'failed',?5)", params![job_id, cursor, old_key_id, new_key_id, now_ms()])?;
                    return Err(error);
                }
            };
            if next.len() > MAX_PROTECTED_ROW_BYTES {
                self.connection.execute("UPDATE receipt_storage_rotation SET state='failed',updated_at_ms=?1 WHERE id=1", [now_ms()])?;
                self.connection.execute("INSERT INTO receipt_storage_rotation_audit(job_id,cursor,old_key_id,new_key_id,processed_count,outcome,created_at_ms) VALUES(?1,?2,?3,?4,0,'failed',?5)", params![job_id, cursor, old_key_id, new_key_id, now_ms()])?;
                return Err(RuntimeError::Code("storage_key_unavailable"));
            }
            replacement.push((action_id, next));
        }
        let tx = self.connection.unchecked_transaction()?;
        let mut last = String::new();
        let processed_count = replacement.len() as i64;
        for (action_id, envelope) in replacement {
            tx.execute("UPDATE receipt_protected_actions SET key_id=?2,envelope=?3 WHERE action_id=?1", params![action_id, new_key_id, envelope])?;
            last = action_id;
        }
        tx.execute("UPDATE receipt_storage_rotation SET cursor=?1,state='running',updated_at_ms=?2 WHERE id=1", params![last, now_ms()])?;
        tx.execute("INSERT INTO receipt_storage_rotation_audit(job_id,cursor,old_key_id,new_key_id,processed_count,outcome,created_at_ms) VALUES(?1,?2,?3,?4,?5,'batch_committed',?6)", params![job_id, last, old_key_id, new_key_id, processed_count, now_ms()])?;
        tx.commit()?;
        Ok(true)
    }

    pub fn load_protected_action(&self, action_id: Uuid, key: &[u8; 32]) -> Result<ProtectedActionRow, RuntimeError> {
        let (stored_key_id, envelope): (String, Vec<u8>) = self.connection.query_row("SELECT key_id,envelope FROM receipt_protected_actions WHERE action_id=?1", [action_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let row = unprotect_action_row(&envelope, key)?;
        if row.action_id != action_id.to_string() || row.key_id != stored_key_id {
            return Err(RuntimeError::Code("pending_recovery"));
        }
        Ok(row)
    }

    pub fn delete_protected_after_terminal(&self, action_id: Uuid) -> Result<(), RuntimeError> {
        let terminal: Option<String> = self.connection.query_row("SELECT state FROM receipt_actions WHERE action_id=?1", [action_id.to_string()], |row| row.get(0)).optional()?;
        if !matches!(terminal.as_deref(), Some("succeeded"|"failed"|"cancelled"|"refused")) { return Err(RuntimeError::Code("pending_recovery")); }
        self.connection.execute("DELETE FROM receipt_protected_actions WHERE action_id=?1", [action_id.to_string()])?;
        Ok(())
    }

    pub fn quarantine(&self, action_id: Uuid, reason: &str) -> Result<(), RuntimeError> {
        if reason.is_empty() || reason.len() > 128 { return Err(RuntimeError::Code("schema_violation")); }
        let changed = self.connection.execute("UPDATE receipt_actions SET state='quarantined',recovery_code='unknown' WHERE action_id=?1 AND state IN ('prepared','pending_recovery')", [action_id.to_string()])?;
        if changed != 1 { return Err(RuntimeError::Code("schema_violation")); }
        self.connection.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES('quarantined_count',1) ON CONFLICT(metric) DO UPDATE SET value=value+1", [])?;
        Ok(())
    }

    /// Authenticated operator closure for an invariant-violating action. It
    /// can only produce a signed terminal refusal and never re-enables dispatch.
    pub fn unquarantine(&self, request: &ActionRequest, authenticated_operator: bool, checkpoint: &str) -> Result<String, RuntimeError> {
        if !authenticated_operator || checkpoint.is_empty() || checkpoint.len() > 256 { return Err(RuntimeError::Code("key_untrusted")); }
        let args_hash = canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input)?;
        let tx = self.connection.unchecked_transaction()?;
        let binding: (String,String,String,String,String,String,i64,Option<String>,Option<String>,String,Option<String>) = tx.query_row(
            "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,policy_decision,fingerprint_input_version,approval_id,parent_approval_ref,state,terminal_receipt_hash FROM receipt_actions WHERE action_id=?1",
            [request.action_id.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?)))?;
        if binding.9 != "quarantined" || binding.10.is_some()
            || request.task_id != binding.0
            || request.run_id != binding.1
            || request.tool_name != binding.2
            || request.normalized_scope != binding.3
            || request.policy_id != binding.4
            || request.policy_decision.as_str() != binding.5
            || binding.6 != crate::FINGERPRINT_INPUT_VERSION as i64
            || request.approval_id.map(|value| value.to_string()) != binding.7
            || request.parent_approval_ref != binding.8
        {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let stored_hash: String = tx.query_row("SELECT tool_args_hash FROM receipt_actions WHERE action_id=?1", [request.action_id.to_string()], |row| row.get(0))?;
        if stored_hash != args_hash { return Err(RuntimeError::Code("schema_violation")); }
        let (hash, _) = signed_receipt(&tx, self.signer, request, "refusal", "refused", &stored_hash, None, Some("recovery_pending"))?;
        tx.execute("UPDATE receipt_actions SET state='refused',recovery_code='unknown',completion_source='reconciliation',terminal_receipt_hash=?2 WHERE action_id=?1 AND state='quarantined'", params![request.action_id.to_string(), hash])?;
        tx.execute("DELETE FROM receipt_protected_actions WHERE action_id=?1", [request.action_id.to_string()])?;
        tx.commit()?;
        Ok(hash)
    }

    pub fn action(&self, action_id: Uuid) -> Result<Option<ActionState>, RuntimeError> {
        Ok(self.connection.query_row("SELECT action_id,state,dispatch_state,pre_receipt_hash,terminal_receipt_hash,tool_args_hash FROM receipt_actions WHERE action_id=?1", [action_id.to_string()], |r| Ok(ActionState { action_id:r.get(0)?,state:r.get(1)?,dispatch_state:r.get(2)?,pre_receipt_hash:r.get(3)?,terminal_receipt_hash:r.get(4)?,tool_args_hash:r.get(5)? })).optional()?)
    }

    pub fn approval_deadline(&self, approval_id: Uuid) -> Result<(i64, i64), RuntimeError> {
        self.connection.query_row(
            "SELECT created_monotonic_ms,deadline_monotonic_ms FROM receipt_approval_intents WHERE approval_id=?1",
            [approval_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).map_err(RuntimeError::from)
    }

    pub fn audit_sampling_config(&self) -> Result<(u8, u8), RuntimeError> {
        self.connection.query_row("SELECT audit_sampling_rate,sampling_policy_version FROM receipt_runtime_config WHERE id=1", [], |row| Ok((row.get::<_, i64>(0)? as u8, row.get::<_, i64>(1)? as u8))).map_err(RuntimeError::from)
    }

    pub fn set_audit_sampling_rate(&self, authenticated_core_command: bool, rate: u8) -> Result<(), RuntimeError> {
        if !authenticated_core_command || rate > 100 { return Err(RuntimeError::Code("key_untrusted")); }
        let tx = self.connection.unchecked_transaction()?;
        let old_rate: i64 = tx.query_row("SELECT audit_sampling_rate FROM receipt_runtime_config WHERE id=1", [], |row| row.get(0))?;
        tx.execute("UPDATE receipt_runtime_config SET audit_sampling_rate=?1,sampling_policy_version=?2 WHERE id=1", params![rate as i64, crate::SAMPLING_POLICY_VERSION as i64])?;
        tx.execute("INSERT INTO receipt_sampling_changes(old_rate,new_rate,sampling_policy_version,created_at_ms) VALUES(?1,?2,?3,?4)", params![old_rate, rate as i64, crate::SAMPLING_POLICY_VERSION as i64, now_ms()])?;
        tx.commit()?;
        Ok(())
    }

    pub fn store_unsampled_read_only_marker(&self, action_id: Uuid, tool_name: &str, call_hash: &str, policy_version: u8) -> Result<(), RuntimeError> {
        if tool_name.is_empty() || tool_name.len() > crate::MAX_IDENTIFIER_BYTES || call_hash.len() != 64 || !call_hash.bytes().all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase()) { return Err(RuntimeError::Code("schema_violation")); }
        self.connection.execute("INSERT INTO receipt_audit_markers(action_id,tool_name,call_hash,sampled,sampling_policy_version,created_at_ms) VALUES(?1,?2,?3,0,?4,?5)", params![action_id.to_string(), tool_name, call_hash, policy_version as i64, now_ms()])?;
        self.connection.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES('read_only_unsampled_count',1) ON CONFLICT(metric) DO UPDATE SET value=value+1", [])?;
        Ok(())
    }

    /// Records a bounded unsigned runtime fact. It is deliberately stored in
    /// diagnostics only and can never be interpreted as a receipt or advance
    /// the hash chain.
    pub fn store_unsigned_runtime_marker(&self, action_id: Uuid, code: &str) -> Result<(), RuntimeError> {
        if !matches!(code, "signer_unavailable" | "storage_key_unavailable") {
            return Err(RuntimeError::Code("schema_violation"));
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO receipt_runtime_diagnostics(code,action_id,detail_code,created_at_ms) VALUES('receipt.unsigned_audit',?1,?2,?3)",
            params![action_id.to_string(), code, now_ms()],
        )?;
        Ok(())
    }

    pub fn counts(&self) -> Result<RuntimeCounts, RuntimeError> {
        Ok(RuntimeCounts {
            pending: self.connection.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state IN ('awaiting_approval','prepared','pending_recovery')", [], |r| r.get(0))?,
            pending_recovery: self.connection.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state='pending_recovery'", [], |r| r.get(0))?,
            quarantined: self.connection.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state='quarantined'", [], |r| r.get(0))?,
            approval_pending: self.connection.query_row("SELECT COUNT(*) FROM receipt_approval_intents WHERE state='pending'", [], |r| r.get(0))?,
        })
    }

    pub fn metrics(&self) -> Result<RuntimeMetrics, RuntimeError> {
        let mut statement = self.connection.prepare("SELECT metric,value FROM receipt_runtime_metrics ORDER BY metric LIMIT 128")?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        let mut counters = BTreeMap::new();
        for row in rows { let (metric, value) = row?; counters.insert(metric, value); }
        for metric in BOUNDED_METRICS {
            counters.entry((*metric).to_owned()).or_insert(0);
        }
        Ok(RuntimeMetrics { counters })
    }

    pub fn storage_rotation_job(&self) -> Result<Option<StorageRotationJob>, RuntimeError> {
        Ok(self.connection.query_row(
            "SELECT job_id,old_key_id,new_key_id,cursor,generation,state FROM receipt_storage_rotation WHERE id=1",
            [], |row| Ok(StorageRotationJob {
                job_id: row.get(0)?, old_key_id: row.get(1)?, new_key_id: row.get(2)?,
                cursor: row.get(3)?, generation: row.get(4)?, state: row.get(5)?,
            }),
        ).optional()?)
    }

    pub fn diagnostic_counts(&self) -> Result<BTreeMap<String, i64>, RuntimeError> {
        let mut statement = self.connection.prepare("SELECT code,COUNT(*) FROM receipt_runtime_diagnostics GROUP BY code ORDER BY code LIMIT 16")?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        let mut counts = BTreeMap::new();
        for row in rows { let (code, count) = row?; counts.insert(code, count); }
        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    struct TestSigner;
    impl ReceiptSigner for TestSigner {
        fn key_id(&self) -> Result<String, RuntimeError> { Ok("test-key".into()) }
        fn sign_payload_hash(&self, hash: &str) -> Result<String, RuntimeError> { Ok(hash.to_string()) }
    }

    fn request(policy: PolicyDecision) -> ActionRequest {
        ActionRequest { action_id: Uuid::now_v7(), task_id:"task-1".into(), run_id:"run-1".into(), tool_name:"filesystem.write".into(), policy_id:"policy-v1".into(), normalized_scope:"workspace".into(), input:json!({"path":"a.txt","content":"x"}), policy_decision:policy, approval_id:None, parent_approval_ref:None, preview:"write a file".into() }
    }

    #[test]
    fn deny_is_terminal_and_never_creates_pre() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let req = request(PolicyDecision::Deny);
        let id = req.action_id;
        assert!(matches!(runtime.prepare(req), Ok(PrepareOutcome::Refused { .. })));
        assert_eq!(runtime.action(id).unwrap().unwrap().pre_receipt_hash, None);
        assert_eq!(runtime.action(id).unwrap().unwrap().state, "refused");
    }

    #[test]
    fn approval_is_two_phase_and_claimed_once() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let req = request(PolicyDecision::ApprovalRequired);
        let id = req.action_id;
        let approval = match runtime.prepare(req.clone()).unwrap() { PrepareOutcome::ApprovalRequired { approval_id, .. } => approval_id, _ => panic!() };
        let (created_mono, deadline_mono) = runtime.approval_deadline(approval).unwrap();
        assert_eq!(deadline_mono - created_mono, APPROVAL_TTL_MS);
        assert_eq!(runtime.action(id).unwrap().unwrap().pre_receipt_hash, None);
        runtime.grant_approval(approval).unwrap();
        assert!(matches!(runtime.claim_approval(&req, approval), Ok(PrepareOutcome::Prepared { .. })));
        assert!(runtime.claim_approval(&req, approval).is_err());
    }

    #[test]
    fn legacy_approval_import_creates_new_pending_id_without_auto_grant() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let req = request(PolicyDecision::ApprovalRequired);
        let outcome = runtime.import_legacy_approval("legacy-approval-1", req).unwrap();
        let (approval_id, action_id) = match outcome { PrepareOutcome::ApprovalRequired { approval_id, action_id, .. } => (approval_id, action_id), _ => panic!() };
        let (state, legacy): (String, String) = db.query_row("SELECT state,legacy_approval_ref FROM receipt_approval_intents WHERE approval_id=?1", [approval_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(state, "pending");
        assert_eq!(legacy, "legacy-approval-1");
        assert_eq!(db.query_row("SELECT legacy_approval_ref FROM receipt_actions WHERE action_id=?1", [action_id.to_string()], |row| row.get::<_, String>(0)).unwrap(), "legacy-approval-1");
    }

    #[test]
    fn pre_is_durable_before_started_and_post_uses_chain_head() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let req = request(PolicyDecision::Allow);
        let id = req.action_id;
        let pre = runtime.prepare(req.clone()).unwrap();
        assert!(matches!(pre, PrepareOutcome::Prepared { .. }));
        assert_eq!(runtime.action(id).unwrap().unwrap().dispatch_state, "not_started");
        runtime.mark_started(id).unwrap();
        let post = runtime.complete(&req, "succeeded", &"a".repeat(64), None).unwrap();
        assert!(!post.is_empty());
        assert_eq!(runtime.action(id).unwrap().unwrap().state, "succeeded");
    }

    #[test]
    fn post_binding_mismatch_quarantines_without_terminal_success() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let request = request(PolicyDecision::Allow);
        let id = request.action_id;
        runtime.prepare(request.clone()).unwrap();
        runtime.mark_started(id).unwrap();
        let mut changed = request;
        changed.tool_name = "filesystem.other_write".into();
        assert!(matches!(
            runtime.complete(&changed, "succeeded", &"a".repeat(64), None),
            Err(RuntimeError::Code("schema_violation"))
        ));
        let state = runtime.action(id).unwrap().unwrap();
        assert_eq!(state.state, "quarantined");
        assert!(state.terminal_receipt_hash.is_none());
    }

    #[test]
    fn durable_terminal_hash_blocks_duplicate_post_append() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let request = request(PolicyDecision::Allow);
        let id = request.action_id;
        runtime.prepare(request.clone()).unwrap();
        runtime.mark_started(id).unwrap();
        runtime.mark_returned(id).unwrap();
        runtime.complete(&request, "succeeded", &"a".repeat(64), None).unwrap();
        assert!(matches!(runtime.complete(&request, "succeeded", &"b".repeat(64), None), Err(RuntimeError::Code("action_id_conflict"))));
        drop(runtime);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM receipt_records WHERE action_id=?1", [id.to_string()], |row| row.get::<_, i64>(0)).unwrap(), 2);
    }

    #[test]
    fn authenticated_unquarantine_is_terminal_and_never_dispatchable() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let request = request(PolicyDecision::Allow);
        let id = request.action_id;
        runtime.prepare(request.clone()).unwrap();
        runtime.mark_started(id).unwrap();
        runtime.mark_pending_recovery(id, "unknown").unwrap();
        runtime.quarantine(id, "invariant").unwrap();
        runtime.unquarantine(&request, true, "checkpoint-1").unwrap();
        assert_eq!(runtime.action(id).unwrap().unwrap().state, "refused");
        assert!(runtime.mark_started(id).is_err());
        assert!(runtime.unquarantine(&request, true, "checkpoint-1").is_err());
    }

    #[test]
    fn protected_row_is_authenticated_bounded_and_fail_closed() {
        let row = ProtectedActionRow { schema_version: 1, action_id: Uuid::now_v7().to_string(), pre_receipt_hash: "a".repeat(64), tool_args_hash: "b".repeat(64), result_status: "failed".into(), result_hash: "c".repeat(64), recovery_code: "external_error".into(), created_at_ms: 1, key_id: "key-1".into() };
        let key = [7u8; 32];
        let envelope = protect_action_row(&row, &key).unwrap();
        assert!(envelope.len() <= MAX_PROTECTED_ROW_BYTES);
        assert_eq!(unprotect_action_row(&envelope, &key).unwrap(), row);
        let mut tampered = envelope.clone(); tampered[13] ^= 1;
        assert!(unprotect_action_row(&tampered, &key).is_err());
    }

    #[test]
    fn protected_rotation_advances_cursor_and_rewraps_without_losing_rows() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let request = request(PolicyDecision::Allow);
        let action_id = request.action_id;
        runtime.prepare(request).unwrap();
        runtime.mark_started(action_id).unwrap();
        let row = ProtectedActionRow { schema_version: 1, action_id: action_id.to_string(), pre_receipt_hash: "a".repeat(64), tool_args_hash: "b".repeat(64), result_status: "failed".into(), result_hash: "c".repeat(64), recovery_code: "external_error".into(), created_at_ms: 1, key_id: "old".into() };
        runtime.store_protected_action(&row, &[1u8; 32]).unwrap();
        assert!(runtime.rewrap_protected_batch("job-1", "old", "new", 1, 8, |envelope| {
            let mut plain = unprotect_action_row(envelope, &[1u8; 32])?;
            plain.key_id = "new".into();
            protect_action_row(&plain, &[2u8; 32])
        }).unwrap());
        assert!(!runtime.rewrap_protected_batch("job-1", "old", "new", 1, 8, |envelope| {
            let mut plain = unprotect_action_row(envelope, &[2u8; 32])?;
            plain.key_id = "new".into();
            protect_action_row(&plain, &[2u8; 32])
        }).unwrap());
        let rewrapped_key_id = runtime.load_protected_action(action_id, &[2u8; 32]).unwrap().key_id;
        let old_key_rejected = runtime.load_protected_action(action_id, &[1u8; 32]).is_err();
        drop(runtime);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM receipt_storage_rotation_audit WHERE job_id='job-1'", [], |row| row.get::<_, i64>(0)).unwrap(), 2);
        assert_eq!(rewrapped_key_id, "new");
        assert!(old_key_rejected);
    }

    #[test]
    fn reconciliation_completion_links_old_and_new_actions_atomically() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let old = request(PolicyDecision::Allow);
        let old_id = old.action_id;
        runtime.prepare(old.clone()).unwrap();
        runtime.mark_started(old_id).unwrap();
        runtime.mark_returned(old_id).unwrap();
        runtime.mark_pending_recovery(old_id, "unknown").unwrap();
        let mut new = request(PolicyDecision::Allow);
        new.action_id = Uuid::now_v7();
        new.tool_name = "filesystem.read".into();
        runtime.prepare(new.clone()).unwrap();
        runtime.mark_started(new.action_id).unwrap();
        runtime.mark_returned(new.action_id).unwrap();
        runtime.complete_reconciliation(&new, old_id, "succeeded", &"d".repeat(64), None).unwrap();
        let links: (String, String, String, String, String) = db.query_row(
            "SELECT o.state,o.reconciliation_action_id,n.reconciles_action_id,o.completion_source,n.completion_source FROM receipt_actions o JOIN receipt_actions n ON n.action_id=o.reconciliation_action_id WHERE o.action_id=?1",
            [old_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        ).unwrap();
        assert_eq!(links.0, "succeeded");
        assert_eq!(links.1, new.action_id.to_string());
        assert_eq!(links.2, old_id.to_string());
        assert_eq!(links.3, "reconciliation");
        assert_eq!(links.4, "reconciliation");
    }

    #[test]
    fn sampling_is_deterministic_and_zero_does_not_sample() {
        assert_eq!(sampled_read_only("018f0f2a-2222-7222-8222-222222222222", "filesystem.read", 10), sampled_read_only("018f0f2a-2222-7222-8222-222222222222", "filesystem.read", 10));
        assert!(!sampled_read_only("action", "filesystem.read", 0));
        assert!(sampled_read_only("action", "filesystem.read", 100));
    }

    #[test]
    fn shared_runtime_error_manifest_has_contiguous_alias_ranges() {
        let manifest: Value = serde_json::from_str(include_str!("../../../contracts/receipts/v1/version-manifest.json")).unwrap();
        let transport = manifest["transport_runtime_error_codes"].as_object().unwrap();
        let mut transport_codes = transport.values().map(|value| value.as_i64().unwrap()).collect::<Vec<_>>();
        transport_codes.sort_unstable();
        assert_eq!(transport_codes, (1001_i64..=1013).collect::<Vec<_>>());
        let aliases = manifest["canonical_refusal_aliases"].as_object().unwrap();
        let mut alias_codes = aliases.values().map(|value| value.as_i64().unwrap()).collect::<Vec<_>>();
        alias_codes.sort_unstable();
        assert_eq!(alias_codes, (2001_i64..=2008).collect::<Vec<_>>());
    }

    #[test]
    fn unsigned_runtime_marker_never_creates_a_receipt_or_chain_head() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let action_id = Uuid::now_v7();
        runtime.store_unsigned_runtime_marker(action_id, "signer_unavailable").unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM receipt_records", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM receipt_chain_heads", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(db.query_row("SELECT detail_code FROM receipt_runtime_diagnostics WHERE action_id=?1", [action_id.to_string()], |row| row.get::<_, String>(0)).unwrap(), "signer_unavailable");
    }

    #[test]
    fn startup_recovery_expires_only_intents_and_never_synthesizes_success() {
        let mut db = Connection::open_in_memory().unwrap();
        install_schema(&db).unwrap();
        let pending = recover_database(&mut db).unwrap();
        assert_eq!(pending, 0);
        let phase: String = db.query_row("SELECT phase FROM receipt_runtime_guard WHERE id=1", [], |row| row.get(0)).unwrap();
        assert_eq!(phase, "ready");
    }

    #[test]
    fn approval_gc_is_blocked_until_recovery_guard_is_ready() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        ReceiptRuntime::new(&mut db, &signer).unwrap();
        db.execute("UPDATE receipt_runtime_guard SET phase='read_only_recovery' WHERE id=1", []).unwrap();
        let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        assert!(matches!(runtime.approval_gc(now_ms()), Err(RuntimeError::Code("pending_recovery"))));
    }

    #[test]
    fn startup_recovery_quarantines_started_without_pre_and_preserves_unknown_result() {
        let mut db = Connection::open_in_memory().unwrap();
        install_schema(&db).unwrap();
        db.execute("INSERT INTO receipt_actions(schema_version,action_id,task_id,run_id,tool_name,normalized_scope,fingerprint_input_version,tool_args_hash,policy_id,policy_decision,state,dispatch_state) VALUES(1,?1,'task','run','shell.execute','workspace',1,?2,'policy','allow','prepared','started')", params![Uuid::now_v7().to_string(), "a".repeat(64)]).unwrap();
        let action_id = db.query_row("SELECT action_id FROM receipt_actions LIMIT 1", [], |row| row.get::<_, String>(0)).unwrap();
        assert!(matches!(recover_database(&mut db), Err(RuntimeError::Code("schema_violation"))));
        let (state, code): (String, Option<String>) = db.query_row("SELECT state,recovery_code FROM receipt_actions WHERE action_id=?1", [action_id], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(state, "quarantined");
        assert_eq!(code.as_deref(), Some("unknown"));
        assert_eq!(db.query_row("SELECT phase FROM receipt_runtime_guard WHERE id=1", [], |row| row.get::<_, String>(0)).unwrap(), "read_only_recovery");
    }

    #[test]
    fn startup_recovery_rebuilds_action_index_from_durable_terminal_post() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let request = request(PolicyDecision::Allow);
        let id = request.action_id;
        {
            let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
            runtime.prepare(request.clone()).unwrap();
            runtime.mark_started(id).unwrap();
            runtime.mark_returned(id).unwrap();
            runtime.complete(&request, "failed", &"a".repeat(64), Some("tool_error")).unwrap();
        }
        db.execute("UPDATE receipt_actions SET state='prepared',terminal_receipt_hash=NULL WHERE action_id=?1", [id.to_string()]).unwrap();
        assert_eq!(recover_database(&mut db).unwrap(), 0);
        let (state, terminal): (String, Option<String>) = db.query_row("SELECT state,terminal_receipt_hash FROM receipt_actions WHERE action_id=?1", [id.to_string()], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(state, "failed");
        assert!(terminal.is_some());
    }

    #[test]
    fn startup_recovery_enters_safe_mode_for_orphan_terminal_receipt() {
        let mut db = Connection::open_in_memory().unwrap();
        let signer = TestSigner;
        let request = request(PolicyDecision::Allow);
        let id = request.action_id;
        {
            let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
            runtime.prepare(request.clone()).unwrap();
            runtime.mark_started(id).unwrap();
            runtime.mark_returned(id).unwrap();
            runtime.complete(&request, "succeeded", &"a".repeat(64), None).unwrap();
        }
        db.execute("DELETE FROM receipt_actions WHERE action_id=?1", [id.to_string()]).unwrap();
        assert!(matches!(recover_database(&mut db), Err(RuntimeError::Code("schema_violation"))));
        assert_eq!(db.query_row("SELECT phase FROM receipt_runtime_guard WHERE id=1", [], |row| row.get::<_, String>(0)).unwrap(), "read_only_recovery");
        assert_eq!(db.query_row("SELECT detail_code FROM receipt_runtime_diagnostics LIMIT 1", [], |row| row.get::<_, String>(0)).unwrap(), "orphan_terminal_receipt");
    }

    #[test]
    fn recovery_matrix_covers_all_eight_pre_started_post_combinations() {
        let cases = [
            (false, false, false, false, false),
            (false, false, true, true, true),
            (true, false, false, false, false),
            (true, false, true, false, false),
            (true, true, false, false, false),
            (true, true, true, false, false),
            (false, true, false, true, true),
            (false, true, true, true, true),
        ];
        for (pre, started, post, expect_safe_mode, expect_pending) in cases {
            let mut db = Connection::open_in_memory().unwrap();
            install_schema(&db).unwrap();
            if !pre && !started && !post {
                assert_eq!(recover_database(&mut db).unwrap(), 0);
                continue;
            }
            let signer = TestSigner;
            let request = request(PolicyDecision::Allow);
            let id = request.action_id;
            {
                let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
                runtime.prepare(request.clone()).unwrap();
                if started || post {
                    runtime.mark_started(id).unwrap();
                    runtime.mark_returned(id).unwrap();
                    if post { runtime.complete(&request, "succeeded", &"a".repeat(64), None).unwrap(); }
                }
            }
            db.execute("UPDATE receipt_actions SET state='prepared',dispatch_state=?2,terminal_receipt_hash=NULL,pre_receipt_hash=CASE WHEN ?3 THEN pre_receipt_hash ELSE NULL END WHERE action_id=?1", params![id.to_string(), if started { "started" } else { "not_started" }, pre]).unwrap();
            if !pre { db.execute("DELETE FROM receipt_records WHERE action_id=?1 AND receipt_kind='pre_action'", [id.to_string()]).unwrap(); }
            let recovery = recover_database(&mut db);
            assert_eq!(recovery.is_err(), expect_safe_mode, "case pre={pre} started={started} post={post}");
            if expect_safe_mode {
                assert_eq!(db.query_row("SELECT phase FROM receipt_runtime_guard WHERE id=1", [], |row| row.get::<_, String>(0)).unwrap(), "read_only_recovery");
            } else {
                let state: String = db.query_row("SELECT state FROM receipt_actions WHERE action_id=?1", [id.to_string()], |row| row.get(0)).unwrap();
                if expect_pending { assert_eq!(state, "pending_recovery"); } else if post { assert_eq!(state, "succeeded"); } else { assert_eq!(state, "pending_recovery"); }
            }
        }
    }

    #[test]
    fn parallel_file_backed_prepares_keep_a_single_verifiable_chain_head() {
        let path = std::env::temp_dir().join(format!("evohime-receipt-concurrency-{}.db", Uuid::now_v7()));
        let connection = Connection::open(&path).unwrap();
        install_schema(&connection).unwrap();
        drop(connection);
        let first = request(PolicyDecision::Allow);
        let second = request(PolicyDecision::Allow);
        std::thread::scope(|scope| {
            scope.spawn(|| { let mut db = Connection::open(&path).unwrap(); let signer = TestSigner; let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap(); assert!(matches!(runtime.prepare(first), Ok(PrepareOutcome::Prepared { .. }))); });
            scope.spawn(|| { let mut db = Connection::open(&path).unwrap(); let signer = TestSigner; let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap(); assert!(matches!(runtime.prepare(second), Ok(PrepareOutcome::Prepared { .. }))); });
        });
        let db = Connection::open(&path).unwrap();
        let records: i64 = db.query_row("SELECT COUNT(*) FROM receipt_records", [], |row| row.get(0)).unwrap();
        let heads: i64 = db.query_row("SELECT COUNT(*) FROM receipt_chain_heads", [], |row| row.get(0)).unwrap();
        assert_eq!(records, 2);
        assert_eq!(heads, 1);
        let head: String = db.query_row("SELECT receipt_hash FROM receipt_chain_heads WHERE key_id='test-key'", [], |row| row.get(0)).unwrap();
        let last: String = db.query_row("SELECT receipt_hash FROM receipt_records WHERE key_id='test-key' ORDER BY rowid DESC LIMIT 1", [], |row| row.get(0)).unwrap();
        assert_eq!(head, last);
        let _ = std::fs::remove_file(path);
    }
}
