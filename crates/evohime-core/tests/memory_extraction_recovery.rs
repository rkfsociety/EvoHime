use evohime_core::{CoreEvent, EventJournal};

#[tokio::test]
async fn dialog_extraction_source_can_be_rehydrated_from_durable_task_events() {
    let directory = tempfile::tempdir().expect("temporary journal directory");
    let journal = EventJournal::open(directory.path().join("core.db")).expect("journal opens");
    let task_id = "memory-source-task-1";

    journal
        .record(&CoreEvent::TaskStarted {
            task_id: task_id.to_owned(),
            prompt: "запомни: язык интерфейса русский".to_owned(),
        })
        .await
        .expect("task start persists");
    journal
        .record(&CoreEvent::TaskCompleted {
            task_id: task_id.to_owned(),
            final_message: "Предпочтение учтено.".to_owned(),
        })
        .await
        .expect("task completion persists");

    let restored = journal
        .memory_extraction_dialog_context(task_id)
        .await
        .expect("durable task context is readable");

    assert_eq!(
        restored,
        Some((
            "запомни: язык интерфейса русский".to_owned(),
            "Предпочтение учтено.".to_owned(),
        ))
    );
}

#[tokio::test]
async fn memory_extraction_diagnostic_uses_bounded_ipc_event_type() {
    let directory = tempfile::tempdir().expect("temporary journal directory");
    let journal = EventJournal::open(directory.path().join("core.db")).expect("journal opens");
    let event = CoreEvent::MemoryExtractionDiagnostic {
        task_id: "memory-source-task-1".to_owned(),
        source_id: Some("sha256:0123abcd".to_owned()),
        origin: "dialog".to_owned(),
        stage: "finalization".to_owned(),
        status: "deferred".to_owned(),
        reason_code: Some("lease_busy".to_owned()),
        backlog: 2,
        conflict_count: 1,
        suppressed_reentry_count: 0,
    };

    journal.record(&event).await.expect("diagnostic persists");
    let history = journal
        .task_history("memory-source-task-1", 8)
        .await
        .expect("event history is readable");
    let stored = history.first().expect("diagnostic event is present");
    let payload = String::from_utf8_lossy(&stored.payload);

    assert_eq!(stored.event_type, "memory.extraction");
    assert!(payload.contains("lease_busy"));
    assert!(!payload.contains("prompt"));
    assert!(!payload.contains("statement"));
    assert!(!payload.contains("model_output"));
}

#[tokio::test]
async fn dialog_recovery_rejects_duplicate_or_mismatched_source_events() {
    let directory = tempfile::tempdir().expect("temporary journal directory");
    let journal = EventJournal::open(directory.path().join("core.db")).expect("journal opens");
    let task_id = "memory-source-task-2";
    journal
        .record(&CoreEvent::TaskStarted {
            task_id: task_id.to_owned(),
            prompt: "first prompt".to_owned(),
        })
        .await
        .expect("first task start persists");
    journal
        .record(&CoreEvent::TaskStarted {
            task_id: task_id.to_owned(),
            prompt: "different prompt".to_owned(),
        })
        .await
        .expect("second task start persists");
    journal
        .record(&CoreEvent::TaskCompleted {
            task_id: task_id.to_owned(),
            final_message: "completion".to_owned(),
        })
        .await
        .expect("task completion persists");

    let restored = journal.memory_extraction_dialog_context(task_id).await;

    assert_eq!(restored, Err("task_source_ambiguous".to_owned()));
}

#[tokio::test]
async fn dialog_recovery_rejects_oversized_source_before_loading_its_payload() {
    let directory = tempfile::tempdir().expect("temporary journal directory");
    let journal = EventJournal::open(directory.path().join("core.db")).expect("journal opens");
    let task_id = "memory-source-task-3";
    journal
        .record(&CoreEvent::TaskStarted {
            task_id: task_id.to_owned(),
            prompt: "bounded prompt".to_owned(),
        })
        .await
        .expect("task start persists");
    journal
        .record(&CoreEvent::TaskCompleted {
            task_id: task_id.to_owned(),
            final_message: "x".repeat(1024 * 1024 + 1),
        })
        .await
        .expect("oversized task completion persists for recovery testing");

    let restored = journal.memory_extraction_dialog_context(task_id).await;

    assert_eq!(restored, Err("task_source_too_large".to_owned()));
}
