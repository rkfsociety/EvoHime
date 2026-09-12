use super::*;

#[tokio::test]
async fn serves_bounded_workspace_list_and_file_read_over_ipc() {
    let root =
        std::env::temp_dir().join(format!("evohime-ipc-workspace-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("src directory");
    std::fs::write(root.join("README.md"), "hello from workspace").expect("readme");
    let journal_path =
        std::env::temp_dir().join(format!("evohime-ipc-workspace-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&journal_path);
    let bridge = IpcBridge::new(EventJournal::open(&journal_path).expect("journal opens"));
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);

    let list = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "workspace-list".into(),
        client_id: "test-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::ListWorkspace(
            generated::ListWorkspace {
                workspace_path: root.display().to_string(),
                relative_path: ".".into(),
                max_entries: 10,
            },
        )),
    };
    transport::write_frame(&mut client, &list.encode_to_vec())
        .await
        .expect("list writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("list serves");
    let response = generated::EventEnvelope::decode(
        transport::read_frame(&mut client)
            .await
            .expect("list reads")
            .as_slice(),
    )
    .expect("list event decodes");
    assert_eq!(response.event_type, "workspace.list");
    let listing: serde_json::Value = serde_json::from_slice(&response.payload).expect("list json");
    assert_eq!(listing["entries"][0]["name"], "src");

    let read = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "workspace-read".into(),
        client_id: "test-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::ReadWorkspaceFile(
            generated::ReadWorkspaceFile {
                workspace_path: root.display().to_string(),
                relative_path: "README.md".into(),
                max_bytes: 100,
            },
        )),
    };
    transport::write_frame(&mut client, &read.encode_to_vec())
        .await
        .expect("read writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("read serves");
    let response = generated::EventEnvelope::decode(
        transport::read_frame(&mut client)
            .await
            .expect("read response")
            .as_slice(),
    )
    .expect("read event decodes");
    assert_eq!(response.event_type, "workspace.file");
    let file: serde_json::Value = serde_json::from_slice(&response.payload).expect("file json");
    assert_eq!(file["content"], "hello from workspace");
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_file(journal_path);
}

#[tokio::test]
async fn terminal_requires_approval_and_denied_retry_does_not_execute() {
    let root =
        std::env::temp_dir().join(format!("evohime-ipc-terminal-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("terminal root");
    let data_root =
        std::env::temp_dir().join(format!("evohime-ipc-terminal-data-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_root);
    std::fs::create_dir_all(&data_root).expect("terminal data root");
    let journal_path = data_root.join("events.db");
    let _ = std::fs::remove_file(&journal_path);
    let receipt_keys = ReceiptKeyManager::new(&data_root);
    receipt_keys.initialize().expect("receipt keys initialize");
    let journal = EventJournal::open(&journal_path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let tools = Arc::new(ToolRegistry::bootstrap());
    let bridge = IpcBridge::with_coordinator_and_approvals(
        journal,
        coordinator,
        ApprovalCoordinator::default(),
        tools,
        None,
        None,
    );
    let task_id = uuid::Uuid::new_v4().to_string();
    let make_terminal = |approval_id: String| generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "terminal-request".into(),
        client_id: "test-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::TerminalExecute(
            generated::TerminalExecute {
                task_id: task_id.clone(),
                workspace_path: root.display().to_string(),
                program: "git".into(),
                args: vec!["status".into()],
                cwd: String::new(),
                timeout_ms: 5_000,
                approval_id,
            },
        )),
    };
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
        .await
        .expect("terminal writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("approval serves");
    let approval = generated::EventEnvelope::decode(
        transport::read_frame(&mut client)
            .await
            .expect("approval reads")
            .as_slice(),
    )
    .expect("approval decodes");
    assert_eq!(approval.event_type, "approval.required");
    let approval_json =
        serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
    assert_eq!(approval_json["preview"]["kind"], "command");
    assert_eq!(approval_json["preview"]["command"], "git status");
    let approval_id = approval_json["approval_id"]
        .as_str()
        .expect("approval id")
        .to_string();

    let resolve = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "resolve-terminal".into(),
        client_id: "test-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::ResolveApproval(
            generated::ResolveApproval {
                approval_id: approval_id.clone(),
                granted: false,
                idempotency_key: String::new(),
                rejection_reason: String::new(),
                cancel: false,
            },
        )),
    };
    transport::write_frame(&mut client, &resolve.encode_to_vec())
        .await
        .expect("resolve writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("resolve serves");

    // План 08-4 acceptance: a denied approval publishes a typed
    // ApprovalDecision/Denied ledger event linked to the receipts
    // approval intent's own action_id — this is the "reject" arm of
    // "approval approve/reject/expiry".
    {
        let journal_handle = bridge.journal();
        let database = journal_handle.database().lock().await;
        let (decision_state, body_payload): (String, Vec<u8>) = database
            .connection()
            .query_row(
                "SELECT state_after, payload FROM events
                       WHERE event_type = 'ledger.approval_decision'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ledger.approval_decision row exists");
        assert_eq!(decision_state, "denied");
        let decision_event: execution_ledger::ExecutionEventV1 =
            serde_json::from_slice(&body_payload).expect("decision event decodes");
        let execution_ledger::ExecutionEventBody::ApprovalDecision {
            approval_intent_id,
            decision,
            ..
        } = decision_event.body
        else {
            panic!(
                "expected ApprovalDecision body, got {:?}",
                decision_event.body
            );
        };
        assert_eq!(approval_intent_id, approval_id);
        assert_eq!(decision, execution_ledger::ApprovalOutcome::Rejected);
    }

    transport::write_frame(&mut client, &make_terminal(approval_id).encode_to_vec())
        .await
        .expect("retry writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("retry serves");
    let result = generated::EventEnvelope::decode(
        transport::read_frame(&mut client)
            .await
            .expect("result reads")
            .as_slice(),
    )
    .expect("result decodes");
    assert_eq!(result.event_type, "terminal.result");
    let result_json: serde_json::Value =
        serde_json::from_slice(&result.payload).expect("result json");
    assert_eq!(result_json["ok"], false);
    assert_eq!(result_json["error_code"], "approval_denied");
    assert!(result_json.get("error").is_none());
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(data_root);
}

/// План 08-4 acceptance: the third arm of "approval approve/reject/
/// expiry". A retry that arrives after the approval window closed must
/// be refused by `grant_approval`'s own deadline check (not by a new
/// check invented here) and publish a typed `ApprovalDecision/Expired`
/// ledger event before the error propagates. The deadline is forced
/// into the past directly in `receipt_approval_intents` — waiting out
/// the real 10-minute TTL is not a workable test.
#[tokio::test]
async fn expired_approval_publishes_ledger_decision_and_refuses_the_retry() {
    let root = std::env::temp_dir().join(format!(
        "evohime-ipc-terminal-expiry-root-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("terminal root");
    let data_root = std::env::temp_dir().join(format!(
        "evohime-ipc-terminal-expiry-data-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&data_root);
    std::fs::create_dir_all(&data_root).expect("terminal data root");
    let journal_path = data_root.join("events.db");
    let _ = std::fs::remove_file(&journal_path);
    let receipt_keys = ReceiptKeyManager::new(&data_root);
    receipt_keys.initialize().expect("receipt keys initialize");
    let journal = EventJournal::open(&journal_path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let tools = Arc::new(ToolRegistry::bootstrap());
    let bridge = IpcBridge::with_coordinator_and_approvals(
        journal.clone(),
        coordinator,
        ApprovalCoordinator::default(),
        tools,
        None,
        None,
    );
    let task_id = uuid::Uuid::new_v4().to_string();
    let make_terminal = |approval_id: String| generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "terminal-request".into(),
        client_id: "test-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::TerminalExecute(
            generated::TerminalExecute {
                task_id: task_id.clone(),
                workspace_path: root.display().to_string(),
                program: "git".into(),
                args: vec!["status".into()],
                cwd: String::new(),
                timeout_ms: 5_000,
                approval_id,
            },
        )),
    };
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
        .await
        .expect("terminal writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("approval serves");
    let approval = generated::EventEnvelope::decode(
        transport::read_frame(&mut client)
            .await
            .expect("approval reads")
            .as_slice(),
    )
    .expect("approval decodes");
    let approval_json =
        serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
    let approval_id = approval_json["approval_id"]
        .as_str()
        .expect("approval id")
        .to_string();

    // Force the approval window into the past — same-process retries
    // hit the monotonic-clock branch of `grant_approval`'s deadline
    // check, so backdating `deadline_monotonic_ms` is what actually
    // exercises it (backdating `expires_at_ms` alone would not, since
    // the boot id matches).
    {
        let database = journal.database().lock().await;
        let changed = database
                .connection()
                .execute(
                    "UPDATE receipt_approval_intents SET deadline_monotonic_ms = 0 WHERE approval_id = ?1",
                    [&approval_id],
                )
                .expect("deadline backdates");
        assert_eq!(changed, 1, "the approval intent row must exist");
    }

    transport::write_frame(
        &mut client,
        &make_terminal(approval_id.clone()).encode_to_vec(),
    )
    .await
    .expect("retry writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect_err("an expired approval must refuse the retry");

    let journal_handle = bridge.journal();
    let database = journal_handle.database().lock().await;
    let (decision_state, body_payload): (String, Vec<u8>) = database
        .connection()
        .query_row(
            "SELECT state_after, payload FROM events
                   WHERE event_type = 'ledger.approval_decision'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("ledger.approval_decision row exists");
    assert_eq!(decision_state, "timed_out");
    let decision_event: execution_ledger::ExecutionEventV1 =
        serde_json::from_slice(&body_payload).expect("decision event decodes");
    let execution_ledger::ExecutionEventBody::ApprovalDecision {
        approval_intent_id,
        decision,
        ..
    } = decision_event.body
    else {
        panic!(
            "expected ApprovalDecision body, got {:?}",
            decision_event.body
        );
    };
    assert_eq!(approval_intent_id, approval_id);
    assert_eq!(decision, execution_ledger::ApprovalOutcome::Expired);
    drop(database);

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(data_root);
}

/// План 08-4 acceptance: "action → tool call → observation → successful
/// typed receipt linked to signed receipts_v1". A real terminal
/// execution, approved and run through `dispatch_terminal_execute`, must
/// leave a typed `ledger.tool_call` (Running) followed by a typed
/// `ledger.tool_receipt` (Succeeded) under the same `action_id` — and
/// that receipt event's `receipt_hash` must resolve to an actual signed
/// row in `receipt_records`, not just a plausible-looking string.
#[test]
fn approved_terminal_execute_links_ledger_receipt_to_signed_receipts_v1() {
    std::thread::Builder::new()
        .name("evohime-ipc-terminal-linkage-test".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime builds")
                .block_on(
                    approved_terminal_execute_links_ledger_receipt_to_signed_receipts_v1_inner(),
                );
        })
        .expect("test thread starts")
        .join()
        .expect("test thread completes");
}

async fn approved_terminal_execute_links_ledger_receipt_to_signed_receipts_v1_inner() {
    let root = std::env::temp_dir().join(format!(
        "evohime-ipc-terminal-linkage-root-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("terminal root");
    std::process::Command::new("git")
        .arg("init")
        .arg(&root)
        .output()
        .expect("git init runs");
    let data_root = std::env::temp_dir().join(format!(
        "evohime-ipc-terminal-linkage-data-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&data_root);
    std::fs::create_dir_all(&data_root).expect("terminal data root");
    let journal_path = data_root.join("events.db");
    let _ = std::fs::remove_file(&journal_path);
    let receipt_keys = ReceiptKeyManager::new(&data_root);
    receipt_keys.initialize().expect("receipt keys initialize");
    let journal = EventJournal::open(&journal_path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let tools = Arc::new(ToolRegistry::bootstrap());
    let bridge = IpcBridge::with_coordinator_and_approvals(
        journal.clone(),
        coordinator,
        ApprovalCoordinator::default(),
        tools,
        None,
        None,
    );
    let task_id = uuid::Uuid::new_v4().to_string();
    let make_terminal = |approval_id: String| generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "terminal-request".into(),
        client_id: "test-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::TerminalExecute(
            generated::TerminalExecute {
                task_id: task_id.clone(),
                workspace_path: root.display().to_string(),
                program: "git".into(),
                args: vec!["status".into()],
                cwd: String::new(),
                timeout_ms: 5_000,
                approval_id,
            },
        )),
    };
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
        .await
        .expect("terminal writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("approval serves");
    let approval = generated::EventEnvelope::decode(
        transport::read_frame(&mut client)
            .await
            .expect("approval reads")
            .as_slice(),
    )
    .expect("approval decodes");
    let approval_json =
        serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
    let approval_id = approval_json["approval_id"]
        .as_str()
        .expect("approval id")
        .to_string();

    let resolve = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "resolve-terminal".into(),
        client_id: "test-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::ResolveApproval(
            generated::ResolveApproval {
                approval_id: approval_id.clone(),
                granted: true,
                idempotency_key: String::new(),
                rejection_reason: String::new(),
                cancel: false,
            },
        )),
    };
    transport::write_frame(&mut client, &resolve.encode_to_vec())
        .await
        .expect("resolve writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("resolve serves");

    // План 08-4 acceptance: a granted approval publishes a typed
    // ApprovalDecision/Approved ledger event — the "approve" arm of
    // "approval approve/reject/expiry" — before the retried execution
    // publishes its own ToolCall/ToolReceipt pair below.
    {
        let database = journal.database().lock().await;
        let (decision_state, body_payload): (String, Vec<u8>) = database
            .connection()
            .query_row(
                "SELECT state_after, payload FROM events
                       WHERE event_type = 'ledger.approval_decision'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ledger.approval_decision row exists");
        assert_eq!(decision_state, "running");
        let decision_event: execution_ledger::ExecutionEventV1 =
            serde_json::from_slice(&body_payload).expect("decision event decodes");
        let execution_ledger::ExecutionEventBody::ApprovalDecision {
            approval_intent_id,
            decision,
            ..
        } = decision_event.body
        else {
            panic!(
                "expected ApprovalDecision body, got {:?}",
                decision_event.body
            );
        };
        assert_eq!(approval_intent_id, approval_id);
        assert_eq!(decision, execution_ledger::ApprovalOutcome::Approved);
    }

    transport::write_frame(&mut client, &make_terminal(approval_id).encode_to_vec())
        .await
        .expect("retry writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("retry serves");
    let result = generated::EventEnvelope::decode(
        transport::read_frame(&mut client)
            .await
            .expect("result reads")
            .as_slice(),
    )
    .expect("result decodes");
    assert_eq!(result.event_type, "terminal.result");
    let result_json: serde_json::Value =
        serde_json::from_slice(&result.payload).expect("result json");
    assert_eq!(
        result_json["ok"], true,
        "git status in a real repo must succeed: {result_json}"
    );

    let database = journal.database().lock().await;
    let (tool_call_action_id, tool_call_state): (String, String) = database
        .connection()
        .query_row(
            "SELECT action_id, state_after FROM events WHERE event_type = 'ledger.tool_call'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("ledger.tool_call row exists");
    assert_eq!(tool_call_state, "running");

    // The "observation" link of "action → tool call → observation →
    // receipt" — must exist under the same action_id, between the call
    // and the receipt.
    let (observation_action_id, observation_payload): (String, Vec<u8>) = database
        .connection()
        .query_row(
            "SELECT action_id, payload FROM events WHERE event_type = 'ledger.observation'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("ledger.observation row exists");
    assert_eq!(observation_action_id, tool_call_action_id);
    let observation_event: execution_ledger::ExecutionEventV1 =
        serde_json::from_slice(&observation_payload).expect("observation event decodes");
    assert!(matches!(
        observation_event.body,
        execution_ledger::ExecutionEventBody::Observation { .. }
    ));

    let (receipt_action_id, receipt_state, receipt_payload): (String, String, Vec<u8>) = database
        .connection()
        .query_row(
            "SELECT action_id, state_after, payload FROM events
                       WHERE event_type = 'ledger.tool_receipt'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("ledger.tool_receipt row exists");
    assert_eq!(receipt_state, "succeeded");
    assert_eq!(
        receipt_action_id, tool_call_action_id,
        "tool_call and tool_receipt must share the same action_id"
    );
    let receipt_event: execution_ledger::ExecutionEventV1 =
        serde_json::from_slice(&receipt_payload).expect("receipt event decodes");
    let execution_ledger::ExecutionEventBody::ToolReceipt {
        receipt_action_id: body_action_id,
        receipt_hash,
    } = receipt_event.body
    else {
        panic!("expected ToolReceipt body, got {:?}", receipt_event.body);
    };
    assert_eq!(body_action_id, receipt_action_id);

    // The linkage is only real if that hash resolves to an actual signed
    // row — not merely a string that looks like one.
    let signed_action_id: String = database
        .connection()
        .query_row(
            "SELECT action_id FROM receipt_records WHERE receipt_hash = ?1",
            [&receipt_hash],
            |row| row.get(0),
        )
        .expect("receipt_hash resolves to a real receipt_records row");
    assert_eq!(signed_action_id, receipt_action_id);
    drop(database);

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(data_root);
}

#[tokio::test]
async fn reconciliation_command_executes_only_new_read_only_action() {
    let root =
        std::env::temp_dir().join(format!("evohime-ipc-reconcile-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("reconcile root");
    std::fs::write(root.join("observed.txt"), "observed state\n").expect("observed file");
    let data_root =
        std::env::temp_dir().join(format!("evohime-ipc-reconcile-data-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_root);
    std::fs::create_dir_all(&data_root).expect("reconcile data root");
    let journal_path = data_root.join("events.db");
    let keys = ReceiptKeyManager::new(&data_root);
    keys.initialize().expect("keys initialize");
    let journal = EventJournal::open(&journal_path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator_and_approvals(
        journal.clone(),
        coordinator,
        ApprovalCoordinator::default(),
        Arc::new(ToolRegistry::bootstrap()),
        None,
        None,
    );
    let task_id = uuid::Uuid::new_v4();
    let old_action_id = uuid::Uuid::now_v7();
    {
        let mut database = journal.database().lock().await;
        let signer = crate::CoreReceiptSigner(Arc::new(keys));
        let mut runtime =
            evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                .unwrap();
        let old_request = evohime_receipts::runtime::ActionRequest {
            action_id: old_action_id,
            task_id: task_id.to_string(),
            run_id: task_id.to_string(),
            tool_name: "shell.execute".into(),
            policy_id: "permission:ShellExecute".into(),
            normalized_scope: "workspace".into(),
            input: serde_json::json!({"program":"echo","args":[]}),
            policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
            approval_id: None,
            parent_approval_ref: None,
            preview: "old mutation".into(),
        };
        runtime.prepare(old_request).unwrap();
        runtime.mark_started(old_action_id).unwrap();
        runtime.mark_returned(old_action_id).unwrap();
        runtime
            .mark_pending_recovery(old_action_id, "unknown")
            .unwrap();
    }
    let command = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "reconcile-read-only".into(),
        client_id: "test-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(
            generated::command_envelope::Command::ReconcilePendingReceiptAction(
                generated::ReconcilePendingReceiptAction {
                    old_action_id: old_action_id.to_string(),
                    tool_name: "filesystem.read".into(),
                    input_json: r#"{"path":"observed.txt"}"#.into(),
                    workspace_path: root.display().to_string(),
                },
            ),
        ),
    };
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .unwrap();
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .unwrap();
    let response = generated::EventEnvelope::decode(
        transport::read_frame(&mut client).await.unwrap().as_slice(),
    )
    .unwrap();
    assert_eq!(response.event_type, "receipt.reconciliation");
    let payload: serde_json::Value = serde_json::from_slice(&response.payload).unwrap();
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["status"], "succeeded");
    assert_ne!(payload["action_id"], old_action_id.to_string());
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&data_root);
}

#[tokio::test]
async fn serves_bounded_git_status_and_diff_through_core_tools() {
    let root = std::env::temp_dir().join(format!("evohime-ipc-git-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("git root");
    let status = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&root)
        .status()
        .expect("git init starts");
    assert!(status.success());
    std::fs::write(root.join("notes.txt"), "hello\n").expect("notes");
    let status = std::process::Command::new("git")
        .args(["add", "notes.txt"])
        .current_dir(&root)
        .status()
        .expect("git add starts");
    assert!(status.success());
    std::fs::write(root.join("notes.txt"), "hello\nworld\n").expect("changed notes");
    let journal_path =
        std::env::temp_dir().join(format!("evohime-ipc-git-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&journal_path);
    let journal = EventJournal::open(&journal_path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let tools = Arc::new(ToolRegistry::bootstrap());
    let bridge = IpcBridge::with_coordinator_and_approvals(
        journal,
        coordinator,
        ApprovalCoordinator::default(),
        tools,
        None,
        None,
    );

    let status_payload = bridge
        .dispatch_git_read(
            root.display().to_string(),
            "git.status",
            serde_json::Value::Null,
            128,
        )
        .await
        .expect("git status reads");
    let status_json: serde_json::Value =
        serde_json::from_slice(&status_payload).expect("status json");
    assert!(status_json["output"]
        .as_str()
        .unwrap()
        .contains("notes.txt"));
    assert_eq!(status_json["truncated"], false);

    let diff_payload = bridge
        .dispatch_git_read(
            root.display().to_string(),
            "git.diff",
            serde_json::json!({"path": "notes.txt"}),
            8,
        )
        .await
        .expect("git diff reads");
    let diff_json: serde_json::Value = serde_json::from_slice(&diff_payload).expect("diff json");
    assert_eq!(diff_json["max_bytes"], 8);
    assert_eq!(diff_json["truncated"], true);

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_file(journal_path);
}

#[tokio::test]
async fn conversation_event_log_ipc_pages_and_live_events_are_typed_and_redacted() {
    let directory = tempfile::tempdir().expect("temp directory");
    let path = directory.path().join("conversation-ipc.db");
    let journal = EventJournal::open(&path).expect("journal opens");
    journal
        .accept_conversation_message(
            "conversation-1",
            "workspace-1",
            "task-1",
            "client-1",
            "token sk-12345678901234567890",
        )
        .await
        .expect("message accepts");
    let bridge = IpcBridge::new(journal);
    let page = bridge
        .dispatch_conversation_event_log(
            generated::ConversationEventLogRequest {
                schema_version: 1,
                conversation_id: "conversation-1".into(),
                before_sequence: 0,
                after_sequence: 0,
                use_before_sequence: false,
                use_after_sequence: true,
                limit: 20,
                kinds_filter: Vec::new(),
            },
            "subscribed",
        )
        .await;
    assert!(page.error_code.is_empty());
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].sequence, 1);
    assert_eq!(page.events[0].client_message_id, "client-1");
    assert!(
        !String::from_utf8_lossy(&page.events[0].payload_json).contains("sk-12345678901234567890")
    );

    let (mut client, mut server) = duplex(64 * 1024);
    bridge
        .push_journal_tail(&mut server, 0)
        .await
        .expect("live tail writes");
    let frame = transport::read_frame(&mut client)
        .await
        .expect("live frame reads");
    let envelope = generated::EventEnvelope::decode(frame.as_slice()).expect("live frame decodes");
    let Some(generated::event_envelope::Event::ConversationEventLog(live)) = envelope.event else {
        panic!("typed conversation event missing");
    };
    assert_eq!(live.operation, "live");
    assert_eq!(live.events[0].event_id, page.events[0].event_id);
}

#[tokio::test]
async fn start_task_retry_is_idempotent_and_conflict_returns_typed_non_success() {
    let path = std::env::temp_dir().join(format!(
        "evohime-ipc-conversation-start-{}-{}.db",
        std::process::id(),
        uuid::Uuid::now_v7()
    ));
    let journal = EventJournal::open(&path).expect("journal opens");
    let (coordinator, mut events) = TaskCoordinator::new_with_journal(16, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal, coordinator);
    let (mut client, server) = duplex(64 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    let command = |request_id: &str, prompt: &str| generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: request_id.into(),
        client_id: "client".into(),
        core_instance_id: String::new(),
        session_epoch: 0,
        command: Some(generated::command_envelope::Command::StartTask(
            generated::StartTask {
                task_id: "task-conversation".into(),
                prompt: prompt.into(),
                workspace_path: ".".into(),
                preferred_route_hint: "cloud".into(),
                execution_kind: "dialogue".into(),
                conversation_id: "conversation-1".into(),
                client_message_id: "client-message-1".into(),
            },
        )),
    };

    transport::write_frame(&mut client, &command("start-1", "same").encode_to_vec())
        .await
        .expect("first command writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("first command serves");
    assert!(matches!(
        events.recv().await,
        Ok(CoreEvent::TaskStarted { .. })
    ));

    transport::write_frame(&mut client, &command("start-retry", "same").encode_to_vec())
        .await
        .expect("retry writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("retry serves");

    transport::write_frame(
        &mut client,
        &command("start-conflict", "different").encode_to_vec(),
    )
    .await
    .expect("conflict writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("conflict serves");
    let frame = transport::read_frame(&mut client)
        .await
        .expect("typed rejection reads");
    let envelope = generated::EventEnvelope::decode(frame.as_slice()).expect("rejection decodes");
    let Some(generated::event_envelope::Event::ConversationEventLog(result)) = envelope.event
    else {
        panic!("conversation rejection missing");
    };
    assert_eq!(result.operation, "accept");
    assert_eq!(result.error_code, "idempotency_conflict");

    let mut duplicate_started = false;
    while let Ok(Ok(event)) =
        tokio::time::timeout(std::time::Duration::from_millis(50), events.recv()).await
    {
        duplicate_started |= matches!(event, CoreEvent::TaskStarted { .. });
    }
    assert!(
        !duplicate_started,
        "retry or conflict dispatched a second task"
    );
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn handshake_exposes_runtime_identity() {
    let path =
        std::env::temp_dir().join(format!("evohime-ipc-handshake-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    let command = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "handshake".into(),
        client_id: "client".into(),
        core_instance_id: String::new(),
        session_epoch: 9,
        command: Some(generated::command_envelope::Command::Handshake(
            generated::Handshake {
                protocol: Some(protocol()),
                client_id: "client".into(),
                session_id: "session".into(),
                session_epoch: 9,
                last_event_sequence: 0,
                capabilities: vec!["task.crud".into()],
                client_role: "shell".into(),
                nonce: String::new(),
                proof: String::new(),
            },
        )),
    };
    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .expect("handshake writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("handshake serves");
    let response = transport::read_frame(&mut client)
        .await
        .expect("response reads");
    let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
    assert!(!event.core_instance_id.is_empty());
    assert!(event.session_epoch > 0);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn malformed_command_is_rejected_without_crashing_bridge() {
    let path =
        std::env::temp_dir().join(format!("evohime-ipc-malformed-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
    let (mut client, server) = duplex(1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    transport::write_frame(&mut client, &[0xff, 0x00, 0x01])
        .await
        .expect("malformed frame writes");
    assert!(matches!(
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await,
        Err(IpcBridgeError::Protobuf(_))
    ));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn reconnect_replays_only_events_after_last_sequence() {
    let path =
        std::env::temp_dir().join(format!("evohime-ipc-reconnect-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let first = journal
        .record(&CoreEvent::TaskStarted {
            task_id: "task-reconnect".into(),
            prompt: "one".into(),
        })
        .await
        .expect("first event");
    journal
        .record(&CoreEvent::TaskCompleted {
            task_id: "task-reconnect".into(),
            final_message: "two".into(),
        })
        .await
        .expect("second event");
    let bridge = IpcBridge::new(journal);
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    let command = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "reconnect".into(),
        client_id: "client".into(),
        core_instance_id: String::new(),
        session_epoch: 0,
        command: Some(generated::command_envelope::Command::ReplayEvents(
            generated::ReplayEvents {
                after_sequence: first as u64,
            },
        )),
    };
    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .expect("reconnect writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("reconnect serves");
    let response = transport::read_frame(&mut client)
        .await
        .expect("event reads");
    let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
    assert_eq!(event.event_type, "task.completed");
    assert_eq!(event.sequence_id, first as u64 + 1);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn serves_task_crud_and_replays_deduplicated_create() {
    let path = std::env::temp_dir().join(format!("evohime-ipc-task-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal, coordinator);
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    let command = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "create-project-1".into(),
        client_id: "test-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::CreateProject(
            generated::CreateProject {
                project_id: "project-1".into(),
                title: "Demo".into(),
                workspace_path: "C:\\Projects\\demo".into(),
                source_ref: "plan:0a".into(),
            },
        )),
    };
    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .expect("command writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("project creates");
    let first = transport::read_frame(&mut client)
        .await
        .expect("first response");

    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .expect("duplicate writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("duplicate replays");
    let second = transport::read_frame(&mut client)
        .await
        .expect("second response");
    assert_eq!(first, second);

    let mut conflict = command.clone();
    if let Some(generated::command_envelope::Command::CreateProject(project)) =
        &mut conflict.command
    {
        project.title = "Different".into();
    }
    transport::write_frame(&mut client, &conflict.encode_to_vec())
        .await
        .expect("conflicting writes");
    assert!(bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .is_err());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn imports_prd_without_touching_workspace_and_rejects_duplicate_import() {
    let path = std::env::temp_dir().join(format!("evohime-ipc-prd-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
    let (mut client, server) = duplex(32 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    let project = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "project-prd".into(),
        client_id: "prd-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::CreateProject(
            generated::CreateProject {
                project_id: "project-prd".into(),
                title: "PRD".into(),
                workspace_path: "C:\\Projects\\prd".into(),
                source_ref: String::new(),
            },
        )),
    };
    transport::write_frame(&mut client, &project.encode_to_vec())
        .await
        .expect("project writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("project creates");
    let _ = transport::read_frame(&mut client)
        .await
        .expect("project response");

    let import = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "import-prd-1".into(),
        client_id: "prd-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::ImportPrd(
            generated::ImportPrd {
                import_id: "import-1".into(),
                project_id: "project-prd".into(),
                origin: "prd.md".into(),
                version: "v1".into(),
                source_text: "# Plan\n\n## Task\n- [ ] Pass\n".into(),
            },
        )),
    };
    transport::write_frame(&mut client, &import.encode_to_vec())
        .await
        .expect("import writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("import succeeds");
    let response = transport::read_frame(&mut client)
        .await
        .expect("import response");
    let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
    assert_eq!(event.event_type, "prd.imported");
    assert_eq!(
        journal
            .list_task_graph("project-prd")
            .await
            .unwrap()
            .0
            .len(),
        1
    );

    let mut duplicate = import;
    if let Some(generated::command_envelope::Command::ImportPrd(request)) = &mut duplicate.command {
        request.source_text.push_str("\n## Another");
        duplicate.request_id = "import-prd-2".into();
    }
    transport::write_frame(&mut client, &duplicate.encode_to_vec())
        .await
        .expect("duplicate writes");
    assert!(bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .is_err());
    assert_eq!(
        journal
            .list_task_graph("project-prd")
            .await
            .unwrap()
            .0
            .len(),
        1
    );
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn serves_run_doctor_with_real_storage_and_pipe_state() {
    let path = std::env::temp_dir().join(format!("evohime-ipc-doctor-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal, coordinator);
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    let command = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "doctor-1".into(),
        client_id: "doctor-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::RunDoctor(
            generated::RunDoctor {
                project_id: String::new(),
                detail_level: 1,
            },
        )),
    };
    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .expect("command writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("doctor serves");
    let response = transport::read_frame(&mut client)
        .await
        .expect("response reads");
    let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
    assert_eq!(event.event_type, "doctor.report");
    let report: serde_json::Value =
        serde_json::from_slice(&event.payload).expect("doctor report is valid json");
    assert_eq!(report["bounded"], serde_json::json!(true));
    let checks = report["checks"].as_array().expect("checks array");
    assert_eq!(checks.len(), 8);
    let storage_check = checks
        .iter()
        .find(|check| check["id"] == "storage")
        .expect("storage check present");
    // A freshly-opened journal exists, is writable, and is on the
    // current schema version, so this reflects real (not fabricated)
    // storage state.
    assert_eq!(storage_check["status"], serde_json::json!("OK"));
    let permissions_check = checks
        .iter()
        .find(|check| check["id"] == "permissions")
        .expect("permissions check present");
    // No project_id was supplied, so the permissions probe is honestly
    // fail-closed rather than fabricated as healthy.
    assert_ne!(permissions_check["status"], serde_json::json!("OK"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn saves_and_lists_research_evidence_against_real_storage() {
    let path = std::env::temp_dir().join(format!("evohime-ipc-research-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal, coordinator);
    let (mut client, server) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);

    let save = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "research-save-1".into(),
        client_id: "research-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::SaveResearchEvidence(
            generated::SaveResearchEvidence {
                work_item_id: "task-42".into(),
                source_kind: "url".into(),
                source_ref: "https://example.test/article".into(),
                title: "Example Article".into(),
                publisher: "Example Org".into(),
                content_type: "text/html".into(),
                raw_excerpt: "Useful finding sk-secret alice@example.test".into(),
                retrieved_at_ms: 1_700_000_000_000,
                ttl_ms: 3_600_000,
            },
        )),
    };
    transport::write_frame(&mut client, &save.encode_to_vec())
        .await
        .expect("save writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("save serves");
    let response = transport::read_frame(&mut client)
        .await
        .expect("save response reads");
    let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
    assert_eq!(event.event_type, "research.evidence.saved");
    let saved: serde_json::Value =
        serde_json::from_slice(&event.payload).expect("save payload is valid json");
    assert_eq!(saved["work_item_id"], serde_json::json!("task-42"));
    let evidence_id = saved["id"].as_str().expect("id present").to_owned();
    assert_eq!(
        saved["evidence"]["excerpt"],
        serde_json::json!("Useful finding [REDACTED] [REDACTED]")
    );

    let list = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "research-list-1".into(),
        client_id: "research-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::ListResearchEvidence(
            generated::ListResearchEvidence {
                work_item_id: "task-42".into(),
            },
        )),
    };
    transport::write_frame(&mut client, &list.encode_to_vec())
        .await
        .expect("list writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("list serves");
    let response = transport::read_frame(&mut client)
        .await
        .expect("list response reads");
    let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
    assert_eq!(event.event_type, "research.evidence.list");
    let listed: serde_json::Value =
        serde_json::from_slice(&event.payload).expect("list payload is valid json");
    let records = listed["records"].as_array().expect("records array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], serde_json::json!(evidence_id));
    assert_eq!(records[0]["source_kind"], serde_json::json!("url"));
    assert_eq!(
        records[0]["redacted_excerpt"],
        serde_json::json!("Useful finding [REDACTED] [REDACTED]")
    );
    assert_eq!(records[0]["provenance_link"], serde_json::json!("task-42"));

    let _ = std::fs::remove_file(path);
}

fn run_research_fetch_command(
    work_item_id: &str,
    url: String,
    allowed_domains: Vec<String>,
    max_bytes: u64,
) -> generated::CommandEnvelope {
    generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: format!("research-fetch-{work_item_id}"),
        client_id: "research-fetch-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(generated::command_envelope::Command::RunResearchFetch(
            generated::RunResearchFetch {
                work_item_id: work_item_id.into(),
                url,
                title: "Example Article".into(),
                allowed_domains,
                max_bytes,
                max_latency_ms: 5_000,
                max_cost_micros: 0,
                ttl_ms: 3_600_000,
            },
        )),
    }
}

#[tokio::test]
async fn run_research_fetch_persists_real_evidence_from_a_live_http_get() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/article"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string("Useful finding sk-secret alice@example.test")
                .insert_header("content-type", "text/plain"),
        )
        .mount(&server)
        .await;
    let _private = evohime_tool_runtime::lock_private_override(Some(true));
    let domain = reqwest::Url::parse(&server.uri())
        .expect("mock uri parses")
        .host_str()
        .expect("mock uri has host")
        .to_ascii_lowercase();

    let path = std::env::temp_dir().join(format!(
        "evohime-ipc-research-fetch-ok-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
    let (mut client, server_io) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

    let command = run_research_fetch_command(
        "task-fetch-ok",
        format!("{}/article", server.uri()),
        vec![domain],
        4096,
    );
    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .expect("fetch command writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("fetch serves");
    let response = transport::read_frame(&mut client)
        .await
        .expect("fetch response reads");
    let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
    assert_eq!(event.event_type, "research.fetch.completed");
    let payload: serde_json::Value =
        serde_json::from_slice(&event.payload).expect("fetch payload is valid json");
    assert_eq!(payload["state"], serde_json::json!("completed"));
    assert_eq!(
        payload["evidence"]["excerpt"],
        serde_json::json!("Useful finding [REDACTED] [REDACTED]")
    );
    let evidence_id = payload["id"].as_str().expect("id present").to_owned();

    let records = journal
        .list_research_evidence("task-fetch-ok")
        .await
        .expect("evidence lists from real storage");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, evidence_id);
    assert_eq!(
        records[0].redacted_excerpt,
        "Useful finding [REDACTED] [REDACTED]"
    );

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn run_research_fetch_denies_domain_outside_allowlist_and_persists_nothing() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/article"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("should not fetch"))
        .mount(&server)
        .await;
    let _private = evohime_tool_runtime::lock_private_override(Some(true));

    let path = std::env::temp_dir().join(format!(
        "evohime-ipc-research-fetch-denied-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
    let (mut client, server_io) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

    let command = run_research_fetch_command(
        "task-fetch-denied",
        format!("{}/article", server.uri()),
        vec!["not-the-mock-domain.example".into()],
        4096,
    );
    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .expect("fetch command writes");
    let outcome = bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await;
    assert!(
        outcome.is_err(),
        "domain-denied fetch must fail the command"
    );
    assert_eq!(
        server
            .received_requests()
            .await
            .expect("requests tracked")
            .len(),
        0,
        "no network call should happen for a denied domain"
    );

    let records = journal
        .list_research_evidence("task-fetch-denied")
        .await
        .expect("list succeeds");
    assert!(records.is_empty(), "no evidence should be persisted");

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn run_research_fetch_blocks_ssrf_targets_and_persists_nothing() {
    let _private = evohime_tool_runtime::lock_private_override(Some(false));

    let path = std::env::temp_dir().join(format!(
        "evohime-ipc-research-fetch-ssrf-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
    let (mut client, server_io) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

    let command = run_research_fetch_command(
        "task-fetch-ssrf",
        "http://127.0.0.1:9/".into(),
        vec!["127.0.0.1".into()],
        4096,
    );
    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .expect("fetch command writes");
    let outcome = bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await;
    assert!(outcome.is_err(), "ssrf-blocked fetch must fail the command");

    let records = journal
        .list_research_evidence("task-fetch-ssrf")
        .await
        .expect("list succeeds");
    assert!(records.is_empty(), "no evidence should be persisted");

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn run_research_fetch_rejects_oversized_response_and_persists_nothing() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/big"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("x".repeat(4_096)))
        .mount(&server)
        .await;
    let _private = evohime_tool_runtime::lock_private_override(Some(true));
    let domain = reqwest::Url::parse(&server.uri())
        .expect("mock uri parses")
        .host_str()
        .expect("mock uri has host")
        .to_ascii_lowercase();

    let path = std::env::temp_dir().join(format!(
        "evohime-ipc-research-fetch-oversized-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
    let (mut client, server_io) = duplex(16 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

    let command = run_research_fetch_command(
        "task-fetch-oversized",
        format!("{}/big", server.uri()),
        vec![domain],
        16,
    );
    transport::write_frame(&mut client, &command.encode_to_vec())
        .await
        .expect("fetch command writes");
    let outcome = bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await;
    assert!(outcome.is_err(), "oversized response must fail the command");

    let records = journal
        .list_research_evidence("task-fetch-oversized")
        .await
        .expect("list succeeds");
    assert!(records.is_empty(), "no evidence should be persisted");

    let _ = std::fs::remove_file(path);
}
