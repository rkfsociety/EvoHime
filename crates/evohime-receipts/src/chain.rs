//! Verify-chain algorithm for stage 01.4 (chain storage and export).
//!
//! This module verifies an ordered slice of receipt rows (from SQLite or an
//! export bundle) against signed key history: envelope/signature integrity,
//! `previous_receipt_hash` linkage, key trust/rotation boundaries and
//! pre/terminal action pairing. It never repairs input; a malformed or
//! inconsistent row always degrades the chain, never gets silently skipped.

use crate::key_lifecycle::{
    verify_transitions, KeyTransition, VerificationStatus as KeyVerificationStatus,
};
use crate::{receipt_hash, verify_runtime_signature, Envelope};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainStatus {
    // Ordered worst-to-best so overall status can be taken as `max`.
    Broken,
    StaleKey,
    Unverified,
    Pending,
    VerifiedPruned,
    Verified,
}

/// One row of chain input. Fields that are part of the signed payload
/// (`previous_receipt_hash`, `action_id`, `approval_id`, `approval_call_hash`)
/// are read from the caller-supplied copies, not re-derived here; callers are
/// responsible for having extracted them from `envelope.payload` themselves
/// so this module stays free of the 01.1 payload shape.
#[derive(Debug, Clone)]
pub struct ChainRow {
    pub sequence: i64,
    pub receipt_id: String,
    pub action_id: String,
    pub receipt_kind: String,
    pub action_status: String,
    pub key_id: String,
    pub receipt_hash: String,
    pub previous_receipt_hash: Option<String>,
    pub approval_id: Option<String>,
    pub approval_call_hash: Option<String>,
    pub envelope: Envelope,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiptVerification {
    pub receipt_id: String,
    pub sequence: i64,
    pub status: ChainStatus,
    pub code: Option<&'static str>,
    pub receipt_hash: String,
    pub key_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChainVerification {
    pub status: ChainStatus,
    pub code: Option<&'static str>,
    pub requested_count: usize,
    pub actual_verified_count: usize,
    pub chain_start_hash: Option<String>,
    pub chain_end_hash: Option<String>,
    pub rows: Vec<ReceiptVerification>,
}

/// `checkpoint_prefix_hash` is the `first_retained_hash`/prefix boundary of a
/// trusted `ReceiptCheckpointV1` when the caller supplies one; when present it
/// authorizes the first row to have an absent predecessor without treating the
/// chain as broken, and downgrades an otherwise-`Verified` result to
/// `VerifiedPruned`.
pub fn verify_chain(
    rows: &[ChainRow],
    key_history: &[KeyTransition],
    trust_key: Option<&str>,
    checkpoint_prefix_hash: Option<&str>,
) -> ChainVerification {
    if rows.is_empty() {
        return ChainVerification {
            status: ChainStatus::Verified,
            code: None,
            requested_count: 0,
            actual_verified_count: 0,
            chain_start_hash: None,
            chain_end_hash: None,
            rows: Vec::new(),
        };
    }

    let key_trust = verify_transitions(key_history, trust_key);
    let mut public_keys: HashMap<&str, Vec<u8>> = HashMap::new();
    let mut stale_keys: HashSet<&str> = HashSet::new();
    let mut boundary_reasons: HashMap<&str, &str> = HashMap::new();
    for transition in key_history {
        if let Ok(bytes) = crate::key_lifecycle::public_key_bytes(&transition.new_public_key) {
            public_keys.insert(transition.new_key_id.as_str(), bytes);
        }
        boundary_reasons.insert(transition.new_key_id.as_str(), transition.continuity.as_str());
        if matches!(transition.continuity.as_str(), "compromised" | "broken") {
            if let Some(previous) = transition.previous_key_id.as_deref() {
                stale_keys.insert(previous);
            }
        }
    }
    let key_history_ok = matches!(key_trust, Ok(KeyVerificationStatus::Verified));
    let key_history_broken = matches!(key_trust, Ok(KeyVerificationStatus::Broken) | Err(_));

    let mut verifications: Vec<ReceiptVerification> = Vec::with_capacity(rows.len());
    let mut overall = ChainStatus::Verified;
    let mut overall_code: Option<&'static str> = None;
    let bump = |status: ChainStatus, code: &'static str, overall: &mut ChainStatus, overall_code: &mut Option<&'static str>| {
        if status < *overall {
            *overall = status;
            *overall_code = Some(code);
        }
    };

    for (index, row) in rows.iter().enumerate() {
        let mut status = ChainStatus::Verified;
        let mut code: Option<&'static str> = None;

        let recomputed = receipt_hash(&row.envelope).ok();
        if recomputed.as_deref() != Some(row.receipt_hash.as_str()) {
            status = ChainStatus::Broken;
            code = Some("receipts.hash_mismatch");
        } else if let Some(public) = public_keys.get(row.key_id.as_str()) {
            if verify_runtime_signature(&row.envelope, public).is_err() {
                status = ChainStatus::Broken;
                code = Some("receipts.signature_invalid");
            }
        } else {
            status = ChainStatus::Unverified;
            code = Some("receipts.key_unknown");
        }

        if status != ChainStatus::Broken {
            if index == 0 {
                if row.previous_receipt_hash.is_some() && checkpoint_prefix_hash.is_none() {
                    status = ChainStatus::Broken;
                    code = Some("receipts.chain_incomplete");
                } else if row.previous_receipt_hash.is_none() && checkpoint_prefix_hash.is_some() {
                    // Authorized pruned prefix: continues below as pruned.
                } else if row.previous_receipt_hash.is_some() {
                    status = ChainStatus::Broken;
                    code = Some("receipts.previous_mismatch");
                }
            } else {
                let previous_row = &rows[index - 1];
                if previous_row.key_id == row.key_id {
                    if row.previous_receipt_hash.as_deref() != Some(previous_row.receipt_hash.as_str()) {
                        status = ChainStatus::Broken;
                        code = Some("receipts.previous_mismatch");
                    } else if row.sequence <= previous_row.sequence {
                        status = ChainStatus::Broken;
                        code = Some("receipts.chain_cycle");
                    }
                } else {
                    // Key segment boundary: must be an authorized rotation
                    // (continuity "chained"/"genesis"), never a forced link.
                    if row.previous_receipt_hash.is_some() {
                        status = ChainStatus::Broken;
                        code = Some("receipts.previous_mismatch");
                    } else {
                        let reason = boundary_reasons.get(row.key_id.as_str()).copied();
                        if !matches!(reason, Some("chained") | Some("genesis")) {
                            status = ChainStatus::Unverified;
                            code = Some("receipts.chain_incomplete");
                        }
                    }
                }
            }
        }

        if status < ChainStatus::Broken {
            if stale_keys.contains(row.key_id.as_str()) {
                status = ChainStatus::StaleKey;
                code = Some("receipts.stale_key");
            } else if key_history_broken {
                status = ChainStatus::min(status, ChainStatus::Broken);
                code = Some("receipts.chain_incomplete");
            } else if !key_history_ok && status == ChainStatus::Verified {
                status = ChainStatus::Unverified;
                code = Some("receipts.key_unknown");
            }
        }

        if status == ChainStatus::Verified && checkpoint_prefix_hash.is_some() && index == 0 {
            status = ChainStatus::VerifiedPruned;
        }

        bump(status, code.unwrap_or("receipts.broken"), &mut overall, &mut overall_code);
        verifications.push(ReceiptVerification {
            receipt_id: row.receipt_id.clone(),
            sequence: row.sequence,
            status,
            code,
            receipt_hash: row.receipt_hash.clone(),
            key_id: row.key_id.clone(),
        });
    }

    // Action pairing: exactly one pre before terminal, by durable sequence.
    let mut pre_by_action: HashMap<&str, &ChainRow> = HashMap::new();
    let mut terminal_by_action: HashMap<&str, Vec<&ChainRow>> = HashMap::new();
    for row in rows {
        if row.receipt_kind == "pre_action" {
            if pre_by_action.insert(row.action_id.as_str(), row).is_some() {
                bump(ChainStatus::Broken, "receipts.chain_fork", &mut overall, &mut overall_code);
            }
        } else {
            terminal_by_action.entry(row.action_id.as_str()).or_default().push(row);
        }
    }
    for (action_id, terminals) in &terminal_by_action {
        if terminals.len() > 1 {
            bump(ChainStatus::Broken, "receipts.chain_fork", &mut overall, &mut overall_code);
        }
        let Some(pre) = pre_by_action.get(action_id) else {
            bump(ChainStatus::Unverified, "receipts.missing_receipt", &mut overall, &mut overall_code);
            continue;
        };
        for terminal in terminals {
            if terminal.sequence <= pre.sequence {
                bump(ChainStatus::Broken, "receipts.previous_mismatch", &mut overall, &mut overall_code);
            }
            if terminal.approval_id.is_some() && terminal.approval_id != pre.approval_id {
                bump(ChainStatus::Unverified, "receipts.approval_unverified", &mut overall, &mut overall_code);
            }
            if terminal.approval_call_hash.is_some() && terminal.approval_call_hash != pre.approval_call_hash {
                bump(ChainStatus::Unverified, "receipts.approval_unverified", &mut overall, &mut overall_code);
            }
        }
    }
    for (action_id, pre) in &pre_by_action {
        if !terminal_by_action.contains_key(action_id) {
            bump(ChainStatus::Pending, "receipts.pending", &mut overall, &mut overall_code);
            let _ = pre;
        }
    }

    let actual_verified_count = verifications
        .iter()
        .filter(|entry| matches!(entry.status, ChainStatus::Verified | ChainStatus::VerifiedPruned))
        .count();

    ChainVerification {
        status: overall,
        code: overall_code,
        requested_count: rows.len(),
        actual_verified_count,
        chain_start_hash: rows.first().map(|row| row.receipt_hash.clone()),
        chain_end_hash: rows.last().map(|row| row.receipt_hash.clone()),
        rows: verifications,
    }
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
            self.0.load_signer().map(|(metadata, _)| metadata.key_id)
                .map_err(|_| crate::runtime::RuntimeError::SignerUnavailable)
        }
        fn sign_payload_hash(&self, payload_hash: &str) -> Result<String, crate::runtime::RuntimeError> {
            self.0.sign_payload_hash(payload_hash).map(|(_, signature)| signature)
                .map_err(|_| crate::runtime::RuntimeError::SignerUnavailable)
        }
    }

    fn make_signer() -> (TestSigner, KeyTransition) {
        let root = std::env::temp_dir().join(format!("evohime-chain-test-{}", uuid::Uuid::now_v7()));
        let manager = std::sync::Arc::new(ReceiptKeyManager::new(&root));
        manager.initialize().unwrap();
        let genesis = manager.load_history().unwrap().into_iter().next().unwrap();
        (TestSigner(manager), genesis)
    }

    fn install(connection: &Connection) {
        crate::runtime::install_schema(connection).unwrap();
    }

    fn action_request(action_id: uuid::Uuid, tool: &str) -> ActionRequest {
        ActionRequest {
            action_id,
            task_id: "task-01".into(),
            run_id: "run-01".into(),
            tool_name: tool.into(),
            policy_id: "policy-01".into(),
            normalized_scope: "scope".into(),
            input: serde_json::json!({"x": 1}),
            policy_decision: PolicyDecision::Allow,
            approval_id: None,
            parent_approval_ref: None,
            preview: "preview".into(),
        }
    }

    #[test]
    fn valid_two_receipt_chain_is_verified() {
        let (signer, genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        install(&db);
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let action_id = uuid::Uuid::now_v7();
        let request = action_request(action_id, "tool.read");
        runtime.prepare(request.clone()).unwrap();
        runtime.mark_started(action_id).unwrap();
        runtime.complete(&request, "succeeded", &"a".repeat(64), None).unwrap();

        let rows = load_rows(&db);
        let result = verify_chain(&rows, &[genesis], None, None);
        assert_eq!(result.status, ChainStatus::Verified, "{:?}", result);
        assert_eq!(result.actual_verified_count, 2);
    }

    #[test]
    fn tampered_hash_breaks_chain() {
        let (signer, genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        install(&db);
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let action_id = uuid::Uuid::now_v7();
        let request = action_request(action_id, "tool.read");
        runtime.prepare(request.clone()).unwrap();
        runtime.mark_started(action_id).unwrap();
        runtime.complete(&request, "succeeded", &"a".repeat(64), None).unwrap();

        let mut rows = load_rows(&db);
        rows[0].receipt_hash = "0".repeat(64);
        let result = verify_chain(&rows, &[genesis], None, None);
        assert_eq!(result.status, ChainStatus::Broken);
    }

    #[test]
    fn pending_action_downgrades_status() {
        let (signer, genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        install(&db);
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let action_id = uuid::Uuid::now_v7();
        runtime.prepare(action_request(action_id, "tool.read")).unwrap();

        let rows = load_rows(&db);
        let result = verify_chain(&rows, &[genesis], None, None);
        assert_eq!(result.status, ChainStatus::Pending);
    }

    #[test]
    fn unknown_key_is_unverified_not_broken() {
        let (signer, _genesis) = make_signer();
        let mut db = Connection::open_in_memory().unwrap();
        install(&db);
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let action_id = uuid::Uuid::now_v7();
        let request = action_request(action_id, "tool.read");
        runtime.prepare(request.clone()).unwrap();
        runtime.mark_started(action_id).unwrap();
        runtime.complete(&request, "succeeded", &"a".repeat(64), None).unwrap();

        let rows = load_rows(&db);
        let result = verify_chain(&rows, &[], None, None);
        assert_eq!(result.status, ChainStatus::Unverified);
    }

    fn load_rows(db: &Connection) -> Vec<ChainRow> {
        let mut stmt = db
            .prepare("SELECT rowid, receipt_id, action_id, receipt_kind, action_status, key_id, receipt_hash, previous_receipt_hash, canonical_envelope FROM receipt_records ORDER BY rowid")
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                let envelope_bytes: Vec<u8> = row.get(8)?;
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    envelope_bytes,
                ))
            })
            .unwrap();
        rows.filter_map(|row| row.ok())
            .map(|(sequence, receipt_id, action_id, receipt_kind, action_status, key_id, receipt_hash, previous_receipt_hash, envelope_bytes)| {
                let envelope: Envelope = serde_json::from_slice(&envelope_bytes).unwrap();
                ChainRow {
                    sequence,
                    receipt_id,
                    action_id,
                    receipt_kind,
                    action_status,
                    key_id,
                    receipt_hash,
                    previous_receipt_hash,
                    approval_id: None,
                    approval_call_hash: None,
                    envelope,
                }
            })
            .collect()
    }
}
