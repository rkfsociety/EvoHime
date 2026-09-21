//! Core-owned execution journal for Signed Receipt v1.
//!
//! This module deliberately keeps tool execution outside SQLite transactions:
//! callers prepare/claim a mutation, dispatch it, and then commit exactly one
//! terminal receipt.  The durable rows are the recovery source of truth.

use crate::runtime_payload::{bounded_preview, parent_approval_ids, valid_parent_approval_ref};
use crate::runtime_platform::{boot_id, monotonic_ms};
use crate::runtime_signing::{signed_receipt, SignedReceiptInput};
use crate::runtime_transaction::RetryTransaction;
use crate::{canonicalize_json, receipt_hash, result_hash, Envelope, ReceiptError};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::json;
use std::collections::BTreeMap;

/// Строка привязки действия из `receipt_actions` в порядке SELECT:
/// task_id, run_id, tool_name, normalized_scope, policy_id, policy_decision,
/// fingerprint_input_version, approval_id, parent_approval_ref, state,
/// terminal_receipt_hash.
type ActionBindingRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
);

/// Та же привязка, снятая при завершении действия: перед идентификацией идут
/// state, dispatch_state и tool_args_hash.
type ActionCompletionRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
);
use std::time::Duration;
use uuid::Uuid;

pub use crate::runtime_contract::*;
pub use crate::runtime_policy::{
    bind_capability_to_action, canonical_call_hash, persist_capability_snapshot,
    persist_policy_decision,
};
pub(crate) use crate::runtime_projection::valid_recovery_code;
pub use crate::runtime_projection::{
    bounded_result_marker, protect_action_row, sampled_read_only, unprotect_action_row,
};
pub use crate::runtime_recovery::recover_database;
pub use crate::runtime_request_contract::ModelRequestReceiptInput;
pub use crate::runtime_schema::install_schema;

pub const APPROVAL_TTL_MS: i64 = 600_000;
pub const MAX_PENDING_ACTIONS: i64 = 1024;
pub const MAX_PREVIEW_BYTES: usize = crate::CONTRACT_MAX_PREVIEW_BYTES;
pub const MAX_CALL_INPUT_BYTES: usize = 262_144;
pub const MAX_PROTECTED_ROW_BYTES: usize = 512;
const BOUNDED_METRICS: &[&str] = &[
    "receipt_pre_latency_ms",
    "receipt_post_latency_ms",
    "receipt_append_latency_ms",
    "receipt_append_busy_retries",
    "receipt_chain_conflicts",
    "receipt_schema_violations",
    "approval_pending_count",
    "pending_recovery_count",
    "quarantined_count",
    "approval_gc_deleted_count",
    "recovery_duration_ms",
    "recovery_safe_mode",
    "read_only_sampled_count",
    "read_only_unsampled_count",
    "receipt_append_count",
];
/// Runs before Core accepts any new mutation. This API intentionally does not
/// require a signer: recovery may inspect and expire state even when signing
/// is unavailable, while all writes to the chain remain blocked by the guard.
pub(crate) fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

pub(crate) fn increment_metric_tx(
    connection: &Connection,
    metric: &str,
    amount: i64,
) -> Result<(), RuntimeError> {
    if metric.is_empty()
        || metric.len() > 64
        || !metric
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'.')
    {
        return Err(RuntimeError::Code("schema_violation"));
    }
    connection.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES(?1,?2) ON CONFLICT(metric) DO UPDATE SET value=value+excluded.value", params![metric, amount])?;
    Ok(())
}

/// Every mutation/chain-write entry point (claim, complete, refuse,
/// dispatch-state transitions, GC, unquarantine) must gate on this before
/// touching `receipt_actions`/`receipt_records`. `read_only_recovery` only
/// permits status/diagnostics/export/backup/staging-restore; it never lets an
/// already-prepared action keep progressing through the chain.
fn require_ready(connection: &Connection) -> Result<(), RuntimeError> {
    let phase: String = connection.query_row(
        "SELECT phase FROM receipt_runtime_guard WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    if phase != "ready" {
        return Err(RuntimeError::Code("pending_recovery"));
    }
    Ok(())
}

fn stored_hash_for_action(
    connection: &Connection,
    action_id: &str,
) -> Result<String, RuntimeError> {
    connection
        .query_row(
            "SELECT tool_args_hash FROM receipt_actions WHERE action_id=?1",
            [action_id],
            |row| row.get(0),
        )
        .map_err(RuntimeError::from)
}

pub struct ReceiptRuntime<'a> {
    connection: &'a mut Connection,
    signer: &'a dyn ReceiptSigner,
}

impl<'a> ReceiptRuntime<'a> {
    pub fn new(
        connection: &'a mut Connection,
        signer: &'a dyn ReceiptSigner,
    ) -> Result<Self, RuntimeError> {
        install_schema(connection)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        Ok(Self { connection, signer })
    }

    /// Appends a request-commit receipt to the existing signed chain. Only
    /// identifiers and the immutable envelope hash are signed.
    pub fn append_model_request_receipt(
        &mut self,
        input: ModelRequestReceiptInput<'_>,
    ) -> Result<SignedModelRequestReceipt, RuntimeError> {
        let ModelRequestReceiptInput {
            request_id,
            logical_request_id,
            ledger_id,
            attempt,
            provider,
            model,
            envelope_hash,
            context_projection_hash,
            route_snapshot_hash,
            policy_snapshot_hash,
        } = input;
        // Some providers (notably LiteRouter) choose the concrete model at
        // dispatch time. The model-request contract therefore permits an
        // empty model identifier; the provider identity and all commitments
        // remain mandatory.
        if request_id.is_empty()
            || logical_request_id.is_empty()
            || ledger_id.is_empty()
            || attempt == 0
            || provider.is_empty()
            || [
                envelope_hash,
                context_projection_hash,
                route_snapshot_hash,
                policy_snapshot_hash,
            ]
            .iter()
            .any(|hash| {
                hash.len() != 64
                    || !hash
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
        {
            return Err(RuntimeError::Code("schema_violation"));
        }
        require_ready(self.connection)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = tx
            .query_row(
                "SELECT receipt_id,receipt_hash,canonical_payload,previous_receipt_hash,key_id,created_at_ms FROM receipt_records WHERE request_id=?1 AND receipt_kind='request_commit'",
                [request_id],
                |row| {
                    Ok(SignedModelRequestReceipt {
                        receipt_id: row.get(0)?,
                        request_id: request_id.to_string(),
                        receipt_hash: row.get(1)?,
                        canonical_payload: row.get(2)?,
                        previous_receipt_hash: row.get(3)?,
                        key_id: row.get(4)?,
                        created_at_ms: row.get(5)?,
                    })
                },
            )
            .optional()?
        {
            tx.commit()?;
            return Ok(existing);
        }
        let key_id = self.signer.key_id()?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT receipt_hash FROM receipt_chain_heads WHERE key_id=?1",
                [&key_id],
                |row| row.get(0),
            )
            .optional()?;
        let receipt_id = Uuid::now_v7().to_string();
        let created_at_ms = now_ms();
        let payload = serde_json::json!({
            "receipt_version": 1,
            "payload_version": 1,
            "receipt_domain": "model_request",
            "receipt_type": "request_commit",
            "receipt_id": receipt_id,
            "request_id": request_id,
            "logical_request_id": logical_request_id,
            "ledger_id": ledger_id,
            "attempt": attempt,
            "provider": provider,
            "model": model,
            "request_envelope_hash": envelope_hash,
            "context_projection_hash": context_projection_hash,
            "route_snapshot_hash": route_snapshot_hash,
            "policy_snapshot_hash": policy_snapshot_hash,
            "previous_receipt_hash": previous,
            "created_at_ms": created_at_ms,
        });
        let canonical_payload = canonicalize_json(
            &serde_json::to_vec(&payload).map_err(|_| ReceiptError::InvalidJson)?,
        )?;
        let payload_hash = crate::sha256_hex(&canonical_payload);
        let envelope = Envelope {
            payload,
            key_id: key_id.clone(),
            signature_algorithm: "Ed25519".into(),
            signature: self.signer.sign_payload_hash(&payload_hash)?,
        };
        let canonical_envelope = canonicalize_json(
            &serde_json::to_vec(&envelope).map_err(|_| ReceiptError::InvalidJson)?,
        )?;
        let receipt_hash = receipt_hash(&envelope)?;
        tx.execute(
            "INSERT INTO receipt_records(schema_version,receipt_id,action_id,request_id,receipt_kind,action_status,task_id,run_id,key_id,canonical_payload,canonical_envelope,receipt_hash,previous_receipt_hash,created_at_ms,source) VALUES(1,?1,NULL,?2,'request_commit','committed',?3,?4,?5,?6,?7,?8,?9,?10,'signed')",
            params![
                receipt_id,
                request_id,
                logical_request_id,
                ledger_id,
                key_id,
                canonical_payload,
                canonical_envelope,
                receipt_hash,
                previous,
                created_at_ms
            ],
        )?;
        tx.execute(
            "INSERT INTO receipt_chain_heads(key_id,receipt_hash,updated_at_ms) VALUES(?1,?2,?3) ON CONFLICT(key_id) DO UPDATE SET receipt_hash=excluded.receipt_hash,updated_at_ms=excluded.updated_at_ms",
            params![key_id, receipt_hash, created_at_ms],
        )?;
        increment_metric_tx(&tx, "receipt_append_count", 1)?;
        tx.commit()?;
        Ok(SignedModelRequestReceipt {
            receipt_id,
            request_id: request_id.to_string(),
            receipt_hash,
            canonical_payload,
            previous_receipt_hash: previous,
            key_id,
            created_at_ms,
        })
    }

    pub fn prepare(&mut self, request: ActionRequest) -> Result<PrepareOutcome, RuntimeError> {
        self.prepare_inner(request, false)
    }

    /// Imports an already Core-created approval id from the PermissionEngine.
    /// This is only for the compatibility approval producer; the renderer
    /// still cannot choose an id.
    pub fn prepare_existing_approval(
        &mut self,
        request: ActionRequest,
    ) -> Result<PrepareOutcome, RuntimeError> {
        self.prepare_inner(request, true)
    }

    /// Imports a legacy in-memory approval as a new pending Core approval.
    /// The legacy identifier is audit-only and cannot authorize the new claim.
    pub fn import_legacy_approval(
        &mut self,
        legacy_ref: &str,
        request: ActionRequest,
    ) -> Result<PrepareOutcome, RuntimeError> {
        if legacy_ref.is_empty() || legacy_ref.len() > 128 {
            return Err(RuntimeError::Code("schema_violation"));
        }
        if !matches!(request.policy_decision, PolicyDecision::ApprovalRequired)
            || request.approval_id.is_some()
        {
            return Err(RuntimeError::Code("approval_stale"));
        }
        let outcome = self.prepare(request)?;
        let PrepareOutcome::ApprovalRequired {
            action_id,
            approval_id,
            expires_at_ms,
        } = outcome
        else {
            return Err(RuntimeError::Code("schema_violation"));
        };
        self.connection.execute(
            "UPDATE receipt_actions SET legacy_approval_ref=?2 WHERE action_id=?1",
            params![action_id.to_string(), legacy_ref],
        )?;
        self.connection.execute(
            "UPDATE receipt_approval_intents SET legacy_approval_ref=?2 WHERE approval_id=?1",
            params![approval_id.to_string(), legacy_ref],
        )?;
        Ok(PrepareOutcome::ApprovalRequired {
            action_id,
            approval_id,
            expires_at_ms,
        })
    }

    /// Idempotent batch migration of legacy pending approval records. Each
    /// entry is keyed by `migration_version + legacy_approval_ref`: a record
    /// already imported under that key is skipped rather than reimported,
    /// and none of them auto-grants or auto-dispatches. Incomplete records
    /// (missing task/session/tool/scope/hash) must be filtered by the caller
    /// before calling this — they are marked `lost` and never imported.
    pub fn migrate_legacy_approvals(
        &mut self,
        migration_version: u8,
        legacy_records: Vec<(String, ActionRequest)>,
    ) -> Result<Vec<PrepareOutcome>, RuntimeError> {
        let mut outcomes = Vec::with_capacity(legacy_records.len());
        for (legacy_ref, request) in legacy_records {
            if legacy_ref.is_empty() || legacy_ref.len() > 128 {
                continue;
            }
            let key = format!("{migration_version}:{legacy_ref}");
            let already: Option<String> = self
                .connection
                .query_row(
                    "SELECT legacy_approval_ref FROM receipt_actions WHERE legacy_approval_ref=?1",
                    [&key],
                    |row| row.get(0),
                )
                .optional()?;
            if already.is_some() {
                continue;
            }
            match self.import_legacy_approval(&key, request) {
                Ok(outcome) => outcomes.push(outcome),
                Err(RuntimeError::Code("approval_stale")) => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(outcomes)
    }

    fn prepare_inner(
        &mut self,
        mut request: ActionRequest,
        existing_approval: bool,
    ) -> Result<PrepareOutcome, RuntimeError> {
        require_ready(self.connection)?;
        if request.action_id.get_version_num() != 7 {
            return Err(RuntimeError::Code("schema_violation"));
        }
        request.preview = bounded_preview(&request.preview);
        if request
            .parent_approval_ref
            .as_deref()
            .is_some_and(|value| !valid_parent_approval_ref(value))
        {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let args_hash = canonical_call_hash(
            &request.tool_name,
            &request.normalized_scope,
            &request.input,
        )?;
        let tx = RetryTransaction::begin(self.connection)?;
        if let Some(parent) = request.parent_approval_ref.as_deref() {
            let Some((parent_action_id, parent_approval_id)) = parent_approval_ids(parent) else {
                return Err(RuntimeError::Code("schema_violation"));
            };
            let parent_state: Option<(String, String)> = tx
                .query_row(
                    "SELECT action_id,state FROM receipt_approval_intents WHERE approval_id=?1",
                    [parent_approval_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if !parent_state.is_some_and(|(action_id, state)| {
                action_id == parent_action_id && matches!(state.as_str(), "granted" | "claimed")
            }) {
                return Err(RuntimeError::Code("approval_stale"));
            }
        }
        if tx
            .query_row(
                "SELECT 1 FROM receipt_actions WHERE action_id=?1",
                [request.action_id.to_string()],
                |_| Ok(1),
            )
            .optional()?
            .is_some()
        {
            return Err(RuntimeError::Code("action_id_conflict"));
        }
        let pending: i64 = tx.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state IN ('prepared','pending_recovery') AND task_id=?1", [&request.task_id], |r| r.get(0))?;
        if pending >= MAX_PENDING_ACTIONS {
            return Err(RuntimeError::Code("pending_limit"));
        }
        let action = request.action_id.to_string();
        let initial_state = if matches!(request.policy_decision, PolicyDecision::ApprovalRequired) {
            "awaiting_approval"
        } else {
            "prepared"
        };
        tx.execute("INSERT INTO receipt_actions(schema_version,action_id,task_id,run_id,tool_name,normalized_scope,fingerprint_input_version,tool_args_hash,policy_id,policy_decision,state,dispatch_state,approval_id,approval_call_hash,parent_approval_ref) VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'not_started',?11,?7,?12)", params![action, request.task_id, request.run_id, request.tool_name, request.normalized_scope, crate::FINGERPRINT_INPUT_VERSION, args_hash, request.policy_id, request.policy_decision.as_str(), initial_state, request.approval_id.map(|v|v.to_string()), request.parent_approval_ref])?;
        match request.policy_decision {
            PolicyDecision::Deny => {
                let (hash, _) = signed_receipt(
                    &tx,
                    self.signer,
                    SignedReceiptInput {
                        request: &request,
                        kind: "refusal",
                        status: "refused",
                        args_hash: &args_hash,
                        result: None,
                        refusal: Some("policy_denied"),
                    },
                )?;
                tx.execute("UPDATE receipt_actions SET state='refused',terminal_receipt_hash=?2 WHERE action_id=?1", params![action, hash])?;
                tx.commit()?;
                Ok(PrepareOutcome::Refused {
                    action_id: request.action_id,
                    receipt_hash: hash,
                    code: "policy_denied".into(),
                })
            }
            PolicyDecision::ApprovalRequired => {
                if request.approval_id.is_some() && !existing_approval {
                    return Err(RuntimeError::Code("approval_stale"));
                }
                let approval_id = request.approval_id.unwrap_or_else(Uuid::now_v7);
                if approval_id.get_version_num() != 7 {
                    return Err(RuntimeError::Code("schema_violation"));
                }
                let created = now_ms();
                let created_monotonic = monotonic_ms()?;
                let expires = created + APPROVAL_TTL_MS;
                let deadline = created_monotonic + APPROVAL_TTL_MS;
                tx.execute("INSERT INTO receipt_approval_intents(approval_id,action_id,task_id,run_id,tool_name,normalized_scope,call_hash,preview,state,created_wall_at_ms,expires_at_ms,clock_boot_id,created_monotonic_ms,deadline_monotonic_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'pending',?9,?10,?11,?12,?13)", params![approval_id.to_string(), action, request.task_id, request.run_id, request.tool_name, request.normalized_scope, args_hash, request.preview, created, expires, boot_id()?, created_monotonic, deadline])?;
                increment_metric_tx(&tx, "approval_pending_count", 1)?;
                tx.commit()?;
                Ok(PrepareOutcome::ApprovalRequired {
                    action_id: request.action_id,
                    approval_id,
                    expires_at_ms: expires,
                })
            }
            PolicyDecision::Allow => {
                let (hash, _) = signed_receipt(
                    &tx,
                    self.signer,
                    SignedReceiptInput {
                        request: &request,
                        kind: "pre_action",
                        status: "prepared",
                        args_hash: &args_hash,
                        result: None,
                        refusal: None,
                    },
                )?;
                tx.execute("UPDATE receipt_actions SET pre_receipt_hash=?2,state='prepared' WHERE action_id=?1", params![action, hash])?;
                tx.commit()?;
                Ok(PrepareOutcome::Prepared {
                    action_id: request.action_id,
                    receipt_hash: hash,
                })
            }
        }
    }

    pub fn mark_started(&self, action_id: Uuid) -> Result<(), RuntimeError> {
        require_ready(self.connection)?;
        let changed = self.connection.execute("UPDATE receipt_actions SET dispatch_state='started',tool_started_at_ms=?2 WHERE action_id=?1 AND state='prepared' AND dispatch_state='not_started'", params![action_id.to_string(), now_ms()])?;
        if changed != 1 {
            return Err(RuntimeError::Code("action_id_conflict"));
        }
        Ok(())
    }

    pub fn mark_returned(&self, action_id: Uuid) -> Result<(), RuntimeError> {
        require_ready(self.connection)?;
        let changed = self.connection.execute(
            "UPDATE receipt_actions SET dispatch_state='returned' WHERE action_id=?1 AND state='prepared' AND dispatch_state='started'",
            [action_id.to_string()],
        )?;
        if changed != 1 {
            return Err(RuntimeError::Code("action_id_conflict"));
        }
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
        let state: Option<String> = self
            .connection
            .query_row(
                "SELECT state FROM receipt_approval_intents WHERE approval_id=?1",
                [approval_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        if state.as_deref() == Some("denied") {
            return Err(RuntimeError::Code("approval_denied"));
        }
        let deadline: Option<(String, i64, i64)> = self.connection.query_row(
            "SELECT clock_boot_id,expires_at_ms,deadline_monotonic_ms FROM receipt_approval_intents WHERE approval_id=?1 AND state='pending'",
            [approval_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?;
        let current_boot = boot_id()?;
        let valid = deadline.is_some_and(|(boot, wall, mono)| {
            if boot == current_boot {
                monotonic_ms().is_ok_and(|now| now < mono)
            } else {
                now_ms() < wall
            }
        });
        let changed = if valid {
            self.connection.execute("UPDATE receipt_approval_intents SET state='granted' WHERE approval_id=?1 AND state='pending'", [approval_id.to_string()])?
        } else {
            self.connection.execute("UPDATE receipt_approval_intents SET state='expired' WHERE approval_id=?1 AND state='pending'", [approval_id.to_string()])?;
            0
        };
        if changed != 1 {
            return Err(RuntimeError::Code("approval_expired"));
        }
        Ok(())
    }

    /// Records an explicit user rejection durably. A rejected intent can
    /// never be promoted to `granted` by a replayed execute request.
    pub fn deny_approval(&self, approval_id: Uuid) -> Result<(), RuntimeError> {
        require_ready(self.connection)?;
        let tx = self.connection.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE receipt_approval_intents SET state='denied' WHERE approval_id=?1 AND state='pending'",
            [approval_id.to_string()],
        )?;
        if changed != 1 {
            return Err(RuntimeError::Code("approval_stale"));
        }
        tx.execute(
            "UPDATE receipt_actions SET state='refused',completion_source='execution' WHERE action_id=(SELECT action_id FROM receipt_approval_intents WHERE approval_id=?1) AND state='awaiting_approval'",
            [approval_id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Claims a granted intent and appends the pre receipt atomically.  The
    /// caller must provide the same call fields that produced the intent.
    #[cfg(test)]
    fn claim_approval(
        &mut self,
        request: &ActionRequest,
        approval_id: Uuid,
    ) -> Result<PrepareOutcome, RuntimeError> {
        self.claim_approval_checked(request, approval_id, |_| true)
    }

    /// Same as [`Self::claim_approval`] but re-applies the caller's current
    /// policy decision as part of the atomic claim gate. `recheck_policy`
    /// receives the request and must return `true` only if the exact same
    /// call is still `allow`/`approval_required` under the *current* policy
    /// snapshot — a stale approval never bypasses a policy that changed
    /// after Prepare.
    pub fn claim_approval_checked(
        &mut self,
        request: &ActionRequest,
        approval_id: Uuid,
        recheck_policy: impl FnOnce(&ActionRequest) -> bool,
    ) -> Result<PrepareOutcome, RuntimeError> {
        require_ready(self.connection)?;
        let args_hash = canonical_call_hash(
            &request.tool_name,
            &request.normalized_scope,
            &request.input,
        )?;
        let tx = RetryTransaction::begin(self.connection)?;
        let row: Option<(String, String, String, i64)> = tx.query_row(
            "SELECT action_id,state,clock_boot_id,deadline_monotonic_ms FROM receipt_approval_intents WHERE approval_id=?1",
            [approval_id.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).optional()?;
        let Some((action_id, state, boot, deadline_monotonic_ms)) = row else {
            return Err(RuntimeError::Code("approval_stale"));
        };
        // Normative rule: within one boot, authorization claim uses only the
        // monotonic deadline. Wall clock is a fail-closed recovery boundary
        // only, never an authorization check in the same boot.
        let expired = if boot == boot_id()? {
            monotonic_ms()? >= deadline_monotonic_ms
        } else {
            true
        };
        if state != "granted" || expired {
            let code = if expired {
                "approval_expired"
            } else {
                "approval_stale"
            };
            let (hash, _) = signed_receipt(
                &tx,
                self.signer,
                SignedReceiptInput {
                    request,
                    kind: "refusal",
                    status: "refused",
                    args_hash: &stored_hash_for_action(&tx, &action_id)?,
                    result: None,
                    refusal: Some(code),
                },
            )?;
            tx.execute("UPDATE receipt_approval_intents SET state=?2 WHERE approval_id=?1 AND state IN ('pending','granted')", params![approval_id.to_string(), if expired { "expired" } else { "lost" }])?;
            tx.execute("UPDATE receipt_actions SET state='refused',terminal_receipt_hash=?2,completion_source='execution' WHERE action_id=?1 AND terminal_receipt_hash IS NULL", params![action_id, hash])?;
            tx.commit()?;
            return Err(RuntimeError::Code(code));
        }
        let stored: String = tx.query_row(
            "SELECT tool_args_hash FROM receipt_actions WHERE action_id=?1",
            [&action_id],
            |r| r.get(0),
        )?;
        if stored != args_hash || request.action_id.to_string() != action_id {
            let (hash, _) = signed_receipt(
                &tx,
                self.signer,
                SignedReceiptInput {
                    request,
                    kind: "refusal",
                    status: "refused",
                    args_hash: &stored,
                    result: None,
                    refusal: Some("call_changed"),
                },
            )?;
            tx.execute("UPDATE receipt_approval_intents SET state='lost' WHERE approval_id=?1 AND state IN ('pending','granted')", [approval_id.to_string()])?;
            tx.execute("UPDATE receipt_actions SET state='refused',terminal_receipt_hash=?2 WHERE action_id=?1 AND terminal_receipt_hash IS NULL", params![action_id, hash])?;
            tx.commit()?;
            return Err(RuntimeError::Code("call_changed"));
        }
        if !recheck_policy(request) {
            let (hash, _) = signed_receipt(
                &tx,
                self.signer,
                SignedReceiptInput {
                    request,
                    kind: "refusal",
                    status: "refused",
                    args_hash: &stored,
                    result: None,
                    refusal: Some("policy_denied"),
                },
            )?;
            tx.execute("UPDATE receipt_approval_intents SET state='lost' WHERE approval_id=?1 AND state IN ('pending','granted')", [approval_id.to_string()])?;
            tx.execute("UPDATE receipt_actions SET state='refused',terminal_receipt_hash=?2 WHERE action_id=?1 AND terminal_receipt_hash IS NULL", params![action_id, hash])?;
            tx.commit()?;
            return Err(RuntimeError::Code("policy_denied"));
        }
        let mut bound = request.clone();
        bound.approval_id = Some(approval_id);
        let (hash, _) = signed_receipt(
            &tx,
            self.signer,
            SignedReceiptInput {
                request: &bound,
                kind: "pre_action",
                status: "prepared",
                args_hash: &stored,
                result: None,
                refusal: None,
            },
        )?;
        tx.execute("UPDATE receipt_approval_intents SET state='claimed' WHERE approval_id=?1 AND state='granted'", [approval_id.to_string()])?;
        tx.execute("UPDATE receipt_actions SET state='prepared',approval_id=?2,approval_call_hash=?3,pre_receipt_hash=?4 WHERE action_id=?1", params![action_id, approval_id.to_string(), stored, hash])?;
        tx.commit()?;
        Ok(PrepareOutcome::Prepared {
            action_id: request.action_id,
            receipt_hash: hash,
        })
    }

    /// Durable claim variant used by Core effect paths. The binding is
    /// checked in SQLite before the atomic claim so an approval cannot move
    /// between sessions or snapshots.
    pub fn claim_approval_checked_with_binding(
        &mut self,
        request: &ActionRequest,
        approval_id: Uuid,
        session_id: &str,
        snapshot_hash: &str,
        policy_version: u32,
        recheck_policy: impl FnOnce(&ActionRequest) -> bool,
    ) -> Result<PrepareOutcome, RuntimeError> {
        let binding: Option<(Option<String>, Option<String>, Option<i64>)> = self.connection
            .query_row(
                "SELECT session_id,snapshot_hash,policy_version FROM receipt_approval_intents WHERE approval_id=?1",
                [approval_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((stored_session, stored_snapshot, stored_version)) = binding else {
            return Err(RuntimeError::Code("approval_stale"));
        };
        if stored_session.as_deref() != Some(session_id)
            || stored_snapshot.as_deref() != Some(snapshot_hash)
            || stored_version != Some(policy_version as i64)
        {
            return Err(RuntimeError::Code("approval_binding_changed"));
        }
        self.claim_approval_checked(request, approval_id, recheck_policy)
    }

    pub fn complete(
        &mut self,
        request: &ActionRequest,
        status: &str,
        output_digest: &str,
        error_category: Option<&str>,
    ) -> Result<String, RuntimeError> {
        self.complete_inner(request, status, output_digest, error_category, None)
    }

    /// Completes a separately authorized read-only reconciliation action and
    /// links it to the historical pending action in the same transaction.
    pub fn complete_reconciliation(
        &mut self,
        request: &ActionRequest,
        old_action_id: Uuid,
        status: &str,
        output_digest: &str,
        error_category: Option<&str>,
    ) -> Result<String, RuntimeError> {
        if old_action_id == request.action_id {
            return Err(RuntimeError::Code("schema_violation"));
        }
        self.complete_inner(
            request,
            status,
            output_digest,
            error_category,
            Some(old_action_id),
        )
    }

    fn complete_inner(
        &mut self,
        request: &ActionRequest,
        status: &str,
        output_digest: &str,
        error_category: Option<&str>,
        reconciliation_old_action: Option<Uuid>,
    ) -> Result<String, RuntimeError> {
        require_ready(self.connection)?;
        if !matches!(status, "succeeded" | "failed" | "cancelled") {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let args_hash = canonical_call_hash(
            &request.tool_name,
            &request.normalized_scope,
            &request.input,
        )?;
        if output_digest.len() != 64
            || !output_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let projection = if status == "succeeded" {
            json!({"status":"succeeded","output_digest":output_digest})
        } else {
            json!({"status":status,"error_category":error_category.ok_or(RuntimeError::Code("schema_violation"))?})
        };
        let result = result_hash(&projection)?;
        let marker = bounded_result_marker(
            status,
            &result,
            error_category,
            now_ms(),
            status == "succeeded",
        )?;
        let tx = RetryTransaction::begin(self.connection)?;
        let (state, dispatch, stored_hash, task_id, run_id, tool_name, normalized_scope, policy_id, policy_decision, fingerprint_version, stored_approval, stored_parent, terminal_hash): ActionCompletionRow = tx.query_row(
            "SELECT state,dispatch_state,tool_args_hash,task_id,run_id,tool_name,normalized_scope,policy_id,policy_decision,fingerprint_input_version,approval_id,parent_approval_ref,terminal_receipt_hash FROM receipt_actions WHERE action_id=?1",
            [request.action_id.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?,r.get(12)?)))?;
        let binding_matches = state == "prepared" || state == "pending_recovery";
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
        if terminal_hash.is_some() {
            return Err(RuntimeError::Code("action_id_conflict"));
        }
        if !binding_matches || dispatch != "started" && dispatch != "returned" || !identity_matches
        {
            if binding_matches {
                tx.execute("UPDATE receipt_actions SET state='quarantined',recovery_code='unknown' WHERE action_id=?1 AND state IN ('prepared','pending_recovery')", [request.action_id.to_string()])?;
                increment_metric_tx(&tx, "receipt_schema_violations", 1)?;
                increment_metric_tx(&tx, "quarantined_count", 1)?;
                tx.commit()?;
            }
            return Err(RuntimeError::Code("schema_violation"));
        }
        if let Some(old_action_id) = reconciliation_old_action {
            let old_state: String = tx.query_row(
                "SELECT state FROM receipt_actions WHERE action_id=?1",
                [old_action_id.to_string()],
                |row| row.get(0),
            )?;
            if old_state != "pending_recovery" {
                return Err(RuntimeError::Code("pending_recovery"));
            }
        }
        let (hash, _) = signed_receipt(
            &tx,
            self.signer,
            SignedReceiptInput {
                request,
                kind: "post_action",
                status,
                args_hash: &stored_hash,
                result: Some(&result),
                refusal: None,
            },
        )?;
        tx.execute("UPDATE receipt_actions SET state=?2,dispatch_state='returned',result_hash=?3,result_marker=?4,terminal_receipt_hash=?5 WHERE action_id=?1", params![request.action_id.to_string(), status, result, marker, hash])?;
        if let Some(old_action_id) = reconciliation_old_action {
            let old_linked = tx.execute("UPDATE receipt_actions SET state='succeeded',reconciliation_action_id=?2,completion_source='reconciliation',terminal_receipt_hash=?3 WHERE action_id=?1 AND state='pending_recovery' AND reconciliation_action_id IS NULL", params![old_action_id.to_string(), request.action_id.to_string(), hash])?;
            let new_linked = tx.execute("UPDATE receipt_actions SET reconciles_action_id=?2,completion_source='reconciliation' WHERE action_id=?1 AND state=?3 AND reconciles_action_id IS NULL", params![request.action_id.to_string(), old_action_id.to_string(), status])?;
            if old_linked != 1 || new_linked != 1 {
                return Err(RuntimeError::Code("pending_recovery"));
            }
            tx.execute(
                "DELETE FROM receipt_protected_actions WHERE action_id=?1",
                [old_action_id.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(hash)
    }

    pub fn refuse(&mut self, request: &ActionRequest, code: &str) -> Result<String, RuntimeError> {
        require_ready(self.connection)?;
        if !matches!(
            code,
            "policy_denied"
                | "approval_denied"
                | "approval_expired"
                | "approval_stale"
                | "call_changed"
                | "key_untrusted"
                | "recovery_pending"
        ) {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let args_hash = canonical_call_hash(
            &request.tool_name,
            &request.normalized_scope,
            &request.input,
        )?;
        let tx = RetryTransaction::begin(self.connection)?;
        let binding: ActionBindingRow = tx.query_row(
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
        if binding.10.is_some() {
            return Err(RuntimeError::Code("action_id_conflict"));
        }
        if !identity_matches
            || !matches!(
                binding.9.as_str(),
                "awaiting_approval" | "prepared" | "pending_recovery"
            )
        {
            if matches!(binding.9.as_str(), "prepared" | "pending_recovery") {
                tx.execute("UPDATE receipt_actions SET state='quarantined',recovery_code='unknown' WHERE action_id=?1", [request.action_id.to_string()])?;
                increment_metric_tx(&tx, "receipt_schema_violations", 1)?;
                increment_metric_tx(&tx, "quarantined_count", 1)?;
                tx.commit()?;
            }
            return Err(RuntimeError::Code("schema_violation"));
        }
        let stored_hash: String = tx.query_row(
            "SELECT tool_args_hash FROM receipt_actions WHERE action_id=?1",
            [request.action_id.to_string()],
            |r| r.get(0),
        )?;
        if stored_hash != args_hash {
            if matches!(binding.9.as_str(), "prepared" | "pending_recovery") {
                tx.execute("UPDATE receipt_actions SET state='quarantined',recovery_code='unknown' WHERE action_id=?1", [request.action_id.to_string()])?;
                increment_metric_tx(&tx, "receipt_schema_violations", 1)?;
                increment_metric_tx(&tx, "quarantined_count", 1)?;
                tx.commit()?;
            }
            return Err(RuntimeError::Code("schema_violation"));
        }
        let (hash, _) = signed_receipt(
            &tx,
            self.signer,
            SignedReceiptInput {
                request,
                kind: "refusal",
                status: "refused",
                args_hash: &stored_hash,
                result: None,
                refusal: Some(code),
            },
        )?;
        let source = if code == "recovery_pending" {
            "reconciliation"
        } else {
            "execution"
        };
        tx.execute("UPDATE receipt_actions SET state='refused',completion_source=?3,terminal_receipt_hash=?2 WHERE action_id=?1", params![request.action_id.to_string(), hash, source])?;
        let approval_state = match code {
            "approval_expired" => "expired",
            "approval_denied" => "denied",
            _ => "lost",
        };
        tx.execute("UPDATE receipt_approval_intents SET state=?2 WHERE action_id=?1 AND state IN ('pending','granted')", params![request.action_id.to_string(), approval_state])?;
        tx.execute(
            "DELETE FROM receipt_protected_actions WHERE action_id=?1",
            [request.action_id.to_string()],
        )?;
        tx.commit()?;
        Ok(hash)
    }

    pub fn mark_pending_recovery(&self, action_id: Uuid, code: &str) -> Result<(), RuntimeError> {
        if !valid_recovery_code(code) {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let changed = self.connection.execute("UPDATE receipt_actions SET state='pending_recovery',recovery_code=?2 WHERE action_id=?1 AND dispatch_state IN ('started','returned')", params![action_id.to_string(), code])?;
        if changed == 1 {
            self.connection.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES('pending_recovery_count',1) ON CONFLICT(metric) DO UPDATE SET value=value+1", [])?;
        }
        Ok(())
    }

    /// Links a new, separately authorized read-only reconciliation action to a
    /// pending historical action. The original action is never dispatched by
    /// this operation and remains visible in its original audit state.
    pub fn link_reconciliation(
        &self,
        old_action_id: Uuid,
        new_action_id: Uuid,
    ) -> Result<(), RuntimeError> {
        if old_action_id == new_action_id {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let tx = self.connection.unchecked_transaction()?;
        let old_state: String = tx.query_row(
            "SELECT state FROM receipt_actions WHERE action_id=?1",
            [old_action_id.to_string()],
            |row| row.get(0),
        )?;
        let new_state: String = tx.query_row(
            "SELECT state FROM receipt_actions WHERE action_id=?1",
            [new_action_id.to_string()],
            |row| row.get(0),
        )?;
        if old_state != "pending_recovery"
            || !matches!(
                new_state.as_str(),
                "prepared" | "succeeded" | "failed" | "cancelled"
            )
        {
            return Err(RuntimeError::Code("pending_recovery"));
        }
        tx.execute("UPDATE receipt_actions SET reconciliation_action_id=?2 WHERE action_id=?1 AND reconciliation_action_id IS NULL", params![old_action_id.to_string(), new_action_id.to_string()])?;
        tx.execute("UPDATE receipt_actions SET reconciles_action_id=?2,completion_source='reconciliation' WHERE action_id=?1 AND reconciles_action_id IS NULL", params![new_action_id.to_string(), old_action_id.to_string()])?;
        tx.commit()?;
        Ok(())
    }

    /// Runs one bounded GC pass. If Recovery starts (and bumps
    /// `receipt_runtime_guard.generation`) between the pre-check and the
    /// transaction's own first read, the deletion is discarded and the next
    /// scheduled pass retries — GC and Recovery never race on the same
    /// intents.
    pub fn approval_gc(&self, now_ms_value: i64) -> Result<i64, RuntimeError> {
        let (phase, generation_before): (String, i64) = self.connection.query_row(
            "SELECT phase,generation FROM receipt_runtime_guard WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if phase != "ready" {
            return Err(RuntimeError::Code("pending_recovery"));
        }
        let cutoff = now_ms_value.saturating_sub(APPROVAL_TTL_MS);
        let tx = self.connection.unchecked_transaction()?;
        let (phase_in_tx, generation_in_tx): (String, i64) = tx.query_row(
            "SELECT phase,generation FROM receipt_runtime_guard WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if phase_in_tx != "ready" || generation_in_tx != generation_before {
            return Ok(0);
        }
        let deleted = tx.execute(
            "DELETE FROM receipt_approval_intents WHERE state IN ('expired','lost','claimed') AND expires_at_ms<=?1 AND NOT EXISTS (SELECT 1 FROM receipt_actions a WHERE a.action_id=receipt_approval_intents.action_id AND a.state='pending_recovery')",
            [cutoff],
        )?;
        increment_metric_tx(&tx, "approval_gc_deleted_count", deleted as i64)?;
        tx.commit()?;
        Ok(deleted as i64)
    }

    /// Retention v1 boundary finder: for each `key_id`, returns the `rowid`
    /// of the oldest row that must survive compaction — the more
    /// restrictive of "older than 90 calendar days" and "beyond the newest
    /// 100,000 rows", so the retained suffix satisfies both bounds at once.
    /// A row belonging to a `pending`/`pending_recovery`/`awaiting_approval`
    /// action is never counted as a deletion candidate, so the returned
    /// cutoff never crosses one — `compact_chain` still re-checks this
    /// under the transaction before deleting anything. A `key_id` with no
    /// candidate cutoff (nothing old enough and under budget) is omitted.
    pub fn retention_candidates(
        &self,
        now_ms_value: i64,
    ) -> Result<Vec<(String, i64)>, RuntimeError> {
        const RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;
        const RETENTION_ROWS: i64 = 100_000;
        let age_cutoff_ms = now_ms_value.saturating_sub(RETENTION_MS);
        let mut key_statement = self
            .connection
            .prepare("SELECT key_id FROM receipt_records WHERE source='signed' GROUP BY key_id")?;
        let key_ids: Vec<String> = key_statement
            .query_map([], |row| row.get(0))?
            .filter_map(|row| row.ok())
            .collect();
        let mut out = Vec::new();
        for key_id in key_ids {
            // Deletable rows only: blocked-by-pending rows are excluded
            // up front so neither bound below can ever pick one.
            let mut deletable_statement = self.connection.prepare(
                "SELECT r.rowid, r.created_at_ms FROM receipt_records r
                 LEFT JOIN receipt_actions a ON a.action_id=r.action_id
                 WHERE r.key_id=?1 AND r.source='signed'
                   AND (a.state IS NULL OR a.state NOT IN ('prepared','pending_recovery','awaiting_approval'))
                 ORDER BY r.rowid ASC",
            )?;
            let deletable: Vec<(i64, i64)> = deletable_statement
                .query_map([&key_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(|row| row.ok())
                .collect();
            let total: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM receipt_records WHERE key_id=?1 AND source='signed'",
                [&key_id],
                |r| r.get(0),
            )?;
            let over_row_budget = (total - RETENTION_ROWS).max(0) as usize;
            // Index of the first row this key's suffix must retain: the
            // later (more restrictive) of the age boundary and the count
            // boundary among the deletable rows.
            let age_boundary_index = deletable
                .iter()
                .position(|(_, created_at_ms)| *created_at_ms >= age_cutoff_ms)
                .unwrap_or(deletable.len());
            let count_boundary_index = over_row_budget.min(deletable.len());
            // Always retain at least the single newest deletable row so the
            // suffix never loses the row `receipt_chain_heads` points at,
            // even if every row happens to be old enough by both bounds.
            let keep_from_index = age_boundary_index
                .max(count_boundary_index)
                .min(deletable.len().saturating_sub(1));
            if keep_from_index > 0 {
                if let Some(&(cutoff_sequence, _)) = deletable.get(keep_from_index) {
                    // deletable[keep_from_index] is the first row this key's
                    // suffix retains — exactly the cutoff `compact_chain`
                    // expects (rowid < cutoff is deleted, rowid == cutoff
                    // becomes first_retained_hash).
                    out.push((key_id, cutoff_sequence));
                }
            }
        }
        Ok(out)
    }

    /// Retention v1 compaction: signs a `ReceiptCheckpointV1` for the prefix
    /// `[genesis, cutoff_sequence)` of `key_id`, then deletes exactly that
    /// prefix in the same transaction as the checkpoint insert. Refuses
    /// (`checkpoint_blocked_by_pending`) if any row in that prefix belongs to
    /// an action that has not reached a terminal state — those rows are
    /// retained until explicit reconciliation, never purged automatically.
    pub fn compact_chain(
        &mut self,
        key_id: &str,
        cutoff_sequence: i64,
    ) -> Result<ReceiptCheckpointRow, RuntimeError> {
        require_ready(self.connection)?;
        let tx = RetryTransaction::begin(self.connection)?;
        let first_retained_hash: String = tx
            .query_row(
                "SELECT receipt_hash FROM receipt_records WHERE key_id=?1 AND rowid=?2",
                params![key_id, cutoff_sequence],
                |row| row.get(0),
            )
            .map_err(|_| RuntimeError::Code("checkpoint_cutoff_invalid"))?;
        let prefix_last_hash: String = tx.query_row(
            "SELECT receipt_hash FROM receipt_records WHERE key_id=?1 AND rowid<?2 ORDER BY rowid DESC LIMIT 1",
            params![key_id, cutoff_sequence], |row| row.get(0),
        ).map_err(|_| RuntimeError::Code("checkpoint_cutoff_invalid"))?;
        let head_receipt_hash: String = tx
            .query_row(
                "SELECT receipt_hash FROM receipt_chain_heads WHERE key_id=?1",
                [key_id],
                |row| row.get(0),
            )
            .map_err(|_| RuntimeError::Code("checkpoint_cutoff_invalid"))?;
        let blocking: i64 = tx.query_row(
            "SELECT COUNT(*) FROM receipt_records r LEFT JOIN receipt_actions a ON a.action_id=r.action_id
             WHERE r.key_id=?1 AND r.rowid<?2
               AND (a.state IS NULL OR a.state IN ('prepared','pending_recovery','awaiting_approval'))",
            params![key_id, cutoff_sequence], |row| row.get(0),
        )?;
        if blocking > 0 {
            return Err(RuntimeError::Code("checkpoint_blocked_by_pending"));
        }
        let checkpoint_id = Uuid::now_v7().to_string();
        let created_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        // The signing key is whichever key is active right now, which may
        // differ from `key_id` (the chain segment being compacted) when
        // compacting a retired key's tail after rotation — the verifier
        // resolves trust through this field, never by assuming the two
        // key ids are the same.
        let signed_by_key_id = self.signer.key_id()?;
        let unsigned = json!({
            "checkpoint_version": 1,
            "checkpoint_id": checkpoint_id,
            "key_id": key_id,
            "cutoff_sequence": cutoff_sequence.to_string(),
            "first_retained_hash": first_retained_hash,
            "prefix_last_hash": prefix_last_hash,
            "last_deleted_receipt_hash": prefix_last_hash,
            "head_receipt_hash": head_receipt_hash,
            "created_at": created_at,
            "signed_by_key_id": signed_by_key_id,
        });
        let canonical = canonicalize_json(
            &serde_json::to_vec(&unsigned).map_err(|_| ReceiptError::InvalidJson)?,
        )?;
        let digest = crate::sha256_hex(&canonical);
        let signature = self.signer.sign_payload_hash(&digest)?;
        tx.execute(
            "UPDATE receipt_checkpoints SET status='superseded' WHERE key_id=?1 AND status='active'",
            [key_id],
        )?;
        tx.execute(
            "INSERT INTO receipt_checkpoints(checkpoint_id,key_id,cutoff_sequence,first_retained_hash,prefix_last_hash,last_deleted_receipt_hash,head_receipt_hash,created_at,canonical_checkpoint,signed_by_key_id,signature,status) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'active')",
            params![checkpoint_id, key_id, cutoff_sequence, first_retained_hash, prefix_last_hash, prefix_last_hash, head_receipt_hash, created_at, canonical, signed_by_key_id, signature],
        )?;
        tx.execute(
            "DELETE FROM receipt_records WHERE key_id=?1 AND rowid<?2",
            params![key_id, cutoff_sequence],
        )?;
        tx.commit()?;
        Ok(ReceiptCheckpointRow {
            checkpoint_id,
            key_id: key_id.to_string(),
            cutoff_sequence,
            first_retained_hash,
            prefix_last_hash: prefix_last_hash.clone(),
            last_deleted_receipt_hash: prefix_last_hash,
            head_receipt_hash,
            created_at,
            signed_by_key_id,
            signature,
            status: "active".into(),
        })
    }

    /// Latest active checkpoint for `key_id`, if retention has ever compacted
    /// a prefix. `None` means the chain's genesis is still the true start.
    pub fn active_checkpoint(
        &self,
        key_id: &str,
    ) -> Result<Option<ReceiptCheckpointRow>, RuntimeError> {
        self.connection.query_row(
            "SELECT checkpoint_id,key_id,cutoff_sequence,first_retained_hash,prefix_last_hash,last_deleted_receipt_hash,head_receipt_hash,created_at,signed_by_key_id,signature,status \
             FROM receipt_checkpoints WHERE key_id=?1 AND status='active'",
            [key_id],
            |row| Ok(ReceiptCheckpointRow {
                checkpoint_id: row.get(0)?,
                key_id: row.get(1)?,
                cutoff_sequence: row.get(2)?,
                first_retained_hash: row.get(3)?,
                prefix_last_hash: row.get(4)?,
                last_deleted_receipt_hash: row.get(5)?,
                head_receipt_hash: row.get(6)?,
                created_at: row.get(7)?,
                signed_by_key_id: row.get(8)?,
                signature: row.get(9)?,
                status: row.get(10)?,
            }),
        ).optional().map_err(RuntimeError::from)
    }

    pub fn store_protected_action(
        &self,
        row: &ProtectedActionRow,
        key: &[u8; 32],
    ) -> Result<(), RuntimeError> {
        let envelope = protect_action_row(row, key)?;
        self.connection.execute(
            "INSERT INTO receipt_protected_actions(action_id,key_id,envelope,created_at_ms) VALUES(?1,?2,?3,?4) ON CONFLICT(action_id) DO UPDATE SET key_id=excluded.key_id,envelope=excluded.envelope",
            params![row.action_id, row.key_id, envelope, row.created_at_ms],
        )?;
        Ok(())
    }

    pub fn store_protected_envelope(
        &self,
        row: &ProtectedActionRow,
        envelope: Vec<u8>,
    ) -> Result<(), RuntimeError> {
        if envelope.len() > MAX_PROTECTED_ROW_BYTES {
            return Err(RuntimeError::Code("storage_key_unavailable"));
        }
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
        if job_id.is_empty()
            || job_id.len() > 128
            || old_key_id.is_empty()
            || old_key_id.len() > 128
            || new_key_id.is_empty()
            || new_key_id.len() > 128
            || batch_size == 0
            || batch_size > 64
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
            } else if current_job != job_id
                || current_old != old_key_id
                || current_new != new_key_id
                || current_generation != generation
            {
                return Err(RuntimeError::Code("chain_conflict"));
            }
        } else {
            self.connection.execute(
                "INSERT INTO receipt_storage_rotation(id,job_id,old_key_id,new_key_id,cursor,generation,state,updated_at_ms) VALUES(1,?1,?2,?3,'',?4,'running',?5)",
                params![job_id, old_key_id, new_key_id, generation, now_ms()],
            )?;
        }
        let cursor: String = self.connection.query_row(
            "SELECT cursor FROM receipt_storage_rotation WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        let rows: Vec<(String, Vec<u8>)> = {
            let mut statement = self.connection.prepare(
                "SELECT action_id,envelope FROM receipt_protected_actions WHERE action_id>?1 ORDER BY action_id LIMIT ?2",
            )?;
            let mapped = statement.query_map(params![cursor, batch_size as i64], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        if rows.is_empty() {
            self.connection.execute(
                "UPDATE receipt_storage_rotation SET state='completed',updated_at_ms=?1 WHERE id=1",
                [now_ms()],
            )?;
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
            tx.execute(
                "UPDATE receipt_protected_actions SET key_id=?2,envelope=?3 WHERE action_id=?1",
                params![action_id, new_key_id, envelope],
            )?;
            last = action_id;
        }
        tx.execute("UPDATE receipt_storage_rotation SET cursor=?1,state='running',updated_at_ms=?2 WHERE id=1", params![last, now_ms()])?;
        tx.execute("INSERT INTO receipt_storage_rotation_audit(job_id,cursor,old_key_id,new_key_id,processed_count,outcome,created_at_ms) VALUES(?1,?2,?3,?4,?5,'batch_committed',?6)", params![job_id, last, old_key_id, new_key_id, processed_count, now_ms()])?;
        tx.commit()?;
        Ok(true)
    }

    pub fn load_protected_action(
        &self,
        action_id: Uuid,
        key: &[u8; 32],
    ) -> Result<ProtectedActionRow, RuntimeError> {
        let (stored_key_id, envelope): (String, Vec<u8>) = self.connection.query_row(
            "SELECT key_id,envelope FROM receipt_protected_actions WHERE action_id=?1",
            [action_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let row = unprotect_action_row(&envelope, key)?;
        if row.action_id != action_id.to_string() || row.key_id != stored_key_id {
            return Err(RuntimeError::Code("pending_recovery"));
        }
        Ok(row)
    }

    /// Reads a protected row during storage-key rotation, trying the new key
    /// first and falling back to the old key in that fixed order. The order
    /// is deterministic and never chosen at random: a row rewrapped by a
    /// concurrent rotation batch must decrypt with the new key, while an
    /// unrewrapped row still decrypts with the old one.
    pub fn load_protected_action_with_fallback(
        &self,
        action_id: Uuid,
        new_key: &[u8; 32],
        old_key: &[u8; 32],
    ) -> Result<ProtectedActionRow, RuntimeError> {
        match self.load_protected_action(action_id, new_key) {
            Ok(row) => Ok(row),
            Err(_) => self.load_protected_action(action_id, old_key),
        }
    }

    pub fn delete_protected_after_terminal(&self, action_id: Uuid) -> Result<(), RuntimeError> {
        let terminal: Option<String> = self
            .connection
            .query_row(
                "SELECT state FROM receipt_actions WHERE action_id=?1",
                [action_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        if !matches!(
            terminal.as_deref(),
            Some("succeeded" | "failed" | "cancelled" | "refused")
        ) {
            return Err(RuntimeError::Code("pending_recovery"));
        }
        self.connection.execute(
            "DELETE FROM receipt_protected_actions WHERE action_id=?1",
            [action_id.to_string()],
        )?;
        Ok(())
    }

    pub fn quarantine(&self, action_id: Uuid, reason: &str) -> Result<(), RuntimeError> {
        if reason.is_empty() || reason.len() > 128 {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let changed = self.connection.execute("UPDATE receipt_actions SET state='quarantined',recovery_code='unknown' WHERE action_id=?1 AND state IN ('prepared','pending_recovery')", [action_id.to_string()])?;
        if changed != 1 {
            return Err(RuntimeError::Code("schema_violation"));
        }
        self.connection.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES('quarantined_count',1) ON CONFLICT(metric) DO UPDATE SET value=value+1", [])?;
        Ok(())
    }

    /// Authenticated operator closure for an invariant-violating action. It
    /// can only produce a signed terminal refusal and never re-enables dispatch.
    pub fn unquarantine(
        &mut self,
        request: &ActionRequest,
        authenticated_operator: bool,
        checkpoint: &str,
    ) -> Result<String, RuntimeError> {
        require_ready(self.connection)?;
        if !authenticated_operator || checkpoint.is_empty() || checkpoint.len() > 256 {
            return Err(RuntimeError::Code("key_untrusted"));
        }
        let args_hash = canonical_call_hash(
            &request.tool_name,
            &request.normalized_scope,
            &request.input,
        )?;
        let tx = RetryTransaction::begin(self.connection)?;
        let binding: ActionBindingRow = tx.query_row(
            "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,policy_decision,fingerprint_input_version,approval_id,parent_approval_ref,state,terminal_receipt_hash FROM receipt_actions WHERE action_id=?1",
            [request.action_id.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?)))?;
        if binding.9 != "quarantined"
            || binding.10.is_some()
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
        let stored_hash: String = tx.query_row(
            "SELECT tool_args_hash FROM receipt_actions WHERE action_id=?1",
            [request.action_id.to_string()],
            |row| row.get(0),
        )?;
        if stored_hash != args_hash {
            return Err(RuntimeError::Code("schema_violation"));
        }
        let (hash, _) = signed_receipt(
            &tx,
            self.signer,
            SignedReceiptInput {
                request,
                kind: "refusal",
                status: "refused",
                args_hash: &stored_hash,
                result: None,
                refusal: Some("recovery_pending"),
            },
        )?;
        tx.execute("UPDATE receipt_actions SET state='refused',recovery_code='unknown',completion_source='reconciliation',terminal_receipt_hash=?2 WHERE action_id=?1 AND state='quarantined'", params![request.action_id.to_string(), hash])?;
        tx.execute(
            "DELETE FROM receipt_protected_actions WHERE action_id=?1",
            [request.action_id.to_string()],
        )?;
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

    pub fn set_audit_sampling_rate(
        &self,
        authenticated_core_command: bool,
        rate: u8,
    ) -> Result<(), RuntimeError> {
        if !authenticated_core_command || rate > 100 {
            return Err(RuntimeError::Code("key_untrusted"));
        }
        let tx = self.connection.unchecked_transaction()?;
        let old_rate: i64 = tx.query_row(
            "SELECT audit_sampling_rate FROM receipt_runtime_config WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        tx.execute("UPDATE receipt_runtime_config SET audit_sampling_rate=?1,sampling_policy_version=?2 WHERE id=1", params![rate as i64, crate::SAMPLING_POLICY_VERSION as i64])?;
        tx.execute("INSERT INTO receipt_sampling_changes(old_rate,new_rate,sampling_policy_version,created_at_ms) VALUES(?1,?2,?3,?4)", params![old_rate, rate as i64, crate::SAMPLING_POLICY_VERSION as i64, now_ms()])?;
        tx.commit()?;
        Ok(())
    }

    pub fn store_unsampled_read_only_marker(
        &self,
        action_id: Uuid,
        tool_name: &str,
        call_hash: &str,
        policy_version: u8,
    ) -> Result<(), RuntimeError> {
        if tool_name.is_empty()
            || tool_name.len() > crate::MAX_IDENTIFIER_BYTES
            || call_hash.len() != 64
            || !call_hash
                .bytes()
                .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
        {
            return Err(RuntimeError::Code("schema_violation"));
        }
        self.connection.execute("INSERT INTO receipt_audit_markers(action_id,tool_name,call_hash,sampled,sampling_policy_version,created_at_ms) VALUES(?1,?2,?3,0,?4,?5)", params![action_id.to_string(), tool_name, call_hash, policy_version as i64, now_ms()])?;
        self.connection.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES('read_only_unsampled_count',1) ON CONFLICT(metric) DO UPDATE SET value=value+1", [])?;
        Ok(())
    }

    /// Records a bounded unsigned runtime fact. It is deliberately stored in
    /// diagnostics only and can never be interpreted as a receipt or advance
    /// the hash chain.
    pub fn store_unsigned_runtime_marker(
        &self,
        action_id: Uuid,
        code: &str,
    ) -> Result<(), RuntimeError> {
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
        let mut statement = self.connection.prepare(
            "SELECT metric,value FROM receipt_runtime_metrics ORDER BY metric LIMIT 128",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut counters = BTreeMap::new();
        for row in rows {
            let (metric, value) = row?;
            counters.insert(metric, value);
        }
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
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut counts = BTreeMap::new();
        for row in rows {
            let (code, count) = row?;
            counts.insert(code, count);
        }
        Ok(counts)
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
