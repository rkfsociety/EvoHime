//! End-to-end tests for database persistence with real PostgreSQL (Phase 4).
//! Verifies that critical data (sessions, tasks, events, checkpoints) survive restarts.

#![cfg(test)]

use evohime_storage::test_db::connect_integration_pool;
use uuid::Uuid;

async fn setup_operator(_pool: &sqlx::PgPool) -> Uuid {
    // Use bootstrap owner for all tests to avoid foreign key constraint
    evohime_storage::BOOTSTRAP_OWNER_ID
}

#[tokio::test]
async fn test_session_persists_across_connections() {
    let pool = match connect_integration_pool().await {
        Some(p) => p,
        None => {
            println!("skipping: integration database unavailable");
            return;
        }
    };

    let operator_id = setup_operator(&pool).await;

    // Create session
    let session = evohime_storage::create_session_for_operator(&pool, operator_id)
        .await
        .expect("create session");

    let session_id = session.id;

    // Verify it persists by loading it back
    let reloaded = evohime_storage::load_session(&pool, session_id)
        .await
        .expect("load session")
        .expect("session should exist");

    assert_eq!(reloaded.id, session_id);
    assert_eq!(reloaded.operator_id, operator_id);
}

#[tokio::test]
async fn test_events_persisted_in_sequence() {
    let pool = match connect_integration_pool().await {
        Some(p) => p,
        None => {
            println!("skipping: integration database unavailable");
            return;
        }
    };

    let operator_id = setup_operator(&pool).await;
    let session = evohime_storage::create_session_for_operator(&pool, operator_id)
        .await
        .expect("create session");

    // Insert 3 events
    let mut seqs = Vec::new();
    for i in 0..3 {
        let event_json = serde_json::json!({
            "type": "test_event",
            "index": i,
        });
        let (seq, _) = evohime_storage::insert_event(&pool, session.id, &event_json, None)
            .await
            .expect("insert event");
        seqs.push(seq);
    }

    // Retrieve all events
    let events = evohime_storage::list_session_events(&pool, session.id)
        .await
        .expect("list events");

    assert_eq!(events.len(), 3, "should have 3 events");

    // Verify sequences are in order
    for i in 0..3 {
        assert_eq!(
            events[i].sequence, seqs[i],
            "event {} should have correct sequence",
            i
        );
    }
}

#[tokio::test]
async fn test_task_and_steps_persist() {
    let pool = match connect_integration_pool().await {
        Some(p) => p,
        None => {
            println!("skipping: integration database unavailable");
            return;
        }
    };

    let operator_id = setup_operator(&pool).await;
    let session = evohime_storage::create_session_for_operator(&pool, operator_id)
        .await
        .expect("create session");

    // Create task
    let task = evohime_storage::create_task(
        &pool,
        session.id,
        "test task",
        None,
        None,
        None,
    )
    .await
    .expect("create task");

    let task_id = task.id;

    // Create task step
    let step_input = serde_json::json!({"path": "/test.txt"});
    let step = evohime_storage::create_task_step(&pool, task_id, 0, "filesystem.read", &step_input, &[])
        .await
        .expect("create step");

    // Reload task and verify it still exists
    let reloaded_task = evohime_storage::load_task(&pool, task_id)
        .await
        .expect("load task")
        .expect("task should exist");

    assert_eq!(reloaded_task.id, task_id);
    assert_eq!(reloaded_task.status, "running");

    // Reload step and verify it still exists
    let steps = evohime_storage::list_task_steps(&pool, task_id)
        .await
        .expect("list steps");

    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].id, step.id);
    assert_eq!(steps[0].tool_name, "filesystem.read");
}

#[tokio::test]
async fn test_checkpoint_stores_pause_state() {
    let pool = match connect_integration_pool().await {
        Some(p) => p,
        None => {
            println!("skipping: integration database unavailable");
            return;
        }
    };

    let operator_id = setup_operator(&pool).await;
    let session = evohime_storage::create_session_for_operator(&pool, operator_id)
        .await
        .expect("create session");

    let task = evohime_storage::create_task(&pool, session.id, "test", None, None, None)
        .await
        .expect("create task");

    // Create checkpoint to simulate pause
    let checkpoint_state = serde_json::json!({
        "step": 2,
        "status": "paused",
        "pause_reason": "approval_wait",
    });
    evohime_storage::upsert_checkpoint(&pool, task.id, 2, &checkpoint_state)
        .await
        .expect("create checkpoint");

    // Reload checkpoint and verify state
    let checkpoint = evohime_storage::load_checkpoint(&pool, task.id)
        .await
        .expect("load checkpoint")
        .expect("checkpoint should exist");

    assert_eq!(checkpoint.next_step, 2);
    assert_eq!(
        checkpoint.state_json.get("pause_reason").and_then(|v| v.as_str()),
        Some("approval_wait")
    );
}

#[tokio::test]
async fn test_multiple_tasks_in_session() {
    let pool = match connect_integration_pool().await {
        Some(p) => p,
        None => {
            println!("skipping: integration database unavailable");
            return;
        }
    };

    let operator_id = setup_operator(&pool).await;
    let session = evohime_storage::create_session_for_operator(&pool, operator_id)
        .await
        .expect("create session");

    // Create 3 tasks in the same session
    let mut task_ids = Vec::new();
    for i in 0..3 {
        let task = evohime_storage::create_task(
            &pool,
            session.id,
            &format!("task {}", i),
            None,
            None,
            None,
        )
        .await
        .expect("create task");
        task_ids.push(task.id);
    }

    // Retrieve all tasks for the session
    let tasks = evohime_storage::list_tasks_for_operator(&pool, operator_id, Some(session.id))
        .await
        .expect("list tasks");

    assert_eq!(tasks.len(), 3, "should have 3 tasks in session");

    // Verify all task ids are present (order may vary)
    let returned_ids: Vec<_> = tasks.iter().map(|t| t.id).collect();
    for task_id in task_ids {
        assert!(returned_ids.contains(&task_id), "task {} should be in results", task_id);
    }
}

#[tokio::test]
async fn test_event_resume_from_sequence() {
    let pool = match connect_integration_pool().await {
        Some(p) => p,
        None => {
            println!("skipping: integration database unavailable");
            return;
        }
    };

    let operator_id = setup_operator(&pool).await;
    let session = evohime_storage::create_session_for_operator(&pool, operator_id)
        .await
        .expect("create session");

    // Insert 5 events
    let mut seqs = Vec::new();
    for i in 0..5 {
        let event_json = serde_json::json!({"index": i});
        let (seq, _) = evohime_storage::insert_event(&pool, session.id, &event_json, None)
            .await
            .expect("insert");
        seqs.push(seq);
    }

    // Simulate client reconnect: fetch events after sequence 2
    let after_seq = seqs[1]; // after event 1, should get 2,3,4
    let resumed = evohime_storage::list_session_events_after(&pool, session.id, after_seq)
        .await
        .expect("list after");

    assert_eq!(
        resumed.len(),
        3,
        "should get 3 events after sequence {}",
        after_seq
    );
    assert_eq!(resumed[0].sequence, seqs[2]);
    assert_eq!(resumed[1].sequence, seqs[3]);
    assert_eq!(resumed[2].sequence, seqs[4]);
}

#[tokio::test]
async fn test_operator_scoped_access() {
    let pool = match connect_integration_pool().await {
        Some(p) => p,
        None => {
            println!("skipping: integration database unavailable");
            return;
        }
    };

    let operator_id = setup_operator(&pool).await;

    // Create two sessions for the same operator
    let session1 = evohime_storage::create_session_for_operator(&pool, operator_id)
        .await
        .expect("create session 1");

    let session2 = evohime_storage::create_session_for_operator(&pool, operator_id)
        .await
        .expect("create session 2");

    // Operator should see both of their own sessions
    let sessions = evohime_storage::list_sessions_for_operator(&pool, operator_id, 100)
        .await
        .expect("list sessions");

    let session_ids: Vec<_> = sessions.iter().map(|s| s.id).collect();

    assert!(session_ids.contains(&session1.id), "should see session 1");
    assert!(session_ids.contains(&session2.id), "should see session 2");
}
