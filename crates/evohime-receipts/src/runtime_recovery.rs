use crate::runtime::{increment_metric_tx, install_schema, now_ms};
use crate::runtime_contract::RuntimeError;
use crate::runtime_platform::{boot_id, monotonic_ms};
use rusqlite::{params, Connection};
use std::time::{Duration, Instant};

/// Reconciles interrupted receipt actions and returns the number of recovered rows.
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
    let wall_now = now_ms();
    let mono_now = monotonic_ms()?;
    tx.execute("UPDATE receipt_approval_intents SET state='expired' WHERE state IN ('pending','granted') AND ((clock_boot_id=?1 AND deadline_monotonic_ms<=?2) OR (clock_boot_id<>?1 AND expires_at_ms<=?3))", params![boot_id()?, mono_now, wall_now])?;
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
        increment_metric_tx(
            &tx,
            "receipt_schema_violations",
            invariant_count
                .saturating_add(orphan_count)
                .saturating_add(terminal_without_pre_count),
        )?;
        if invariant_count > 0 {
            increment_metric_tx(&tx, "quarantined_count", invariant_count)?;
        }
        increment_metric_tx(&tx, "recovery_safe_mode", 1)?;
        tx.execute(
            "UPDATE receipt_runtime_guard SET phase='read_only_recovery',updated_at_ms=?1",
            [now_ms()],
        )?;
        tx.commit()?;
        return Err(RuntimeError::Code("schema_violation"));
    }
    let pending: i64 = tx.query_row("SELECT COUNT(*) FROM receipt_actions WHERE state IN ('prepared','pending_recovery','quarantined')", [], |r| r.get(0))?;
    increment_metric_tx(
        &tx,
        "recovery_duration_ms",
        recovery_started.elapsed().as_millis().min(i64::MAX as u128) as i64,
    )?;
    tx.execute(
        "UPDATE receipt_runtime_guard SET phase='ready',updated_at_ms=?1 WHERE id=1",
        [now_ms()],
    )?;
    tx.commit()?;
    Ok(pending)
}
