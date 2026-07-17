//! Integration scenarios for task lifecycle (roadmap P1).
//!
//! Covers: start → pause → resume → complete, fail → retry,
//! and recover_after_restart. Skips when Postgres is unavailable.

use evohime_task_engine::{
    complete_task, fail_task, pause_task, recover_after_restart, resume_task, retry_task, start_task,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

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

    let running = start_task(
        &pool,
        session.id,
        "integration recovery",
        None,
        None,
        None,
    )
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

    let resumed = resume_task(&pool, running.id).await.expect("resume after recovery");
    assert_eq!(resumed.status, "running");
    complete_task(&pool, running.id).await.expect("complete");
    complete_task(&pool, failed_task.id).await.expect("complete retried");
}
