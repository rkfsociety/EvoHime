use crate::runtime_contract::RuntimeError;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

/// Stores one immutable snapshot. Re-inserting the same hash is idempotent,
/// while a hash collision with different canonical bytes fails closed.
pub fn persist_capability_snapshot(
    connection: &Connection,
    snapshot: &crate::capability::CapabilitySnapshotV1,
) -> Result<(), RuntimeError> {
    snapshot
        .validate()
        .map_err(|_| RuntimeError::Code("policy_error"))?;
    let expected = snapshot
        .compute_hash()
        .map_err(|_| RuntimeError::Code("policy_error"))?;
    if expected != snapshot.snapshot_hash {
        return Err(RuntimeError::Code("policy_error"));
    }
    let canonical = snapshot
        .canonical_bytes()
        .map_err(|_| RuntimeError::Code("policy_error"))?;
    let summary = crate::canonicalize_json(
        &serde_json::to_vec(&snapshot.redacted_summary())
            .map_err(|_| RuntimeError::Code("policy_error"))?,
    )
    .map_err(|_| RuntimeError::Code("policy_error"))?;
    let changed = connection.execute(
        "INSERT INTO receipt_capability_snapshots(snapshot_hash,snapshot_id,run_id,session_id,task_id,policy_id,policy_version,canonical_snapshot,redacted_summary,created_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(snapshot_hash) DO NOTHING",
        params![snapshot.snapshot_hash, snapshot.snapshot_id, snapshot.run_id, snapshot.session_id, snapshot.task_id, snapshot.policy_id, snapshot.policy_version as i64, canonical, summary, crate::runtime::now_ms()],
    )?;
    if changed == 0 {
        let stored: Vec<u8> = connection.query_row(
            "SELECT canonical_snapshot FROM receipt_capability_snapshots WHERE snapshot_hash=?1",
            [&snapshot.snapshot_hash],
            |row| row.get(0),
        )?;
        if stored != canonical {
            return Err(RuntimeError::Code("policy_error"));
        }
    }
    Ok(())
}

type CapabilityBindingRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
);

type ApprovalBindingRow = (Option<String>, Option<String>, Option<i64>, Option<i64>);

/// Binds a validated capability snapshot and hook-chain version to a prepared action.
pub fn bind_capability_to_action(
    connection: &Connection,
    action_id: Uuid,
    snapshot: &crate::capability::CapabilitySnapshotV1,
    hook_chain_version: u32,
) -> Result<(), RuntimeError> {
    persist_capability_snapshot(connection, snapshot)?;
    let current: Option<CapabilityBindingRow> = connection
        .query_row(
            "SELECT session_id,snapshot_id,snapshot_hash,policy_version,hook_chain_version FROM receipt_actions WHERE action_id=?1",
            [action_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()?;
    let Some((session, snapshot_id, snapshot_hash, policy_version, existing_hook)) = current else {
        return Err(RuntimeError::Code("action_not_found"));
    };
    if session
        .as_deref()
        .is_some_and(|value| value != snapshot.session_id)
        || snapshot_id
            .as_deref()
            .is_some_and(|value| value != snapshot.snapshot_id)
        || snapshot_hash
            .as_deref()
            .is_some_and(|value| value != snapshot.snapshot_hash)
        || policy_version.is_some_and(|value| value != snapshot.policy_version as i64)
        || existing_hook.is_some_and(|value| value != hook_chain_version as i64)
    {
        return Err(RuntimeError::Code("policy_error"));
    }
    let changed = connection.execute(
        "UPDATE receipt_actions SET session_id=?2,snapshot_id=?3,snapshot_hash=?4,policy_version=?5,hook_chain_version=?6 WHERE action_id=?1",
        params![action_id.to_string(), snapshot.session_id, snapshot.snapshot_id, snapshot.snapshot_hash, snapshot.policy_version as i64, hook_chain_version as i64],
    )?;
    if changed != 1 {
        return Err(RuntimeError::Code("action_not_found"));
    }
    let approval_current: Option<ApprovalBindingRow> = connection
        .query_row(
            "SELECT session_id,snapshot_hash,policy_version,hook_chain_version FROM receipt_approval_intents WHERE action_id=?1",
            [action_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    if let Some((session, hash, version, hook)) = approval_current {
        if session
            .as_deref()
            .is_some_and(|value| value != snapshot.session_id)
            || hash
                .as_deref()
                .is_some_and(|value| value != snapshot.snapshot_hash)
            || version.is_some_and(|value| value != snapshot.policy_version as i64)
            || hook.is_some_and(|value| value != hook_chain_version as i64)
        {
            return Err(RuntimeError::Code("policy_error"));
        }
    }
    connection.execute(
        "UPDATE receipt_approval_intents SET session_id=?2,snapshot_hash=?3,policy_version=?4,hook_chain_version=?5 WHERE action_id=?1",
        params![action_id.to_string(), snapshot.session_id, snapshot.snapshot_hash, snapshot.policy_version as i64, hook_chain_version as i64],
    )?;
    Ok(())
}

/// Persists an immutable policy decision associated with an action.
pub fn persist_policy_decision(
    connection: &Connection,
    action_id: Uuid,
    snapshot_hash: Option<&str>,
    decision: &crate::capability::PolicyDecision,
) -> Result<(), RuntimeError> {
    if decision.reason_code.is_empty() || decision.reason_code.len() > 512 {
        return Err(RuntimeError::Code("policy_error"));
    }
    let outcome = match decision.outcome {
        crate::capability::PolicyOutcome::Allowed => "allowed",
        crate::capability::PolicyOutcome::ApprovalRequired => "approval_required",
        crate::capability::PolicyOutcome::Denied => "denied",
        crate::capability::PolicyOutcome::Unavailable => "unavailable",
        crate::capability::PolicyOutcome::Expired => "expired",
        crate::capability::PolicyOutcome::Cancelled => "cancelled",
        crate::capability::PolicyOutcome::PolicyError => "policy_error",
        crate::capability::PolicyOutcome::UnknownOutcome => "unknown_outcome",
    };
    let changed = connection.execute(
        "INSERT INTO receipt_policy_decisions(action_id,snapshot_hash,outcome,reason_code,retryable,created_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(action_id) DO NOTHING",
        params![action_id.to_string(), snapshot_hash, outcome, decision.reason_code, i64::from(decision.retryable), crate::runtime::now_ms()],
    )?;
    if changed == 0 {
        let existing: (Option<String>, String, String, i64) = connection.query_row(
            "SELECT snapshot_hash,outcome,reason_code,retryable FROM receipt_policy_decisions WHERE action_id=?1",
            [action_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if existing.0.as_deref() != snapshot_hash
            || existing.1 != outcome
            || existing.2 != decision.reason_code
            || existing.3 != i64::from(decision.retryable)
        {
            return Err(RuntimeError::Code("policy_error"));
        }
    }
    Ok(())
}

/// Computes the canonical input hash after enforcing the receipt call-size limit.
pub fn canonical_call_hash(
    tool_name: &str,
    normalized_scope: &str,
    input: &serde_json::Value,
) -> Result<String, RuntimeError> {
    if tool_name.contains('\n') || normalized_scope.contains('\n') {
        return Err(RuntimeError::Code("schema_violation"));
    }
    let fingerprint = evohime_permissions::fingerprint_input(input);
    if fingerprint.len() > crate::runtime::MAX_CALL_INPUT_BYTES {
        return Err(RuntimeError::Code("call_input_too_large"));
    }
    Ok(evohime_permissions::canonical_call_hash(
        tool_name,
        normalized_scope,
        input,
    ))
}
