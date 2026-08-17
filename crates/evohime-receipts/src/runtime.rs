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
use std::time::Instant;
use thiserror::Error;
use uuid::Uuid;

pub const APPROVAL_TTL_MS: i64 = 600_000;
pub const MAX_PENDING_ACTIONS: i64 = 1024;
pub const MAX_PREVIEW_BYTES: usize = 1024;
pub const MAX_CALL_INPUT_BYTES: usize = 262_144;
pub const MAX_PROTECTED_ROW_BYTES: usize = 512;
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
    install_schema(connection)?;
    let tx = connection.unchecked_transaction()?;
    tx.execute("UPDATE receipt_runtime_guard SET phase='recovery_in_progress',generation=generation+1,updated_at_ms=?1 WHERE id=1", [now_ms()])?;
    let quick: String = tx.query_row("PRAGMA quick_check(100)", [], |r| r.get(0))?;
    if quick != "ok" {
        tx.execute("UPDATE receipt_runtime_guard SET phase='read_only_recovery',updated_at_ms=?1 WHERE id=1", [now_ms()])?;
        tx.commit()?;
        return Err(RuntimeError::Code("schema_violation"));
    }
    let wall_now = now_ms(); let mono_now = monotonic_ms();
    tx.execute("UPDATE receipt_approval_intents SET state='expired' WHERE state IN ('pending','granted') AND ((clock_boot_id=?1 AND deadline_monotonic_ms<=?2) OR (clock_boot_id<>?1 AND expires_at_ms<=?3))", params![boot_id(), mono_now, wall_now])?;
    let pending: i64 = tx.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state IN ('prepared','pending_recovery','quarantined')", [], |r| r.get(0))?;
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
           deadline_monotonic_ms INTEGER NOT NULL
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
    for (table, column, definition) in [
        ("receipt_records", "schema_version", "INTEGER NOT NULL DEFAULT 1"),
        ("receipt_actions", "schema_version", "INTEGER NOT NULL DEFAULT 1"),
        ("receipt_actions", "normalized_scope", "TEXT NOT NULL DEFAULT ''"),
        ("receipt_actions", "fingerprint_input_version", "INTEGER NOT NULL DEFAULT 1"),
        ("receipt_approval_intents", "schema_version", "INTEGER NOT NULL DEFAULT 1"),
        ("receipt_actions", "reconciliation_action_id", "TEXT"),
        ("receipt_actions", "reconciles_action_id", "TEXT"),
        ("receipt_actions", "completion_source", "TEXT NOT NULL DEFAULT 'execution'"),
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
    payload
}

fn signed_receipt(tx: &Transaction<'_>, signer: &dyn ReceiptSigner, request: &ActionRequest,
                  kind: &str, status: &str, args_hash: &str, result: Option<&str>, refusal: Option<&str>)
                  -> Result<(String, String), RuntimeError> {
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
    Ok((hash, payload_hash))
}

pub struct ReceiptRuntime<'a> {
    connection: &'a mut Connection,
    signer: &'a dyn ReceiptSigner,
}

impl<'a> ReceiptRuntime<'a> {
    pub fn new(connection: &'a mut Connection, signer: &'a dyn ReceiptSigner) -> Result<Self, RuntimeError> {
        install_schema(connection)?;
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

    fn prepare_inner(&self, mut request: ActionRequest, existing_approval: bool) -> Result<PrepareOutcome, RuntimeError> {
        let phase: String = self.connection.query_row("SELECT phase FROM receipt_runtime_guard WHERE id=1", [], |row| row.get(0))?;
        if phase != "ready" { return Err(RuntimeError::Code("pending_recovery")); }
        if request.action_id.get_version_num() != 7 { return Err(RuntimeError::Code("schema_violation")); }
        request.preview = bounded_preview(&request.preview);
        let args_hash = canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input)?;
        let tx = self.connection.unchecked_transaction()?;
        if tx.query_row("SELECT 1 FROM receipt_actions WHERE action_id=?1", [request.action_id.to_string()], |_| Ok(1)).optional()?.is_some() {
            return Err(RuntimeError::Code("action_id_conflict"));
        }
        let pending: i64 = tx.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state IN ('prepared','pending_recovery') AND task_id=?1", [&request.task_id], |r| r.get(0))?;
        if pending >= MAX_PENDING_ACTIONS { return Err(RuntimeError::Code("pending_limit")); }
        let action = request.action_id.to_string();
        let initial_state = if matches!(request.policy_decision, PolicyDecision::ApprovalRequired) { "awaiting_approval" } else { "prepared" };
        tx.execute("INSERT INTO receipt_actions(schema_version,action_id,task_id,run_id,tool_name,normalized_scope,fingerprint_input_version,tool_args_hash,policy_id,policy_decision,state,dispatch_state,approval_id,approval_call_hash) VALUES(1,?1,?2,?3,?4,?5,1,?6,?7,?8,?9,'not_started',?10,?6)", params![action, request.task_id, request.run_id, request.tool_name, request.normalized_scope, args_hash, request.policy_id, request.policy_decision.as_str(), initial_state, request.approval_id.map(|v|v.to_string())])?;
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
        if !matches!(status, "succeeded"|"failed"|"cancelled") { return Err(RuntimeError::Code("schema_violation")); }
        let args_hash = canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input)?;
        if output_digest.len() != 64 || !output_digest.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { return Err(RuntimeError::Code("schema_violation")); }
        let projection = if status == "succeeded" { json!({"status":"succeeded","output_digest":output_digest}) } else { json!({"status":status,"error_category":error_category.ok_or(RuntimeError::Code("schema_violation"))?}) };
        let result = result_hash(&projection)?;
        let marker = bounded_result_marker(status, &result, error_category, now_ms(), status == "succeeded")?;
        let tx = self.connection.unchecked_transaction()?;
        let (state, dispatch, stored_hash): (String,String,String) = tx.query_row("SELECT state,dispatch_state,tool_args_hash FROM receipt_actions WHERE action_id=?1", [request.action_id.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if (state != "prepared" && state != "pending_recovery") || (dispatch != "started" && dispatch != "returned") || stored_hash != args_hash { return Err(RuntimeError::Code("schema_violation")); }
        let (hash, _) = signed_receipt(&tx, self.signer, request, "post_action", status, &stored_hash, Some(&result), None)?;
        tx.execute("UPDATE receipt_actions SET state=?2,dispatch_state='returned',result_hash=?3,result_marker=?4,terminal_receipt_hash=?5 WHERE action_id=?1", params![request.action_id.to_string(), status, result, marker, hash])?;
        tx.commit()?;
        Ok(hash)
    }

    pub fn refuse(&self, request: &ActionRequest, code: &str) -> Result<String, RuntimeError> {
        if !matches!(code, "policy_denied"|"approval_denied"|"approval_expired"|"approval_stale"|"call_changed"|"key_untrusted"|"recovery_pending") { return Err(RuntimeError::Code("schema_violation")); }
        let args_hash = canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input)?;
        let tx = self.connection.unchecked_transaction()?;
        let (hash, _) = signed_receipt(&tx, self.signer, request, "refusal", "refused", &args_hash, None, Some(code))?;
        tx.execute("UPDATE receipt_actions SET state='refused',terminal_receipt_hash=?2 WHERE action_id=?1", params![request.action_id.to_string(), hash])?;
        let approval_state = match code { "approval_expired" => "expired", "approval_denied" => "denied", _ => "lost" };
        tx.execute("UPDATE receipt_approval_intents SET state=?2 WHERE action_id=?1 AND state IN ('pending','granted')", params![request.action_id.to_string(), approval_state])?;
        tx.commit()?;
        Ok(hash)
    }

    pub fn mark_pending_recovery(&self, action_id: Uuid, code: &str) -> Result<(), RuntimeError> {
        if !valid_recovery_code(code) { return Err(RuntimeError::Code("schema_violation")); }
        self.connection.execute("UPDATE receipt_actions SET state='pending_recovery',recovery_code=?2 WHERE action_id=?1 AND dispatch_state IN ('started','returned')", params![action_id.to_string(), code])?;
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
        let cutoff = now_ms_value.saturating_sub(APPROVAL_TTL_MS);
        let deleted = self.connection.execute(
            "DELETE FROM receipt_approval_intents WHERE state IN ('expired','lost','claimed') AND expires_at_ms<=?1 AND NOT EXISTS (SELECT 1 FROM receipt_actions a WHERE a.action_id=receipt_approval_intents.action_id AND a.state='pending_recovery')",
            [cutoff],
        )?;
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

    pub fn load_protected_action(&self, action_id: Uuid, key: &[u8; 32]) -> Result<ProtectedActionRow, RuntimeError> {
        let envelope: Vec<u8> = self.connection.query_row("SELECT envelope FROM receipt_protected_actions WHERE action_id=?1", [action_id.to_string()], |row| row.get(0))?;
        unprotect_action_row(&envelope, key)
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
        Ok(())
    }

    /// Manual operator closure is deliberately incapable of returning an
    /// action to a dispatchable state.
    pub fn unquarantine(&self, action_id: Uuid, authenticated_operator: bool, checkpoint: &str) -> Result<(), RuntimeError> {
        if !authenticated_operator || checkpoint.is_empty() || checkpoint.len() > 256 { return Err(RuntimeError::Code("key_untrusted")); }
        let changed = self.connection.execute("UPDATE receipt_actions SET state='refused',recovery_code='unknown' WHERE action_id=?1 AND state='quarantined'", [action_id.to_string()])?;
        if changed != 1 { return Err(RuntimeError::Code("schema_violation")); }
        Ok(())
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

    pub fn counts(&self) -> Result<RuntimeCounts, RuntimeError> {
        Ok(RuntimeCounts {
            pending: self.connection.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state IN ('awaiting_approval','prepared','pending_recovery')", [], |r| r.get(0))?,
            pending_recovery: self.connection.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state='pending_recovery'", [], |r| r.get(0))?,
            quarantined: self.connection.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state='quarantined'", [], |r| r.get(0))?,
            approval_pending: self.connection.query_row("SELECT COUNT(*) FROM receipt_approval_intents WHERE state='pending'", [], |r| r.get(0))?,
        })
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
        ActionRequest { action_id: Uuid::now_v7(), task_id:"task-1".into(), run_id:"run-1".into(), tool_name:"filesystem.write".into(), policy_id:"policy-v1".into(), normalized_scope:"workspace".into(), input:json!({"path":"a.txt","content":"x"}), policy_decision:policy, approval_id:None, preview:"write a file".into() }
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
    fn sampling_is_deterministic_and_zero_does_not_sample() {
        assert_eq!(sampled_read_only("018f0f2a-2222-7222-8222-222222222222", "filesystem.read", 10), sampled_read_only("018f0f2a-2222-7222-8222-222222222222", "filesystem.read", 10));
        assert!(!sampled_read_only("action", "filesystem.read", 0));
        assert!(sampled_read_only("action", "filesystem.read", 100));
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
}
