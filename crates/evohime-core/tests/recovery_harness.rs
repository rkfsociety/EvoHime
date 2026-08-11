use evohime_local_storage::{
    LocalDatabase, RunCheckpointRecord, RunEffectRecord, RunRecord, WorkItemRecord,
};
use std::{
    env,
    fs,
    path::PathBuf,
    process::{Child, Command},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const CHILD_ENV: &str = "EVOHIME_RECOVERY_HARNESS_CHILD";
const DATABASE_ENV: &str = "EVOHIME_RECOVERY_HARNESS_DATABASE";
const MARKER_ENV: &str = "EVOHIME_RECOVERY_HARNESS_MARKER";

fn temp_path(suffix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is valid")
        .as_nanos();
    env::temp_dir().join(format!("evohime-recovery-harness-{suffix}-{stamp}.db"))
}

fn seed_executing_effect(database_path: &PathBuf) {
    let database = LocalDatabase::open(database_path).expect("database opens");
    database
        .create_project("project-harness", "Recovery harness", ".", None)
        .expect("project creates");
    let task = WorkItemRecord {
        id: "task-harness".into(),
        project_id: "project-harness".into(),
        parent_id: None,
        title: "recovery harness".into(),
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
        id: "run-harness".into(),
        work_item_id: task.id,
        status: "running".into(),
        policy_snapshot: Vec::new(),
        role_snapshot: Vec::new(),
        skill_snapshot: Vec::new(),
        model_route_snapshot: Vec::new(),
    };
    let checkpoint = RunCheckpointRecord {
        run_id: run.id.clone(),
        checkpoint_id: "checkpoint-harness".into(),
        stage: "build".into(),
        node_id: "bounded-build".into(),
        attempt: 1,
        input_hash: "harness-intent".into(),
        state_json: br#"{"stage":"build"}"#.to_vec(),
        pending_effects_json: br#"["effect-harness"]"#.to_vec(),
        committed_at: String::new(),
    };
    let effect = RunEffectRecord {
        effect_id: "effect-harness".into(),
        run_id: run.id.clone(),
        node_id: "bounded-build".into(),
        kind: "bounded_build".into(),
        idempotency_key: "run-harness:bounded-build".into(),
        immutable_intent_hash: "harness-intent".into(),
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
        .expect("effect starts");
}

fn run_until_killed(mut child: Child, marker: &PathBuf) {
    for _ in 0..50 {
        if marker.exists() {
            child.kill().expect("child kills");
            let _ = child.wait();
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    panic!("recovery child did not reach executing effect");
}

#[test]
fn kill_restart_recovers_unknown_effect_without_retry() {
    if env::var_os(CHILD_ENV).is_some() {
        let database = PathBuf::from(env::var_os(DATABASE_ENV).expect("database path"));
        let marker = PathBuf::from(env::var_os(MARKER_ENV).expect("marker path"));
        seed_executing_effect(&database);
        fs::write(marker, b"executing").expect("marker writes");
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }

    let database = temp_path("child");
    let marker = database.with_extension("ready");
    let child = Command::new(env::current_exe().expect("test executable"))
        .arg("kill_restart_recovers_unknown_effect_without_retry")
        .env(CHILD_ENV, "1")
        .env(DATABASE_ENV, &database)
        .env(MARKER_ENV, &marker)
        .spawn()
        .expect("recovery child starts");
    run_until_killed(child, &marker);

    let database_handle = LocalDatabase::open(&database).expect("database reopens");
    let recovered = database_handle
        .recover_unknown_effects()
        .expect("unknown effect recovers");
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        database_handle.get_run("run-harness").expect("run reads").expect("run exists").status,
        "blocked"
    );
    assert!(database_handle.recover_unknown_effects().expect("recovery repeats").is_empty());

    let _ = fs::remove_file(&marker);
    let _ = fs::remove_file(&database);
    let _ = fs::remove_file(database.with_extension("db-wal"));
    let _ = fs::remove_file(database.with_extension("db-shm"));
}
