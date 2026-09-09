use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    let CoreCommand::DurableBackgroundExecution { operation, run_id, owner_scope, payload, expected_revision, idempotency_key, reply } = command else { return; };
    let event_operation = operation.clone();
    let event_run_id = run_id.clone();
    let result = async {
        let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
        journal.durable_background_command(&operation, &run_id, &owner_scope, &payload, expected_revision, &idempotency_key).await
    }.await;
    let projection = match &result {
        Ok(value) => String::from_utf8(value.clone()).unwrap_or_else(|_| "{\"status\":\"error\",\"error_code\":\"invalid_projection_encoding\"}".into()),
        Err(error) => {
            let journal = state.lock().await.journal.clone();
            if let Some(journal) = journal { String::from_utf8(journal.durable_background_error_projection(error).await).unwrap_or_else(|_| "{\"status\":\"error\"}".into()) } else { "{\"status\":\"error\",\"error_code\":\"storage_unavailable\"}".into() }
        }
    };
    let event = CoreEvent::DurableBackgroundExecution { run_id: event_run_id, operation: event_operation, revision: expected_revision, projection_json: projection };
    let journal = state.lock().await.journal.clone();
    if let Some(journal) = journal { let _ = journal.record(&event).await; }
    TaskCoordinator::emit_state_event(&state, event).await;
    let _ = reply.send(result);
}
