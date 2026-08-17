//! Core-owned execution journal for Signed Receipt v1.
//!
//! This module deliberately keeps tool execution outside SQLite transactions:
//! callers prepare/claim a mutation, dispatch it, and then commit exactly one
//! terminal receipt.  The durable rows are the recovery source of truth.

use crate::{canonicalize_json, payload_bytes, receipt_hash, result_hash, Envelope, ReceiptError};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub const APPROVAL_TTL_MS: i64 = 600_000;
pub const MAX_PENDING_ACTIONS: i64 = 1024;
pub const MAX_PREVIEW_BYTES: usize = 1024;

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

/// Signing boundary.  The signer receives the SHA-256 digest of canonical
/// payload bytes, never raw tool input or a mutable JSON representation.
pub trait ReceiptSigner: Send + Sync {
    fn key_id(&self) -> Result<String, RuntimeError>;
    fn sign_payload_hash(&self, payload_hash: &str) -> Result<String, RuntimeError>;
}

pub fn install_schema(connection: &Connection) -> Result<(), RuntimeError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS receipt_records (
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
           action_id TEXT PRIMARY KEY NOT NULL,
           task_id TEXT NOT NULL,
           run_id TEXT NOT NULL,
           tool_name TEXT NOT NULL,
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
           tool_started_at_ms INTEGER,
           UNIQUE(action_id)
         );
         CREATE TABLE IF NOT EXISTS receipt_approval_intents (
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
         INSERT OR IGNORE INTO receipt_runtime_guard(id,phase,generation,updated_at_ms)
           VALUES(1,'ready',0,0);",
    )?;
    Ok(())
}

pub fn canonical_call_hash(tool_name: &str, normalized_scope: &str, input: &Value) -> Result<String, RuntimeError> {
    if tool_name.contains('\n') || normalized_scope.contains('\n') {
        return Err(RuntimeError::Code("schema_violation"));
    }
    let input = payload_bytes(input)?;
    let mut bytes = Vec::with_capacity(tool_name.len() + normalized_scope.len() + input.len() + 2);
    bytes.extend_from_slice(tool_name.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(normalized_scope.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(&input);
    Ok(hex::encode(Sha256::digest(bytes)))
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
    let previous: Option<String> = tx.query_row("SELECT receipt_hash FROM receipt_chain_heads WHERE key_id=?1", [signer.key_id()?], |r| r.get(0)).optional()?;
    let payload = build_payload(request, kind, status, args_hash, previous.as_deref(), result, refusal);
    crate::validate_payload_v1(&payload)?;
    let payload_bytes = crate::payload_bytes(&payload)?;
    let payload_hash = crate::sha256_hex(&payload_bytes);
    let key_id = signer.key_id()?;
    let signature = signer.sign_payload_hash(&payload_hash)?;
    let envelope = Envelope { payload: payload.clone(), key_id: key_id.clone(), signature_algorithm: "Ed25519".into(), signature };
    let envelope_bytes = canonicalize_json(&serde_json::to_vec(&envelope).map_err(|_| ReceiptError::InvalidJson)?)?;
    let hash = receipt_hash(&envelope)?;
    let object = payload.as_object().unwrap();
    tx.execute("INSERT INTO receipt_records(receipt_id,action_id,receipt_kind,action_status,task_id,run_id,key_id,canonical_payload,canonical_envelope,receipt_hash,previous_receipt_hash,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![object["receipt_id"].as_str(), request.action_id.to_string(), kind, status, request.task_id, request.run_id, key_id, payload_bytes, envelope_bytes, hash, previous, now_ms()])?;
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

    pub fn prepare(&self, mut request: ActionRequest) -> Result<PrepareOutcome, RuntimeError> {
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
        tx.execute("INSERT INTO receipt_actions(action_id,task_id,run_id,tool_name,tool_args_hash,policy_id,policy_decision,state,dispatch_state,approval_id,approval_call_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'not_started',?9,?5)", params![action, request.task_id, request.run_id, request.tool_name, args_hash, request.policy_id, request.policy_decision.as_str(), initial_state, request.approval_id.map(|v|v.to_string())])?;
        match request.policy_decision {
            PolicyDecision::Deny => {
                let (hash, _) = signed_receipt(&tx, self.signer, &request, "refusal", "refused", &args_hash, None, Some("policy_denied"))?;
                tx.execute("UPDATE receipt_actions SET state='refused',terminal_receipt_hash=?2 WHERE action_id=?1", params![action, hash])?;
                tx.commit()?;
                Ok(PrepareOutcome::Refused { action_id: request.action_id, receipt_hash: hash, code: "policy_denied".into() })
            }
            PolicyDecision::ApprovalRequired => {
                if request.approval_id.is_some() {
                    return Err(RuntimeError::Code("approval_stale"));
                }
                let approval_id = Uuid::now_v7();
                let created = now_ms();
                let expires = created + APPROVAL_TTL_MS;
                tx.execute("INSERT INTO receipt_approval_intents(approval_id,action_id,task_id,run_id,tool_name,normalized_scope,call_hash,preview,state,created_wall_at_ms,expires_at_ms,created_monotonic_ms,deadline_monotonic_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'pending',?9,?10,?9,?10)", params![approval_id.to_string(), action, request.task_id, request.run_id, request.tool_name, request.normalized_scope, args_hash, request.preview, created, expires])?;
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

    /// Records the UI decision without holding the IPC request open.  A
    /// decision is one-way and does not itself authorize dispatch.
    pub fn grant_approval(&self, approval_id: Uuid) -> Result<(), RuntimeError> {
        let changed = self.connection.execute(
            "UPDATE receipt_approval_intents SET state='granted' WHERE approval_id=?1 AND state='pending' AND expires_at_ms>?2",
            params![approval_id.to_string(), now_ms()],
        )?;
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
        let projection = if status == "succeeded" { json!({"status":"succeeded","output_digest":output_digest}) } else { json!({"status":status,"error_category":error_category.ok_or(RuntimeError::Code("schema_violation"))?}) };
        let result = result_hash(&projection)?;
        let tx = self.connection.unchecked_transaction()?;
        let (state, stored_hash): (String,String) = tx.query_row("SELECT state,tool_args_hash FROM receipt_actions WHERE action_id=?1", [request.action_id.to_string()], |r| Ok((r.get(0)?,r.get(1)?)))?;
        if state != "prepared" && state != "pending_recovery" || stored_hash != args_hash { return Err(RuntimeError::Code("schema_violation")); }
        let (hash, _) = signed_receipt(&tx, self.signer, request, "post_action", status, &stored_hash, Some(&result), None)?;
        tx.execute("UPDATE receipt_actions SET state=?2,dispatch_state='returned',result_hash=?3,terminal_receipt_hash=?4 WHERE action_id=?1", params![request.action_id.to_string(), status, result, hash])?;
        tx.commit()?;
        Ok(hash)
    }

    pub fn refuse(&self, request: &ActionRequest, code: &str) -> Result<String, RuntimeError> {
        if !matches!(code, "policy_denied"|"approval_denied"|"approval_expired"|"approval_stale"|"call_changed"|"key_untrusted"|"recovery_pending") { return Err(RuntimeError::Code("schema_violation")); }
        let args_hash = canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input)?;
        let tx = self.connection.unchecked_transaction()?;
        let (hash, _) = signed_receipt(&tx, self.signer, request, "refusal", "refused", &args_hash, None, Some(code))?;
        tx.execute("UPDATE receipt_actions SET state='refused',terminal_receipt_hash=?2 WHERE action_id=?1", params![request.action_id.to_string(), hash])?;
        tx.commit()?;
        Ok(hash)
    }

    pub fn mark_pending_recovery(&self, action_id: Uuid, code: &str) -> Result<(), RuntimeError> {
        self.connection.execute("UPDATE receipt_actions SET state='pending_recovery',recovery_code=?2 WHERE action_id=?1 AND dispatch_state='started'", params![action_id.to_string(), code])?;
        Ok(())
    }

    pub fn action(&self, action_id: Uuid) -> Result<Option<ActionState>, RuntimeError> {
        Ok(self.connection.query_row("SELECT action_id,state,dispatch_state,pre_receipt_hash,terminal_receipt_hash,tool_args_hash FROM receipt_actions WHERE action_id=?1", [action_id.to_string()], |r| Ok(ActionState { action_id:r.get(0)?,state:r.get(1)?,dispatch_state:r.get(2)?,pre_receipt_hash:r.get(3)?,terminal_receipt_hash:r.get(4)?,tool_args_hash:r.get(5)? })).optional()?)
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
}
