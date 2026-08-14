use evohime_core::EventJournal;
use evohime_local_storage::{
    LocalDatabase, RecoveryState, RunCheckpointRecord, RunEffectRecord, RunRecord, WorkItemRecord,
};
use std::{
    env,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_path(suffix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is valid")
        .as_nanos();
    env::temp_dir().join(format!(
        "evohime-recovery-state-machine-{suffix}-{stamp}.db"
    ))
}

fn seed_executing_effect(
    database: &LocalDatabase,
    run_id: &str,
    effect_id: &str,
    task_id: &str,
    kind: &str,
) {
    database
        .create_project("project-state-machine", "State machine wiring", ".", None)
        .expect("project creates");
    let task = WorkItemRecord {
        id: task_id.into(),
        project_id: "project-state-machine".into(),
        parent_id: None,
        title: "state machine wiring".into(),
        description: String::new(),
        source_ref: None,
        acceptance_criteria: String::new(),
        non_goals: String::new(),
        status: "in_progress".into(),
        priority: 0,
        estimate: None,
        complexity: None,
        attempt_count: 0,
        version: 1,
    };
    database.create_work_item(&task).expect("task creates");
    let run = RunRecord {
        id: run_id.into(),
        work_item_id: task.id.clone(),
        status: "running".into(),
        policy_snapshot: Vec::new(),
        role_snapshot: Vec::new(),
        skill_snapshot: Vec::new(),
        model_route_snapshot: Vec::new(),
    };
    let checkpoint = RunCheckpointRecord {
        run_id: run.id.clone(),
        checkpoint_id: format!("checkpoint-{effect_id}"),
        stage: "build".into(),
        node_id: "bounded-build".into(),
        attempt: 1,
        input_hash: "state-machine-intent".into(),
        state_json: br#"{"stage":"build"}"#.to_vec(),
        pending_effects_json: format!("[\"{effect_id}\"]").into_bytes(),
        committed_at: String::new(),
    };
    let effect = RunEffectRecord {
        effect_id: effect_id.into(),
        run_id: run.id.clone(),
        node_id: "bounded-build".into(),
        kind: kind.into(),
        idempotency_key: format!("{run_id}:bounded-build"),
        immutable_intent_hash: "state-machine-intent".into(),
        state: "prepared".into(),
        started_at: None,
        completed_at: None,
        result_hash: None,
    };
    if kind == "agent_task" {
        database
            .prepare_agent_run_effect(&effect, &task.id)
            .expect("agent effect prepares");
        database
            .mark_agent_effect_executing(&effect.effect_id)
            .expect("agent effect starts");
    } else {
        database
            .prepare_run_effect(&run, &checkpoint, &effect)
            .expect("effect prepares");
        database
            .mark_effect_executing(&effect.effect_id)
            .expect("effect starts");
    }
}

/// Core startup recovery (`recover_and_reconcile_after_restart`, invoked from
/// `main.rs` before the IPC bridge starts serving) must drive every
/// recovered run through the durable, idempotent recovery state machine
/// (`RECOVERING -> RECONCILING -> RESUMABLE|BLOCKED`) instead of only
/// touching `run_effects`/`run_reconciliations` ad hoc. This is the wiring
/// that connects the bounded state machine (evohime-local-storage) to the
/// live Core dispatch/startup path.
#[tokio::test]
async fn startup_recovery_drives_the_durable_recovery_state_machine_to_blocked() {
    let path = temp_path("blocked");
    {
        let database = LocalDatabase::open(&path).expect("database opens");
        seed_executing_effect(
            &database,
            "run-blocked",
            "effect-blocked",
            "task-blocked",
            "bounded_build",
        );
    }

    let journal = EventJournal::open(&path).expect("journal reopens");
    let reconciliations = journal
        .recover_and_reconcile_after_restart()
        .await
        .expect("recovery runs");
    assert_eq!(reconciliations.len(), 1);

    let database = LocalDatabase::open(&path).expect("database reopens for assertions");
    let recovery = database
        .latest_recovery("run-blocked")
        .expect("recovery reads")
        .expect("recovery record exists");
    // No snapshot was ever recorded for this run, so the outcome is
    // unconfirmed and the state machine must land on BLOCKED, not silently
    // resume.
    assert_eq!(recovery.state, RecoveryState::Blocked);

    // Re-running recovery after nothing new is `unknown` must not attempt a
    // blind retry or a second, conflicting transition.
    let empty = journal
        .recover_and_reconcile_after_restart()
        .await
        .expect("second recovery pass runs");
    assert!(empty.is_empty());

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[tokio::test]
async fn agent_run_reconciles_only_from_a_durable_terminal_event() {
    let path = temp_path("agent-completed");
    {
        let database = LocalDatabase::open(&path).expect("database opens");
        seed_executing_effect(
            &database,
            "run-agent-completed",
            "effect-agent-completed",
            "task-agent-completed",
            "agent_task",
        );
        database
            .append_event(
                "task-agent-completed",
                "task.completed",
                br#"{"final_message":"done"}"#,
            )
            .expect("terminal event persists");
    }

    let journal = EventJournal::open(&path).expect("journal reopens");
    let reconciliations = journal
        .recover_and_reconcile_after_restart()
        .await
        .expect("agent recovery runs");
    assert_eq!(reconciliations.len(), 1);
    assert_eq!(reconciliations[0].verifier, "task_event_journal");
    assert_eq!(reconciliations[0].state, "reconciled_success");

    let database = LocalDatabase::open(&path).expect("database reopens for assertions");
    assert_eq!(
        database
            .get_agent_run_effect("effect-agent-completed")
            .unwrap()
            .unwrap()
            .state,
        "completed_success"
    );
    assert_eq!(
        database
            .latest_agent_recovery("run-agent-completed")
            .unwrap()
            .unwrap()
            .state,
        RecoveryState::Resumable
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}
