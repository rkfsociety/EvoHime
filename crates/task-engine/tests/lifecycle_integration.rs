//! Integration scenarios for task lifecycle (roadmap P1).
//!
//! Covers: start → pause → resume → complete, fail → retry,
//! and recover_after_restart. Skips when Postgres is unavailable.

use evohime_protocol::TaskStatus;
use evohime_task_engine::{
    cancel_task, complete_task, fail_task, pause_task, recover_after_restart, resume_task,
    retry_task, start_task, transition, TaskEngineError,
};
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::Mutex;
use uuid::Uuid;

static TEST_MUTEX: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

async fn connect_pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://evohime:evohime@localhost:5432/evohime".into());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .ok()?;
    evohime_storage::run_migrations(&pool).await.ok()?;
    Some(pool)
}

#[tokio::test]
async fn start_pause_resume_complete_flow() {
    let _guard = TEST_MUTEX.lock().await;
    let Some(pool) = connect_pool().await else {
        eprintln!("skipping task-engine integration test: database unavailable");
        return;
    };

    let session = evohime_storage::create_session(&pool)
        .await
        .expect("session");
    let task = start_task(
        &pool,
        session.id,
        "integration pause/resume",
        None,
        None,
        None,
    )
    .await
    .expect("start");
    assert_eq!(task.status, "running");

    let paused = pause_task(&pool, task.id).await.expect("pause");
    assert_eq!(paused.status, "paused");

    evohime_storage::merge_checkpoint(
        &pool,
        task.id,
        None,
        &json!({
            "pause_reason": "approval_required",
            "approval_wait": {
                "approval_id": Uuid::new_v4(),
                "tool_name": "filesystem.write",
            }
        }),
    )
    .await
    .expect("checkpoint");

    let checkpoint = evohime_storage::load_checkpoint(&pool, task.id)
        .await
        .expect("load checkpoint")
        .expect("checkpoint row");
    assert_eq!(
        checkpoint.state_json["pause_reason"],
        json!("approval_required")
    );

    let resumed = resume_task(&pool, task.id).await.expect("resume");
    assert_eq!(resumed.status, "running");

    let completed = complete_task(&pool, task.id).await.expect("complete");
    assert_eq!(completed.status, "completed");
}

#[tokio::test]
async fn fail_retry_and_recover_running_after_restart() {
    let _guard = TEST_MUTEX.lock().await;
    let Some(pool) = connect_pool().await else {
        eprintln!("skipping task-engine integration test: database unavailable");
        return;
    };

    let session = evohime_storage::create_session(&pool)
        .await
        .expect("session");
    let failed_task = start_task(&pool, session.id, "integration retry", None, None, None)
        .await
        .expect("start");
    fail_task(&pool, failed_task.id).await.expect("fail");
    let retried = retry_task(&pool, failed_task.id).await.expect("retry");
    assert_eq!(retried.status, "running");

    let running = start_task(&pool, session.id, "integration recovery", None, None, None)
        .await
        .expect("start running");
    assert_eq!(running.status, "running");

    let recovered = recover_after_restart(&pool).await.expect("recover");
    assert!(
        recovered.iter().any(|task| task.id == running.id),
        "running task should be paused by recovery"
    );
    let loaded = evohime_storage::load_task(&pool, running.id)
        .await
        .expect("load")
        .expect("task");
    assert_eq!(loaded.status, "paused");

    let resumed = resume_task(&pool, running.id)
        .await
        .expect("resume after recovery");
    assert_eq!(resumed.status, "running");
    complete_task(&pool, running.id).await.expect("complete");

    let retried_after_recovery = resume_task(&pool, failed_task.id)
        .await
        .expect("resume retried task after recovery");
    assert_eq!(retried_after_recovery.status, "running");
    complete_task(&pool, failed_task.id)
        .await
        .expect("complete retried");
}

#[tokio::test]
async fn cancel_via_fsm_and_reject_illegal_transition() {
    let _guard = TEST_MUTEX.lock().await;
    let Some(pool) = connect_pool().await else {
        eprintln!("skipping task-engine integration test: database unavailable");
        return;
    };

    let session = evohime_storage::create_session(&pool)
        .await
        .expect("session");
    let running = start_task(&pool, session.id, "integration cancel", None, None, None)
        .await
        .expect("start");
    let cancelled = cancel_task(&pool, running.id).await.expect("cancel");
    assert_eq!(cancelled.status, "cancelled");

    let err = transition(&pool, running.id, TaskStatus::Completed)
        .await
        .expect_err("completed from cancelled must fail");
    assert!(matches!(err, TaskEngineError::InvalidTransition { .. }));

    let paused = start_task(
        &pool,
        session.id,
        "integration cancel paused",
        None,
        None,
        None,
    )
    .await
    .expect("start paused path");
    pause_task(&pool, paused.id).await.expect("pause");
    let cancelled_paused = cancel_task(&pool, paused.id).await.expect("cancel paused");
    assert_eq!(cancelled_paused.status, "cancelled");

    let missing = Uuid::new_v4();
    let not_found = cancel_task(&pool, missing).await.expect_err("missing task");
    assert!(matches!(not_found, TaskEngineError::NotFound(id) if id == missing));
}
