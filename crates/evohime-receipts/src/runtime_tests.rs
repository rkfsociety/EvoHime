use super::*;
use rusqlite::Connection;
use serde_json::Value;
use std::time::Instant;

struct TestSigner;
impl ReceiptSigner for TestSigner {
    fn key_id(&self) -> Result<String, RuntimeError> {
        Ok("test-key".into())
    }
    fn sign_payload_hash(&self, hash: &str) -> Result<String, RuntimeError> {
        Ok(hash.to_string())
    }
}

fn request(policy: PolicyDecision) -> ActionRequest {
    ActionRequest {
        action_id: Uuid::now_v7(),
        task_id: "task-1".into(),
        run_id: "run-1".into(),
        tool_name: "filesystem.write".into(),
        policy_id: "policy-v1".into(),
        normalized_scope: "workspace".into(),
        input: json!({"path":"a.txt","content":"x"}),
        policy_decision: policy,
        approval_id: None,
        parent_approval_ref: None,
        preview: "write a file".into(),
    }
}

#[test]
fn deny_is_terminal_and_never_creates_pre() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let req = request(PolicyDecision::Deny);
    let id = req.action_id;
    assert!(matches!(
        runtime.prepare(req),
        Ok(PrepareOutcome::Refused { .. })
    ));
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
    let approval = match runtime.prepare(req.clone()).unwrap() {
        PrepareOutcome::ApprovalRequired { approval_id, .. } => approval_id,
        _ => panic!(),
    };
    let (created_mono, deadline_mono) = runtime.approval_deadline(approval).unwrap();
    assert_eq!(deadline_mono - created_mono, APPROVAL_TTL_MS);
    assert_eq!(runtime.action(id).unwrap().unwrap().pre_receipt_hash, None);
    runtime.grant_approval(approval).unwrap();
    assert!(matches!(
        runtime.claim_approval(&req, approval),
        Ok(PrepareOutcome::Prepared { .. })
    ));
    assert!(runtime.claim_approval(&req, approval).is_err());
}

#[test]
fn legacy_approval_import_creates_new_pending_id_without_auto_grant() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let req = request(PolicyDecision::ApprovalRequired);
    let outcome = runtime
        .import_legacy_approval("legacy-approval-1", req)
        .unwrap();
    let (approval_id, action_id) = match outcome {
        PrepareOutcome::ApprovalRequired {
            approval_id,
            action_id,
            ..
        } => (approval_id, action_id),
        _ => panic!(),
    };
    let (state, legacy): (String, String) = db
        .query_row(
            "SELECT state,legacy_approval_ref FROM receipt_approval_intents WHERE approval_id=?1",
            [approval_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "pending");
    assert_eq!(legacy, "legacy-approval-1");
    assert_eq!(
        db.query_row(
            "SELECT legacy_approval_ref FROM receipt_actions WHERE action_id=?1",
            [action_id.to_string()],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "legacy-approval-1"
    );
}

#[test]
fn pre_is_durable_before_started_and_post_uses_chain_head() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let req = request(PolicyDecision::Allow);
    let id = req.action_id;
    let pre = runtime.prepare(req.clone()).unwrap();
    assert!(matches!(pre, PrepareOutcome::Prepared { .. }));
    assert_eq!(
        runtime.action(id).unwrap().unwrap().dispatch_state,
        "not_started"
    );
    runtime.mark_started(id).unwrap();
    let post = runtime
        .complete(&req, "succeeded", &"a".repeat(64), None)
        .unwrap();
    assert!(!post.is_empty());
    assert_eq!(runtime.action(id).unwrap().unwrap().state, "succeeded");
}

#[test]
fn post_binding_mismatch_quarantines_without_terminal_success() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
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
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let request = request(PolicyDecision::Allow);
    let id = request.action_id;
    runtime.prepare(request.clone()).unwrap();
    runtime.mark_started(id).unwrap();
    runtime.mark_returned(id).unwrap();
    runtime
        .complete(&request, "succeeded", &"a".repeat(64), None)
        .unwrap();
    assert!(matches!(
        runtime.complete(&request, "succeeded", &"b".repeat(64), None),
        Err(RuntimeError::Code("action_id_conflict"))
    ));
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM receipt_records WHERE action_id=?1",
            [id.to_string()],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}

#[test]
fn authenticated_unquarantine_is_terminal_and_never_dispatchable() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let request = request(PolicyDecision::Allow);
    let id = request.action_id;
    runtime.prepare(request.clone()).unwrap();
    runtime.mark_started(id).unwrap();
    runtime.mark_pending_recovery(id, "unknown").unwrap();
    runtime.quarantine(id, "invariant").unwrap();
    runtime
        .unquarantine(&request, true, "checkpoint-1")
        .unwrap();
    assert_eq!(runtime.action(id).unwrap().unwrap().state, "refused");
    assert!(runtime.mark_started(id).is_err());
    assert!(runtime
        .unquarantine(&request, true, "checkpoint-1")
        .is_err());
}

#[test]
fn read_only_recovery_blocks_every_mutation_and_chain_write_entry_point() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();

    let started = request(PolicyDecision::Allow);
    runtime.prepare(started.clone()).unwrap();
    runtime.mark_started(started.action_id).unwrap();

    let not_started = request(PolicyDecision::Allow);
    runtime.prepare(not_started.clone()).unwrap();

    let approval_req = request(PolicyDecision::ApprovalRequired);
    let approval_id = match runtime.prepare(approval_req.clone()).unwrap() {
        PrepareOutcome::ApprovalRequired { approval_id, .. } => approval_id,
        _ => panic!(),
    };
    runtime.grant_approval(approval_id).unwrap();

    let refuse_req = request(PolicyDecision::ApprovalRequired);
    runtime.prepare(refuse_req.clone()).unwrap();

    let quarantined = request(PolicyDecision::Allow);
    runtime.prepare(quarantined.clone()).unwrap();
    runtime.mark_started(quarantined.action_id).unwrap();
    runtime
        .mark_pending_recovery(quarantined.action_id, "unknown")
        .unwrap();
    runtime
        .quarantine(quarantined.action_id, "invariant")
        .unwrap();

    db.execute(
        "UPDATE receipt_runtime_guard SET phase='read_only_recovery' WHERE id=1",
        [],
    )
    .unwrap();
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();

    assert!(matches!(
        runtime.mark_started(not_started.action_id),
        Err(RuntimeError::Code("pending_recovery"))
    ));
    assert!(matches!(
        runtime.mark_returned(started.action_id),
        Err(RuntimeError::Code("pending_recovery"))
    ));
    assert!(matches!(
        runtime.complete(&started, "succeeded", &"a".repeat(64), None),
        Err(RuntimeError::Code("pending_recovery"))
    ));
    assert!(matches!(
        runtime.claim_approval(&approval_req, approval_id),
        Err(RuntimeError::Code("pending_recovery"))
    ));
    assert!(matches!(
        runtime.refuse(&refuse_req, "approval_expired"),
        Err(RuntimeError::Code("pending_recovery"))
    ));
    assert!(matches!(
        runtime.unquarantine(&quarantined, true, "checkpoint-1"),
        Err(RuntimeError::Code("pending_recovery"))
    ));
    assert!(matches!(
        runtime.prepare(request(PolicyDecision::Allow)),
        Err(RuntimeError::Code("pending_recovery"))
    ));

    assert_eq!(
        runtime
            .action(started.action_id)
            .unwrap()
            .unwrap()
            .dispatch_state,
        "started"
    );
    assert_eq!(
        runtime
            .action(quarantined.action_id)
            .unwrap()
            .unwrap()
            .state,
        "quarantined"
    );
}

#[test]
fn protected_row_is_authenticated_bounded_and_fail_closed() {
    let row = ProtectedActionRow {
        schema_version: 1,
        action_id: Uuid::now_v7().to_string(),
        pre_receipt_hash: "a".repeat(64),
        tool_args_hash: "b".repeat(64),
        result_status: "failed".into(),
        result_hash: "c".repeat(64),
        recovery_code: "external_error".into(),
        created_at_ms: 1,
        key_id: "key-1".into(),
    };
    let key = [7u8; 32];
    let envelope = protect_action_row(&row, &key).unwrap();
    assert!(envelope.len() <= MAX_PROTECTED_ROW_BYTES);
    assert_eq!(unprotect_action_row(&envelope, &key).unwrap(), row);
    let mut tampered = envelope.clone();
    tampered[13] ^= 1;
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
    let row = ProtectedActionRow {
        schema_version: 1,
        action_id: action_id.to_string(),
        pre_receipt_hash: "a".repeat(64),
        tool_args_hash: "b".repeat(64),
        result_status: "failed".into(),
        result_hash: "c".repeat(64),
        recovery_code: "external_error".into(),
        created_at_ms: 1,
        key_id: "old".into(),
    };
    runtime.store_protected_action(&row, &[1u8; 32]).unwrap();
    assert!(runtime
        .rewrap_protected_batch("job-1", "old", "new", 1, 8, |envelope| {
            let mut plain = unprotect_action_row(envelope, &[1u8; 32])?;
            plain.key_id = "new".into();
            protect_action_row(&plain, &[2u8; 32])
        })
        .unwrap());
    assert!(!runtime
        .rewrap_protected_batch("job-1", "old", "new", 1, 8, |envelope| {
            let mut plain = unprotect_action_row(envelope, &[2u8; 32])?;
            plain.key_id = "new".into();
            protect_action_row(&plain, &[2u8; 32])
        })
        .unwrap());
    let rewrapped_key_id = runtime
        .load_protected_action(action_id, &[2u8; 32])
        .unwrap()
        .key_id;
    let old_key_rejected = runtime
        .load_protected_action(action_id, &[1u8; 32])
        .is_err();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM receipt_storage_rotation_audit WHERE job_id='job-1'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(rewrapped_key_id, "new");
    assert!(old_key_rejected);
}

#[test]
fn reconciliation_completion_links_old_and_new_actions_atomically() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
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
    runtime
        .complete_reconciliation(&new, old_id, "succeeded", &"d".repeat(64), None)
        .unwrap();
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
    assert_eq!(
        sampled_read_only(
            "018f0f2a-2222-7222-8222-222222222222",
            "filesystem.read",
            10
        ),
        sampled_read_only(
            "018f0f2a-2222-7222-8222-222222222222",
            "filesystem.read",
            10
        )
    );
    assert!(!sampled_read_only("action", "filesystem.read", 0));
    assert!(sampled_read_only("action", "filesystem.read", 100));
}

#[test]
fn shared_runtime_error_manifest_has_contiguous_alias_ranges() {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../contracts/receipts/v1/version-manifest.json"
    ))
    .unwrap();
    let transport = manifest["transport_runtime_error_codes"]
        .as_object()
        .unwrap();
    let mut transport_codes = transport
        .values()
        .map(|value| value.as_i64().unwrap())
        .collect::<Vec<_>>();
    transport_codes.sort_unstable();
    assert_eq!(transport_codes, (1001_i64..=1013).collect::<Vec<_>>());
    let aliases = manifest["canonical_refusal_aliases"].as_object().unwrap();
    let mut alias_codes = aliases
        .values()
        .map(|value| value.as_i64().unwrap())
        .collect::<Vec<_>>();
    alias_codes.sort_unstable();
    assert_eq!(alias_codes, (2001_i64..=2008).collect::<Vec<_>>());
}

#[test]
fn unsigned_runtime_marker_never_creates_a_receipt_or_chain_head() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let action_id = Uuid::now_v7();
    runtime
        .store_unsigned_runtime_marker(action_id, "signer_unavailable")
        .unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM receipt_records", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM receipt_chain_heads", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT detail_code FROM receipt_runtime_diagnostics WHERE action_id=?1",
            [action_id.to_string()],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "signer_unavailable"
    );
}

#[test]
fn startup_recovery_expires_only_intents_and_never_synthesizes_success() {
    let mut db = Connection::open_in_memory().unwrap();
    install_schema(&db).unwrap();
    let pending = recover_database(&mut db).unwrap();
    assert_eq!(pending, 0);
    let phase: String = db
        .query_row(
            "SELECT phase FROM receipt_runtime_guard WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(phase, "ready");
}

#[test]
fn approval_gc_is_blocked_until_recovery_guard_is_ready() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    ReceiptRuntime::new(&mut db, &signer).unwrap();
    db.execute(
        "UPDATE receipt_runtime_guard SET phase='read_only_recovery' WHERE id=1",
        [],
    )
    .unwrap();
    let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    assert!(matches!(
        runtime.approval_gc(now_ms()),
        Err(RuntimeError::Code("pending_recovery"))
    ));
}

#[test]
fn approval_gc_deletes_only_terminal_intents_past_ttl_and_spares_pending_recovery() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;

    let (claimed_approval, pending_approval, stuck_approval, stuck_action) = {
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        let claimed_approval = match runtime
            .prepare(request(PolicyDecision::ApprovalRequired))
            .unwrap()
        {
            PrepareOutcome::ApprovalRequired { approval_id, .. } => approval_id,
            _ => panic!(),
        };
        let pending_approval = match runtime
            .prepare(request(PolicyDecision::ApprovalRequired))
            .unwrap()
        {
            PrepareOutcome::ApprovalRequired { approval_id, .. } => approval_id,
            _ => panic!(),
        };
        let (stuck_approval, stuck_action) = match runtime
            .prepare(request(PolicyDecision::ApprovalRequired))
            .unwrap()
        {
            PrepareOutcome::ApprovalRequired {
                approval_id,
                action_id,
                ..
            } => (approval_id, action_id),
            _ => panic!(),
        };
        (
            claimed_approval,
            pending_approval,
            stuck_approval,
            stuck_action,
        )
    };

    // Well past the 10-minute TTL cutoff for all three.
    let past = now_ms() - APPROVAL_TTL_MS * 2;
    // Terminal + past TTL + action not stuck: must be deleted.
    db.execute("UPDATE receipt_approval_intents SET state='claimed', expires_at_ms=?1 WHERE approval_id=?2", params![past, claimed_approval.to_string()]).unwrap();
    // Still pending (not terminal): must survive regardless of age.
    db.execute(
        "UPDATE receipt_approval_intents SET expires_at_ms=?1 WHERE approval_id=?2",
        params![past, pending_approval.to_string()],
    )
    .unwrap();
    // Terminal + past TTL, but its action is pending_recovery: must survive until authenticated closure.
    db.execute("UPDATE receipt_approval_intents SET state='expired', expires_at_ms=?1 WHERE approval_id=?2", params![past, stuck_approval.to_string()]).unwrap();
    db.execute(
        "UPDATE receipt_actions SET state='pending_recovery' WHERE action_id=?1",
        params![stuck_action.to_string()],
    )
    .unwrap();

    let runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let deleted = runtime.approval_gc(now_ms()).unwrap();
    assert_eq!(deleted, 1);

    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM receipt_approval_intents WHERE approval_id=?1",
            [claimed_approval.to_string()],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM receipt_approval_intents WHERE approval_id=?1",
            [pending_approval.to_string()],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM receipt_approval_intents WHERE approval_id=?1",
            [stuck_approval.to_string()],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT value FROM receipt_runtime_metrics WHERE metric='approval_gc_deleted_count'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn startup_recovery_quarantines_started_without_pre_and_preserves_unknown_result() {
    let mut db = Connection::open_in_memory().unwrap();
    install_schema(&db).unwrap();
    db.execute("INSERT INTO receipt_actions(schema_version,action_id,task_id,run_id,tool_name,normalized_scope,fingerprint_input_version,tool_args_hash,policy_id,policy_decision,state,dispatch_state) VALUES(1,?1,'task','run','shell.execute','workspace',1,?2,'policy','allow','prepared','started')", params![Uuid::now_v7().to_string(), "a".repeat(64)]).unwrap();
    let action_id = db
        .query_row("SELECT action_id FROM receipt_actions LIMIT 1", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap();
    assert!(matches!(
        recover_database(&mut db),
        Err(RuntimeError::Code("schema_violation"))
    ));
    let (state, code): (String, Option<String>) = db
        .query_row(
            "SELECT state,recovery_code FROM receipt_actions WHERE action_id=?1",
            [action_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "quarantined");
    assert_eq!(code.as_deref(), Some("unknown"));
    assert_eq!(
        db.query_row(
            "SELECT phase FROM receipt_runtime_guard WHERE id=1",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "read_only_recovery"
    );
}

#[test]
fn startup_recovery_rebuilds_action_index_from_durable_terminal_post() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let request = request(PolicyDecision::Allow);
    let id = request.action_id;
    {
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        runtime.prepare(request.clone()).unwrap();
        runtime.mark_started(id).unwrap();
        runtime.mark_returned(id).unwrap();
        runtime
            .complete(&request, "failed", &"a".repeat(64), Some("tool_error"))
            .unwrap();
    }
    db.execute(
        "UPDATE receipt_actions SET state='prepared',terminal_receipt_hash=NULL WHERE action_id=?1",
        [id.to_string()],
    )
    .unwrap();
    assert_eq!(recover_database(&mut db).unwrap(), 0);
    let (state, terminal): (String, Option<String>) = db
        .query_row(
            "SELECT state,terminal_receipt_hash FROM receipt_actions WHERE action_id=?1",
            [id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
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
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        runtime.prepare(request.clone()).unwrap();
        runtime.mark_started(id).unwrap();
        runtime.mark_returned(id).unwrap();
        runtime
            .complete(&request, "succeeded", &"a".repeat(64), None)
            .unwrap();
    }
    db.execute(
        "DELETE FROM receipt_actions WHERE action_id=?1",
        [id.to_string()],
    )
    .unwrap();
    assert!(matches!(
        recover_database(&mut db),
        Err(RuntimeError::Code("schema_violation"))
    ));
    assert_eq!(
        db.query_row(
            "SELECT phase FROM receipt_runtime_guard WHERE id=1",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "read_only_recovery"
    );
    assert_eq!(
        db.query_row(
            "SELECT detail_code FROM receipt_runtime_diagnostics LIMIT 1",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "orphan_terminal_receipt"
    );
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
            let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
            runtime.prepare(request.clone()).unwrap();
            if started || post {
                runtime.mark_started(id).unwrap();
                runtime.mark_returned(id).unwrap();
                if post {
                    runtime
                        .complete(&request, "succeeded", &"a".repeat(64), None)
                        .unwrap();
                }
            }
        }
        db.execute("UPDATE receipt_actions SET state='prepared',dispatch_state=?2,terminal_receipt_hash=NULL,pre_receipt_hash=CASE WHEN ?3 THEN pre_receipt_hash ELSE NULL END WHERE action_id=?1", params![id.to_string(), if started { "started" } else { "not_started" }, pre]).unwrap();
        if !pre {
            db.execute(
                "DELETE FROM receipt_records WHERE action_id=?1 AND receipt_kind='pre_action'",
                [id.to_string()],
            )
            .unwrap();
        }
        let recovery = recover_database(&mut db);
        assert_eq!(
            recovery.is_err(),
            expect_safe_mode,
            "case pre={pre} started={started} post={post}"
        );
        if expect_safe_mode {
            assert_eq!(
                db.query_row(
                    "SELECT phase FROM receipt_runtime_guard WHERE id=1",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
                "read_only_recovery"
            );
        } else {
            let state: String = db
                .query_row(
                    "SELECT state FROM receipt_actions WHERE action_id=?1",
                    [id.to_string()],
                    |row| row.get(0),
                )
                .unwrap();
            if expect_pending {
                assert_eq!(state, "pending_recovery");
            } else if post {
                assert_eq!(state, "succeeded");
            } else {
                assert_eq!(state, "pending_recovery");
            }
        }
    }
}

#[test]
fn parallel_file_backed_prepares_keep_a_single_verifiable_chain_head() {
    let path =
        std::env::temp_dir().join(format!("evohime-receipt-concurrency-{}.db", Uuid::now_v7()));
    let connection = Connection::open(&path).unwrap();
    install_schema(&connection).unwrap();
    drop(connection);
    let first = request(PolicyDecision::Allow);
    let second = request(PolicyDecision::Allow);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut db = Connection::open(&path).unwrap();
            let signer = TestSigner;
            let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
            assert!(matches!(
                runtime.prepare(first),
                Ok(PrepareOutcome::Prepared { .. })
            ));
        });
        scope.spawn(|| {
            let mut db = Connection::open(&path).unwrap();
            let signer = TestSigner;
            let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
            assert!(matches!(
                runtime.prepare(second),
                Ok(PrepareOutcome::Prepared { .. })
            ));
        });
    });
    let db = Connection::open(&path).unwrap();
    let records: i64 = db
        .query_row("SELECT COUNT(*) FROM receipt_records", [], |row| row.get(0))
        .unwrap();
    let heads: i64 = db
        .query_row("SELECT COUNT(*) FROM receipt_chain_heads", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(records, 2);
    assert_eq!(heads, 1);
    let head: String = db
        .query_row(
            "SELECT receipt_hash FROM receipt_chain_heads WHERE key_id='test-key'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let last: String = db.query_row("SELECT receipt_hash FROM receipt_records WHERE key_id='test-key' ORDER BY rowid DESC LIMIT 1", [], |row| row.get(0)).unwrap();
    assert_eq!(head, last);
    let _ = std::fs::remove_file(path);
}

#[test]
fn secret_like_preview_text_is_redacted_before_truncation() {
    let preview = bounded_preview(
        "set API_KEY=abc123 for the request; password: hunter2 stays out of receipts",
    );
    assert!(!preview.contains("abc123"));
    assert!(!preview.contains("hunter2"));
    assert!(preview.contains("[REDACTED]"));
}

#[test]
fn plain_preview_text_is_untouched() {
    assert_eq!(
        bounded_preview("write a short readme section"),
        "write a short readme section"
    );
}

#[test]
fn claim_uses_monotonic_deadline_not_wall_clock_within_one_boot() {
    let path =
        std::env::temp_dir().join(format!("evohime-receipt-monotonic-{}.db", Uuid::now_v7()));
    let req = request(PolicyDecision::ApprovalRequired);
    let approval = {
        let mut db = Connection::open(&path).unwrap();
        let signer = TestSigner;
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        match runtime.prepare(req.clone()).unwrap() {
            PrepareOutcome::ApprovalRequired { approval_id, .. } => approval_id,
            _ => panic!(),
        }
    };
    {
        // Simulate a wall clock that already looks expired while the
        // monotonic deadline (same boot) has not passed.
        let side = Connection::open(&path).unwrap();
        side.execute("UPDATE receipt_approval_intents SET state='granted', expires_at_ms=0 WHERE approval_id=?1", [approval.to_string()]).unwrap();
    }
    let mut db = Connection::open(&path).unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let bound = ActionRequest {
        approval_id: Some(approval),
        ..req
    };
    // Claim must still succeed: wall clock is not used for authorization.
    assert!(matches!(
        runtime.claim_approval(&bound, approval),
        Ok(PrepareOutcome::Prepared { .. })
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn claim_expires_once_monotonic_deadline_has_passed_even_with_future_wall_clock() {
    let path = std::env::temp_dir().join(format!(
        "evohime-receipt-monotonic-expired-{}.db",
        Uuid::now_v7()
    ));
    let req = request(PolicyDecision::ApprovalRequired);
    let approval = {
        let mut db = Connection::open(&path).unwrap();
        let signer = TestSigner;
        let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
        match runtime.prepare(req.clone()).unwrap() {
            PrepareOutcome::ApprovalRequired { approval_id, .. } => approval_id,
            _ => panic!(),
        }
    };
    {
        let side = Connection::open(&path).unwrap();
        side.execute("UPDATE receipt_approval_intents SET state='granted', expires_at_ms=99999999999999, deadline_monotonic_ms=-1 WHERE approval_id=?1", [approval.to_string()]).unwrap();
    }
    let mut db = Connection::open(&path).unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let bound = ActionRequest {
        approval_id: Some(approval),
        ..req
    };
    assert!(matches!(
        runtime.claim_approval(&bound, approval),
        Err(RuntimeError::Code("approval_expired"))
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn claim_checked_rejects_when_current_policy_denies() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let req = request(PolicyDecision::ApprovalRequired);
    let approval = match runtime.prepare(req.clone()).unwrap() {
        PrepareOutcome::ApprovalRequired { approval_id, .. } => approval_id,
        _ => panic!(),
    };
    runtime.grant_approval(approval).unwrap();
    let bound = ActionRequest {
        approval_id: Some(approval),
        ..req
    };
    assert!(matches!(
        runtime.claim_approval_checked(&bound, approval, |_| false),
        Err(RuntimeError::Code("policy_denied"))
    ));
    // Policy denial is durable and terminal: the refusal is signed and
    // the approval cannot be replayed under a different policy decision.
    assert!(matches!(
        runtime.claim_approval(&bound, approval),
        Err(RuntimeError::Code("approval_stale"))
    ));
}

#[test]
fn sustained_writer_contention_surfaces_chain_conflict_after_the_retry_budget() {
    let path = std::env::temp_dir().join(format!("evohime-receipt-busy-{}.db", Uuid::now_v7()));
    let mut db = Connection::open(&path).unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let blocker = Connection::open(&path).unwrap();
    blocker.busy_timeout(Duration::from_secs(5)).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    let result = runtime.prepare(request(PolicyDecision::Allow));
    assert!(matches!(result, Err(RuntimeError::Code("chain_conflict"))));
    // The full 0/10/50/250ms schedule must have actually elapsed.
    assert!(started.elapsed() >= Duration::from_millis(300));
    blocker.execute_batch("ROLLBACK").unwrap();
    drop(blocker);
    // Once the writer releases the lock, the connection recovers and a
    // fresh append succeeds normally (busy_timeout was restored, not
    // left at zero after the failed attempt).
    assert!(matches!(
        runtime.prepare(request(PolicyDecision::Allow)),
        Ok(PrepareOutcome::Prepared { .. })
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn migrate_legacy_approvals_is_idempotent_per_version_and_ref() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let record = (
        "legacy-42".to_string(),
        request(PolicyDecision::ApprovalRequired),
    );
    let first = runtime
        .migrate_legacy_approvals(1, vec![record.clone()])
        .unwrap();
    assert_eq!(first.len(), 1);
    // Re-running the same batch (e.g. a retried startup pass) must not
    // create a second pending approval for the same legacy record.
    let mut second_record = record.clone();
    second_record.1.action_id = Uuid::now_v7();
    let second = runtime
        .migrate_legacy_approvals(1, vec![second_record])
        .unwrap();
    assert!(second.is_empty());
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM receipt_actions WHERE legacy_approval_ref='1:legacy-42'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn load_protected_action_with_fallback_tries_new_key_then_old_key() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let old_key = [7u8; 32];
    let new_key = [9u8; 32];
    let req = request(PolicyDecision::Allow);
    let action_id = req.action_id;
    runtime.prepare(req).unwrap();
    let row = ProtectedActionRow {
        schema_version: 1,
        action_id: action_id.to_string(),
        pre_receipt_hash: "a".repeat(64),
        tool_args_hash: "b".repeat(64),
        result_status: "failed".into(),
        result_hash: "c".repeat(64),
        recovery_code: "external_error".into(),
        created_at_ms: now_ms(),
        key_id: "old-key".into(),
    };
    // Row was written under the old key and has not been rewrapped yet.
    runtime.store_protected_action(&row, &old_key).unwrap();
    let loaded = runtime
        .load_protected_action_with_fallback(action_id, &new_key, &old_key)
        .unwrap();
    assert_eq!(loaded.action_id, action_id.to_string());
    // A rewrapped row must be read via the new key without falling back.
    runtime.store_protected_action(&row, &new_key).unwrap();
    let loaded_new = runtime
        .load_protected_action_with_fallback(action_id, &new_key, &old_key)
        .unwrap();
    assert_eq!(loaded_new.action_id, action_id.to_string());
}

#[test]
fn compact_chain_signs_a_checkpoint_and_deletes_the_prefix() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    for _ in 0..4 {
        let req = request(PolicyDecision::Allow);
        let id = req.action_id;
        runtime.prepare(req.clone()).unwrap();
        runtime.mark_started(id).unwrap();
        runtime
            .complete(&req, "succeeded", &"a".repeat(64), None)
            .unwrap();
    }
    // 4 actions x (pre + terminal) = 8 rows; cutoff 7 deletes the first
    // 6, retaining the last action's pre/terminal pair.
    let checkpoint = runtime.compact_chain("test-key", 7).unwrap();
    assert_eq!(checkpoint.key_id, "test-key");
    assert_eq!(checkpoint.cutoff_sequence, 7);
    assert_eq!(checkpoint.status, "active");

    let remaining: i64 = db
        .query_row("SELECT COUNT(*) FROM receipt_records", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        remaining, 2,
        "rows before the cutoff are deleted, cutoff row and after survive"
    );
    let stored: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM receipt_checkpoints WHERE status='active'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, 1);
}

#[test]
fn compact_chain_refuses_to_delete_a_pending_action_prefix() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    // First action stays pending (pre only, no terminal).
    let pending = request(PolicyDecision::Allow);
    runtime.prepare(pending.clone()).unwrap();
    // Second action completes normally, advancing the chain head.
    let done = request(PolicyDecision::Allow);
    let done_id = done.action_id;
    runtime.prepare(done.clone()).unwrap();
    runtime.mark_started(done_id).unwrap();
    runtime
        .complete(&done, "succeeded", &"a".repeat(64), None)
        .unwrap();

    let result = runtime.compact_chain("test-key", 3);
    assert!(
        matches!(
            result,
            Err(RuntimeError::Code("checkpoint_blocked_by_pending"))
        ),
        "{result:?}"
    );
    let remaining: i64 = db
        .query_row("SELECT COUNT(*) FROM receipt_records", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        remaining, 3,
        "nothing is deleted when the prefix is blocked"
    );
}

#[test]
fn retention_candidates_skip_a_key_entirely_within_bounds() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let req = request(PolicyDecision::Allow);
    let id = req.action_id;
    runtime.prepare(req.clone()).unwrap();
    runtime.mark_started(id).unwrap();
    runtime
        .complete(&req, "succeeded", &"a".repeat(64), None)
        .unwrap();

    let candidates = runtime.retention_candidates(now_ms()).unwrap();
    assert!(
        candidates.is_empty(),
        "a fresh, small chain has nothing to compact yet"
    );
}

#[test]
fn retention_candidates_finds_a_cutoff_once_rows_are_old_enough() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    for _ in 0..3 {
        let req = request(PolicyDecision::Allow);
        let id = req.action_id;
        runtime.prepare(req.clone()).unwrap();
        runtime.mark_started(id).unwrap();
        runtime
            .complete(&req, "succeeded", &"a".repeat(64), None)
            .unwrap();
    }
    let far_future = now_ms() + 91 * 24 * 60 * 60 * 1000;
    let candidates = runtime.retention_candidates(far_future).unwrap();
    assert_eq!(candidates.len(), 1);
    let (key_id, cutoff) = &candidates[0];
    assert_eq!(key_id, "test-key");
    assert!(*cutoff > 0);
}

#[test]
fn request_commit_receipt_is_signed_and_idempotent_on_the_same_chain() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();
    let first = runtime
        .append_model_request_receipt(ModelRequestReceiptInput {
            request_id: "request-1",
            logical_request_id: "logical-1",
            ledger_id: "ledger-1",
            attempt: 1,
            provider: "mock",
            model: "model",
            envelope_hash: &"a".repeat(64),
            context_projection_hash: &"b".repeat(64),
            route_snapshot_hash: &"c".repeat(64),
            policy_snapshot_hash: &"d".repeat(64),
        })
        .unwrap();
    let second = runtime
        .append_model_request_receipt(ModelRequestReceiptInput {
            request_id: "request-1",
            logical_request_id: "logical-1",
            ledger_id: "ledger-1",
            attempt: 1,
            provider: "mock",
            model: "model",
            envelope_hash: &"a".repeat(64),
            context_projection_hash: &"b".repeat(64),
            route_snapshot_hash: &"c".repeat(64),
            policy_snapshot_hash: &"d".repeat(64),
        })
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.previous_receipt_hash, None);
    assert_eq!(
        db.query_row("SELECT receipt_kind FROM receipt_records", [], |row| row
            .get::<_, String>(
            0
        ))
        .unwrap(),
        "request_commit"
    );
    assert_eq!(
        db.query_row(
            "SELECT receipt_hash FROM receipt_chain_heads WHERE key_id='test-key'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        first.receipt_hash
    );
}

#[test]
fn request_commit_receipt_allows_provider_selected_model() {
    let mut db = Connection::open_in_memory().unwrap();
    let signer = TestSigner;
    let mut runtime = ReceiptRuntime::new(&mut db, &signer).unwrap();

    let receipt = runtime
        .append_model_request_receipt(ModelRequestReceiptInput {
            request_id: "request-provider-selected-model",
            logical_request_id: "logical-provider-selected-model",
            ledger_id: "ledger-provider-selected-model",
            attempt: 1,
            provider: "literouter",
            model: "",
            envelope_hash: &"a".repeat(64),
            context_projection_hash: &"b".repeat(64),
            route_snapshot_hash: &"c".repeat(64),
            policy_snapshot_hash: &"d".repeat(64),
        })
        .expect("provider-selected model is a valid request receipt");

    assert!(!receipt.receipt_hash.is_empty());
    let model: String = db
            .query_row(
                "SELECT json_extract(canonical_payload, '$.model') FROM receipt_records WHERE request_id=?1",
                ["request-provider-selected-model"],
                |row| row.get(0),
            )
            .unwrap();
    assert!(model.is_empty());
}
