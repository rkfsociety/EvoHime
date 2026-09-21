use super::*;
use crate::automation_store::{self, AutomationDefinitionRecord};

#[test]
fn metadata_and_waits_are_bounded_and_idempotent() {
    let c = Connection::open_in_memory().unwrap();
    automation_store::install_schema(&c).unwrap();
    install_schema(&c).unwrap();
    automation_store::insert_definition(
        &c,
        &AutomationDefinitionRecord {
            definition_id: "d".into(),
            revision: 1,
            owner_scope: "o".into(),
            definition_json: "{}".into(),
            definition_hash: "h".into(),
        },
        1,
    )
    .unwrap();
    let run = automation_store::AutomationRunRecord {
        run_id: "r".into(),
        definition_id: "d".into(),
        revision: 1,
        owner_scope: "o".into(),
        idempotency_key: "k".into(),
        payload_hash: "p".into(),
        state: "accepted".into(),
        generation: 1,
        permission_snapshot: "p".into(),
        approval_snapshot: "a".into(),
    };
    automation_store::insert_run(&c, &run, 1).unwrap();
    assert!(save_run_metadata(
        &c,
        "r",
        "workflow_run",
        br#"{}"#,
        "q",
        "background_workflow",
        None,
        "hash",
        Some(10),
        1
    )
    .unwrap());
    let mut c = c;
    assert!(put_wait(
        &mut c,
        &WaitRecord {
            run_id: "r".into(),
            revision: 1,
            condition_json: br#"{}"#.to_vec(),
            wake_at_ms: Some(10),
            state: "waiting".into()
        },
        1
    )
    .unwrap());
    assert!(!put_wait(
        &mut c,
        &WaitRecord {
            run_id: "r".into(),
            revision: 1,
            condition_json: br#"{"duplicate":true}"#.to_vec(),
            wake_at_ms: Some(99),
            state: "waiting".into()
        },
        2
    )
    .unwrap());
    assert_eq!(due_wakeups(&c, 10, 10).unwrap().len(), 1);
    assert!(!put_wait(
        &mut c,
        &WaitRecord {
            run_id: "r".into(),
            revision: 0,
            condition_json: br#"{"stale":true}"#.to_vec(),
            wake_at_ms: Some(1),
            state: "waiting".into()
        },
        2
    )
    .unwrap());
    assert_eq!(due_wakeups(&c, 10, 10).unwrap().len(), 1);
    assert!(save_queue(
        &c,
        &QueueRecord {
            queue_id: "q".into(),
            owner_scope: "o".into(),
            revision: 1,
            max_active: 2,
            max_queued: 4,
            priority: "background_workflow".into(),
            overflow_policy: "reject_new".into(),
            content_hash: "hash".into()
        },
        1
    )
    .unwrap());
    assert_eq!(list_queues(&c, "o", 10).unwrap().len(), 1);
    let attempt = AttemptRecord {
        attempt_id: "a".into(),
        run_id: "r".into(),
        generation: 1,
        dispatcher_id: "test".into(),
        state: "blocked".into(),
        outcome_code: "runtime_adapter_unavailable".into(),
        started_at_ms: Some(1),
        ended_at_ms: Some(1),
    };
    assert!(insert_attempt(&c, &attempt, 1).unwrap());
    assert!(!insert_attempt(&c, &attempt, 2).unwrap());
    assert_eq!(reconcile_after_restart(&c, 3).unwrap(), 0);
}
