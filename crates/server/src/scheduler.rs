//! Cron scheduler: fires due scheduled tasks by creating agent sessions.
//!
//! Loop runs every 30 seconds, fetches tasks where `next_run_at <= now()`,
//! creates a session + task for each, then advances `next_run_at`.

use crate::app::AppState;
use chrono::Utc;
use cron::Schedule;
use evohime_storage::scheduled::{due_scheduled_tasks, record_scheduled_task_run};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Parse a cron expression and return the next scheduled time after now.
pub fn next_run_after_now(cron_expr: &str) -> Result<chrono::DateTime<Utc>, String> {
    let schedule = Schedule::from_str(cron_expr)
        .map_err(|e| format!("invalid cron expression '{cron_expr}': {e}"))?;
    schedule
        .upcoming(Utc)
        .next()
        .ok_or_else(|| format!("cron expression '{cron_expr}' yields no future times"))
}

/// Validate a cron expression without running it.
pub fn validate_cron(cron_expr: &str) -> Result<(), String> {
    Schedule::from_str(cron_expr)
        .map(|_| ())
        .map_err(|e| format!("invalid cron expression: {e}"))
}

/// Background loop. Fires due tasks every `interval`.
pub async fn scheduler_loop(state: Arc<AppState>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        if let Err(e) = tick(&state).await {
            error!(error = %e, "scheduler tick error");
        }
    }
}

async fn tick(state: &Arc<AppState>) -> anyhow::Result<()> {
    let due = due_scheduled_tasks(&state.pool).await?;
    if due.is_empty() {
        return Ok(());
    }
    for task in due {
        let next = match next_run_after_now(&task.cron_expr) {
            Ok(t) => t,
            Err(e) => {
                warn!(id = %task.id, error = %e, "skipping task with bad cron; pausing it");
                let _ =
                    evohime_storage::scheduled::set_scheduled_task_status(&state.pool, task.id, &task.workspace_path, "paused").await;
                continue;
            }
        };

        info!(
            id = %task.id,
            title = %task.title,
            ?next,
            "firing scheduled task"
        );

        // Create a session + task through the normal task pipeline.
        let workspace_path = task.workspace_path.clone();
        let prompt = task.prompt.clone();
        let task_id = task.id;
        let state2 = state.clone();
        tokio::spawn(async move {
            if let Err(e) = fire_scheduled_task(&state2, &workspace_path, &prompt).await {
                error!(id = %task_id, error = %e, "failed to fire scheduled task");
            }
        });

        if let Err(e) = record_scheduled_task_run(&state.pool, task.id, next).await {
            error!(id = %task.id, error = %e, "failed to record scheduled task run");
        }
    }
    Ok(())
}

pub(crate) async fn fire_scheduled_task_pub(
    state: &Arc<AppState>,
    workspace_path: &str,
    prompt: &str,
) -> anyhow::Result<()> {
    fire_scheduled_task(state, workspace_path, prompt).await
}

async fn fire_scheduled_task(
    state: &Arc<AppState>,
    workspace_path: &str,
    prompt: &str,
) -> anyhow::Result<()> {
    use crate::task::{emit_event, process_user_message};
    use evohime_protocol::ServerEvent;
    use evohime_task_engine::fail_task;

    let session = evohime_storage::create_session(&state.pool).await?;
    let session_id = session.id;

    let task_row = evohime_storage::create_task(
        &state.pool,
        session_id,
        prompt,
        None,
        None,
        Some(workspace_path),
    )
    .await?;

    let ts = Utc::now();
    let _ = emit_event(
        state,
        session_id,
        Some(task_row.id),
        ServerEvent::TaskStarted {
            task_id: task_row.id,
            session_id,
            user_message: task_row.user_message.clone(),
            created_at: ts,
        },
    )
    .await;

    let token = tokio_util::sync::CancellationToken::new();
    state
        .task_cancellations
        .lock()
        .await
        .insert(task_row.id, token.clone());

    let state2 = state.clone();
    let registered_task_id = task_row.id;
    tokio::spawn(async move {
        if let Err((failed_id, err)) =
            process_user_message(&state2, session_id, task_row, token).await
        {
            let _ = emit_event(
                &state2,
                session_id,
                Some(failed_id),
                ServerEvent::TaskFailed {
                    task_id: failed_id,
                    error: err.to_string(),
                },
            )
            .await;
            let _ = fail_task(&state2.pool, failed_id).await;
        }
        state2.task_cancellations.lock().await.remove(&registered_task_id);
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{next_run_after_now, validate_cron};

    #[test]
    fn validate_cron_accepts_six_field_with_seconds() {
        // cron crate v0.17 uses 6-field: sec min hour day-of-month month day-of-week
        assert!(validate_cron("0 0 8 * * 1-5").is_ok());
        assert!(validate_cron("0 * * * * *").is_ok());
    }

    #[test]
    fn validate_cron_rejects_garbage() {
        assert!(validate_cron("not a cron").is_err());
    }

    #[test]
    fn next_run_after_now_returns_future_time() {
        // Every second
        let next = next_run_after_now("* * * * * *").unwrap();
        assert!(next > chrono::Utc::now());
    }
}
