use super::*;

#[allow(clippy::too_many_arguments)]
fn accept_message(
    connection: &Connection,
    conversation_id: &str,
    workspace_id: &str,
    task_id: &str,
    client_message_id: &str,
    authoritative_payload: &[u8],
    renderer_payload: &[u8],
    content_hash: &str,
    timestamp_ms: i64,
) -> Result<MessageAcceptance, ConversationStoreError> {
    super::accept_message(
        connection,
        AcceptMessageInput {
            conversation_id,
            workspace_id,
            task_id,
            client_message_id,
            authoritative_payload,
            renderer_payload,
            content_hash,
            timestamp_ms,
        },
    )
}

fn message_payload(text: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"content": text})).unwrap()
}

#[test]
fn accepts_messages_once_and_keeps_per_conversation_sequence() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let first_payload = message_payload("одинаковый текст");
    let first = accept_message(
        &connection,
        "conversation-1",
        "workspace-1",
        "task-1",
        "client-1",
        &first_payload,
        &first_payload,
        &"1".repeat(64),
        10,
    )
    .unwrap();
    let duplicate = accept_message(
        &connection,
        "conversation-1",
        "workspace-1",
        "task-retry",
        "client-1",
        &first_payload,
        &first_payload,
        &"1".repeat(64),
        11,
    )
    .unwrap();
    let second_payload = message_payload("одинаковый текст");
    let second = accept_message(
        &connection,
        "conversation-1",
        "workspace-1",
        "task-2",
        "client-2",
        &second_payload,
        &second_payload,
        &"2".repeat(64),
        12,
    )
    .unwrap();

    assert_eq!(first.event.sequence, 1);
    assert!(!first.deduplicated);
    assert_eq!(duplicate.event.event_id, first.event.event_id);
    assert_eq!(duplicate.task_id, "task-1");
    assert!(duplicate.deduplicated);
    assert_eq!(second.event.sequence, 2);
}

#[test]
fn rejects_reusing_a_client_message_id_for_different_content() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let payload = message_payload("first");
    accept_message(
        &connection,
        "conversation-1",
        "workspace-1",
        "task-1",
        "client-1",
        &payload,
        &payload,
        &"1".repeat(64),
        10,
    )
    .unwrap();

    let error = accept_message(
        &connection,
        "conversation-1",
        "workspace-1",
        "task-2",
        "client-1",
        &message_payload("second"),
        &message_payload("second"),
        &"2".repeat(64),
        11,
    )
    .unwrap_err();
    assert_eq!(error, ConversationStoreError::IdempotencyConflict);
}

#[test]
fn accepted_message_dispatch_is_claimed_once_and_retryable_after_definite_failure() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let payload = message_payload("dispatch");
    accept_message(
        &connection,
        "conversation-1",
        "workspace-1",
        "task-1",
        "client-1",
        &payload,
        &payload,
        &"1".repeat(64),
        10,
    )
    .unwrap();
    assert!(claim_message_dispatch(&connection, "conversation-1", "client-1").unwrap());
    assert!(!claim_message_dispatch(&connection, "conversation-1", "client-1").unwrap());
    finish_message_dispatch(&connection, "conversation-1", "client-1", false).unwrap();
    assert!(claim_message_dispatch(&connection, "conversation-1", "client-1").unwrap());
    finish_message_dispatch(&connection, "conversation-1", "client-1", true).unwrap();
    assert!(!claim_message_dispatch(&connection, "conversation-1", "client-1").unwrap());
}

#[test]
fn duplicate_message_cannot_cross_the_conversation_workspace_binding() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let payload = message_payload("same");
    accept_message(
        &connection,
        "conversation-1",
        "workspace-1",
        "task-1",
        "client-1",
        &payload,
        &payload,
        &"1".repeat(64),
        10,
    )
    .unwrap();

    let error = accept_message(
        &connection,
        "conversation-1",
        "workspace-2",
        "task-2",
        "client-1",
        &payload,
        &payload,
        &"1".repeat(64),
        11,
    )
    .unwrap_err();
    assert_eq!(error, ConversationStoreError::InvalidInput);
}

#[test]
fn history_after_uses_a_stable_cursor_while_new_events_arrive() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    for index in 1..=3 {
        let payload = message_payload(&format!("event-{index}"));
        append_event(
            &connection,
            NewConversationEvent {
                conversation_id: "conversation-1",
                workspace_id: "workspace-1",
                kind: "task_state",
                category: "task",
                authoritative_payload: &payload,
                renderer_payload: &payload,
                correlation_id: None,
                causation_id: None,
                task_id: Some("task-1"),
                run_id: None,
                turn_id: None,
                client_message_id: None,
                persistence_class: "durable",
                sensitivity: "internal",
                timestamp_ms: index,
            },
        )
        .unwrap();
    }

    let first_page = history_after(&connection, "conversation-1", 0, 2).unwrap();
    let later_payload = message_payload("event-4");
    append_event(
        &connection,
        NewConversationEvent {
            conversation_id: "conversation-1",
            workspace_id: "workspace-1",
            kind: "task_state",
            category: "task",
            authoritative_payload: &later_payload,
            renderer_payload: &later_payload,
            correlation_id: None,
            causation_id: None,
            task_id: Some("task-1"),
            run_id: None,
            turn_id: None,
            client_message_id: None,
            persistence_class: "durable",
            sensitivity: "internal",
            timestamp_ms: 4,
        },
    )
    .unwrap();
    let second_page = history_after(
        &connection,
        "conversation-1",
        first_page.newest_sequence.unwrap(),
        2,
    )
    .unwrap();

    assert_eq!(
        first_page
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(first_page.has_newer);
    assert_eq!(
        second_page
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    assert!(!second_page.has_newer);
}

#[test]
fn history_before_and_compacted_cursor_have_explicit_bounds() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    for index in 1..=4 {
        let payload = message_payload(&format!("event-{index}"));
        append_event(
            &connection,
            NewConversationEvent {
                conversation_id: "conversation-1",
                workspace_id: "workspace-1",
                kind: "task_state",
                category: "task",
                authoritative_payload: &payload,
                renderer_payload: &payload,
                correlation_id: None,
                causation_id: None,
                task_id: Some("task-1"),
                run_id: None,
                turn_id: None,
                client_message_id: None,
                persistence_class: "durable",
                sensitivity: "internal",
                timestamp_ms: index,
            },
        )
        .unwrap();
    }
    let before = history_before(&connection, "conversation-1", 4, 2).unwrap();
    assert_eq!(
        before
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert!(before.has_older);
    assert!(before.has_newer);

    let earliest = record_compacted_prefix(
        &connection,
        "conversation-1",
        2,
        "snapshot-1",
        br#"{"summary":"events 1-2"}"#,
        10,
    )
    .unwrap();
    assert_eq!(earliest, 3);
    assert_eq!(
        history_after(&connection, "conversation-1", 0, 2).unwrap_err(),
        ConversationStoreError::CursorExpired {
            earliest_available_sequence: 3
        }
    );
    let retained = history_after(&connection, "conversation-1", 2, 10).unwrap();
    assert_eq!(retained.earliest_available_sequence, 3);
    assert_eq!(
        retained
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    connection
            .execute(
                "UPDATE conversation_log_compacted_ranges SET snapshot_payload=X'00' WHERE conversation_id='conversation-1'",
                [],
            )
            .unwrap();
    assert!(matches!(
        history_after(&connection, "conversation-1", 0, 2),
        Err(ConversationStoreError::Sql(message)) if message.contains("checksum")
    ));
}

#[test]
fn schema_49_migrates_and_sequence_survives_database_reopen() {
    let path = std::env::temp_dir().join(format!(
        "evohime-conversation-log-{}-{}.db",
        std::process::id(),
        Uuid::now_v7()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let legacy = Connection::open(&path).unwrap();
        legacy.pragma_update(None, "user_version", 49).unwrap();
    }
    {
        let database = crate::LocalDatabase::open(&path).unwrap();
        assert_eq!(database.schema_version().unwrap(), crate::SCHEMA_VERSION);
        let payload = message_payload("first");
        let first = append_event(
            database.connection(),
            NewConversationEvent {
                conversation_id: "conversation-1",
                workspace_id: "workspace-1",
                kind: "task_state",
                category: "task",
                authoritative_payload: &payload,
                renderer_payload: &payload,
                correlation_id: None,
                causation_id: None,
                task_id: Some("task-1"),
                run_id: None,
                turn_id: None,
                client_message_id: None,
                persistence_class: "durable",
                sensitivity: "internal",
                timestamp_ms: 1,
            },
        )
        .unwrap();
        assert_eq!(first.sequence, 1);
    }
    {
        let database = crate::LocalDatabase::open(&path).unwrap();
        let payload = message_payload("second");
        let second = append_event(
            database.connection(),
            NewConversationEvent {
                conversation_id: "conversation-1",
                workspace_id: "workspace-1",
                kind: "task_state",
                category: "task",
                authoritative_payload: &payload,
                renderer_payload: &payload,
                correlation_id: None,
                causation_id: None,
                task_id: Some("task-2"),
                run_id: None,
                turn_id: None,
                client_message_id: None,
                persistence_class: "durable",
                sensitivity: "internal",
                timestamp_ms: 2,
            },
        )
        .unwrap();
        assert_eq!(second.sequence, 2);
    }
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
    let _ = std::fs::remove_file(path);
}
