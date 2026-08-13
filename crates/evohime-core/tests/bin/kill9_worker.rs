//! Test-only helper binary used by the kill-9 model test
//! (`crates/evohime-core/tests/kill9_recovery.rs`).
//!
//! It opens the durable local database at the path given as argv[1], drives
//! a run's effect into the `executing` state exactly like a real Core
//! process performing a bounded Build effect would, signals readiness by
//! writing the marker file given as argv[2], and then blocks forever
//! (simulating "still doing work"). The parent test process forcibly
//! terminates this process (`TerminateProcess` via `Child::kill()`) once the
//! marker file appears, leaving the effect stuck in `executing` in storage
//! exactly as an unclean process termination would — this is the
//! "kill -9" the test models.
//!
//! This is not part of the shipped product; it exists only so the
//! integration test can exercise recovery against a genuinely killed OS
//! process instead of only simulating the leftover storage state in-process.

use evohime_local_storage::{
    LocalDatabase, RunCheckpointRecord, RunEffectRecord, RunRecord, WorkItemRecord,
};
use std::{io::Write, thread, time::Duration};

fn main() {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().expect("argv[1] = database path");
    let ready_marker_path = args.next().expect("argv[2] = ready marker path");
    let run_id = args.next().unwrap_or_else(|| "kill9-run".to_string());
    let effect_id = args.next().unwrap_or_else(|| "kill9-effect".to_string());
    let task_id = args.next().unwrap_or_else(|| "kill9-task".to_string());

    let database = LocalDatabase::open(&db_path).expect("database opens");
    database
        .create_project("project-kill9", "Kill-9 model", ".", None)
        .expect("project creates");
    let task = WorkItemRecord {
        id: task_id.clone(),
        project_id: "project-kill9".into(),
        parent_id: None,
        title: "kill-9 worker task".into(),
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
        id: run_id.clone(),
        work_item_id: task.id,
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
        input_hash: "kill9-intent".into(),
        state_json: br#"{"stage":"build"}"#.to_vec(),
        pending_effects_json: format!("[\"{effect_id}\"]").into_bytes(),
        committed_at: String::new(),
    };
    let effect = RunEffectRecord {
        effect_id: effect_id.clone(),
        run_id: run.id.clone(),
        node_id: "bounded-build".into(),
        kind: "bounded_build".into(),
        idempotency_key: format!("{run_id}:bounded-build"),
        immutable_intent_hash: "kill9-intent".into(),
        state: "prepared".into(),
        started_at: None,
        completed_at: None,
        result_hash: None,
    };
    database
        .prepare_run_effect(&run, &checkpoint, &effect)
        .expect("effect prepares");
    database
        .mark_effect_executing(&effect.effect_id)
        .expect("effect starts executing");

    // Signal readiness to the parent test process: the effect is now
    // durably recorded as `executing`, matching the state a real crash
    // mid-effect would leave behind.
    std::fs::write(&ready_marker_path, b"ready").expect("ready marker writes");
    let _ = std::io::stdout().flush();

    // Simulate "still doing work" until the parent forcibly kills this
    // process. If the parent's kill somehow fails, exit after a bounded
    // time so the helper never hangs a CI run indefinitely.
    thread::sleep(Duration::from_secs(120));
}
