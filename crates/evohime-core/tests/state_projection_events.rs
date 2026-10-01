use evohime_core::{CoreCommand, CoreEvent, EventJournal, TaskCoordinator};

#[tokio::test]
async fn state_projection_commands_record_one_journal_event_each() {
    let directory = tempfile::tempdir().expect("temporary journal directory");
    let journal = EventJournal::open(directory.path().join("core.db")).expect("journal opens");
    let (coordinator, _notifications) = TaskCoordinator::new_with_journal(8, None, journal.clone());

    let (reply, response) = tokio::sync::oneshot::channel();
    coordinator
        .dispatch(CoreCommand::ContextNamespace {
            operation: "unsupported".into(),
            namespace_id: "namespace-test".into(),
            payload: Vec::new(),
            expected_revision: 0,
            idempotency_key: String::new(),
            reply,
        })
        .await
        .expect("context command dispatches");
    assert!(response.await.expect("context command responds").is_err());

    let (reply, response) = tokio::sync::oneshot::channel();
    coordinator
        .dispatch(CoreCommand::DurableBackgroundExecution {
            operation: "unsupported".into(),
            run_id: "run-test".into(),
            owner_scope: "workspace-test".into(),
            payload: Vec::new(),
            expected_revision: 0,
            idempotency_key: String::new(),
            reply,
        })
        .await
        .expect("background command dispatches");
    assert!(response
        .await
        .expect("background command responds")
        .is_err());

    let events = journal.replay(0, 32).await.expect("journal replays");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "context_namespace.result")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "background_execution.result")
            .count(),
        1
    );
}

#[tokio::test]
async fn state_projection_journal_failure_is_reported_before_success_reply() {
    let directory = tempfile::tempdir().expect("temporary journal directory");
    let journal = EventJournal::open(directory.path().join("core.db")).expect("journal opens");
    {
        let mut database = journal.database().lock().await;
        database
            .connection_mut()
            .execute_batch("DROP TABLE events")
            .expect("test removes journal table");
    }
    let (coordinator, mut notifications) =
        TaskCoordinator::new_with_journal(8, None, journal.clone());

    let (reply, response) = tokio::sync::oneshot::channel();
    coordinator
        .dispatch(CoreCommand::DurableBackgroundExecution {
            operation: "list_runs".into(),
            run_id: "run-test".into(),
            owner_scope: "workspace-test".into(),
            payload: Vec::new(),
            expected_revision: 0,
            idempotency_key: String::new(),
            reply,
        })
        .await
        .expect("background command dispatches");

    assert_eq!(
        response.await.expect("background command responds"),
        Err("event_persistence_failed".into())
    );
    let notification =
        tokio::time::timeout(std::time::Duration::from_secs(1), notifications.recv())
            .await
            .expect("persistence failure is reported")
            .expect("failure notification is sent");
    assert!(matches!(
        notification,
        CoreEvent::EventPersistenceFailed { source, .. } if source == "journal"
    ));
    assert!(coordinator
        .persistence_error()
        .await
        .is_some_and(|error| error.starts_with("journal:")));
}
