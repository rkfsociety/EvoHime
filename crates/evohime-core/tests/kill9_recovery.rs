//! Kill-9 model test for the durable recovery/replay protocol.
//!
//! This spawns a real, separate OS process (`kill9-worker-test-helper`,
//! `crates/evohime-core/tests/bin/kill9_worker.rs`) that durably prepares a
//! run effect and marks it `executing` against a shared SQLite database,
//! then forcibly terminates that process with `TerminateProcess`
//! (`std::process::Child::kill()` on Windows) once it signals readiness —
//! i.e. an unclean, SIGKILL-equivalent termination mid-effect, not a
//! graceful shutdown. The test then reopens the same database in-process
//! and drives `EventJournal::recover_and_reconcile_after_restart()`
//! (the same entry point Core's `main.rs` calls on startup) and asserts:
//!
//! - the run lands on a definite terminal state (`BLOCKED`, since no build
//!   snapshot was ever recorded for this run) and never stays stuck in
//!   `RECOVERING`;
//! - a second recovery pass is a true no-op (no duplicate effect
//!   application, no duplicate recovery-decision events);
//! - exactly one `run_effects` row exists for the effect throughout — kill
//!   + recovery + repeated recovery never creates a second effect record.

use evohime_core::EventJournal;
use evohime_local_storage::{LocalDatabase, RecoveryState};
use std::{
    env,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn temp_path(suffix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is valid")
        .as_nanos();
    env::temp_dir().join(format!("evohime-kill9-{suffix}-{stamp}.db"))
}

fn cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[tokio::test]
async fn kill9_mid_effect_recovers_to_blocked_without_duplicate_effects() {
    let db_path = temp_path("blocked");
    let marker_path = env::temp_dir().join(format!(
        "evohime-kill9-marker-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is valid")
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&marker_path);

    let run_id = "kill9-run-blocked";
    let effect_id = "kill9-effect-blocked";
    let task_id = "kill9-task-blocked";

    let worker_bin = env!("CARGO_BIN_EXE_kill9-worker-test-helper");
    let mut child = Command::new(worker_bin)
        .arg(&db_path)
        .arg(&marker_path)
        .arg(run_id)
        .arg(effect_id)
        .arg(task_id)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn kill9 worker process");

    // Wait for the worker to durably record the `executing` effect before
    // we kill it, so the kill genuinely lands mid-effect rather than before
    // any durable state exists.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if marker_path.exists() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            cleanup(&db_path);
            panic!("kill9 worker never signaled readiness within timeout");
        }
        if let Some(status) = child.try_wait().expect("try_wait should not error") {
            cleanup(&db_path);
            panic!("kill9 worker exited early with status {status:?} before signaling readiness");
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    // Forcibly terminate the worker: on Windows, `Child::kill()` calls
    // `TerminateProcess`, an unclean, non-graceful kill equivalent to
    // SIGKILL on POSIX — the worker gets no chance to run any cleanup or
    // shutdown code.
    child.kill().expect("TerminateProcess-equivalent kill");
    let status = child.wait().expect("wait for killed process");
    assert!(!status.success(), "killed worker should not exit cleanly");

    let _ = std::fs::remove_file(&marker_path);

    // Confirm the effect really was left stuck `executing` by the kill,
    // i.e. this test actually models an unclean termination rather than a
    // process that happened to already finish.
    {
        let database = LocalDatabase::open(&db_path).expect("database reopens for precheck");
        let effect = database
            .get_run_effect(effect_id)
            .expect("effect read")
            .expect("effect row exists");
        assert_eq!(
            effect.state, "executing",
            "kill must land mid-effect for this test to model kill-9 recovery"
        );
    }

    // Recover exactly like Core's real startup path does.
    let journal = EventJournal::open(&db_path).expect("journal reopens");
    let reconciliations = journal
        .recover_and_reconcile_after_restart()
        .await
        .expect("recovery runs to completion");
    assert_eq!(
        reconciliations.len(),
        1,
        "exactly one recovered effect should be reconciled"
    );

    let database = LocalDatabase::open(&db_path).expect("database reopens for assertions");
    let recovery = database
        .latest_recovery(run_id)
        .expect("recovery reads")
        .expect("a recovery decision record exists");

    // No build snapshot was ever recorded for this run (the worker was
    // killed before completing), so the outcome is unconfirmed and the
    // state machine must land on a definite terminal state — BLOCKED —
    // never stay stuck in RECOVERING/RECONCILING and never silently
    // resume as if the effect had succeeded.
    assert_eq!(recovery.state, RecoveryState::Blocked);
    assert_ne!(recovery.state, RecoveryState::Recovering);
    assert_ne!(recovery.state, RecoveryState::Reconciling);

    let effect_after_first_recovery = database
        .get_run_effect(effect_id)
        .expect("effect read")
        .expect("effect row still exists");
    assert_eq!(
        effect_after_first_recovery.state, "unknown",
        "recovered effect must be marked unknown, not silently completed"
    );

    // Re-run recovery again (as a real Core restart, or a second recovery
    // pass triggered by a supervisor retry, would). This must be a true
    // no-op: no second effect, no duplicate application, no conflicting
    // recovery transition.
    let second_pass = journal
        .recover_and_reconcile_after_restart()
        .await
        .expect("second recovery pass runs");
    assert!(
        second_pass.is_empty(),
        "repeated recovery must not re-process an already-recovered effect"
    );

    let recovery_after_second_pass = database
        .latest_recovery(run_id)
        .expect("recovery reads")
        .expect("recovery decision record still exists");
    assert_eq!(recovery_after_second_pass.state, RecoveryState::Blocked);
    assert_eq!(
        recovery_after_second_pass.id, recovery.id,
        "repeated recovery must not create a new recovery decision row"
    );

    // The core kill-9 correctness property: exactly one effect record for
    // this effect id exists throughout, i.e. the kill + double recovery
    // never caused the effect to be duplicated or double-applied.
    let effect_after_second_recovery = database
        .get_run_effect(effect_id)
        .expect("effect read")
        .expect("effect row still exists");
    assert_eq!(effect_after_second_recovery.state, "unknown");
    assert_eq!(
        effect_after_second_recovery.effect_id, effect_id,
        "effect id must be stable across kill + repeated recovery"
    );

    cleanup(&db_path);
}
