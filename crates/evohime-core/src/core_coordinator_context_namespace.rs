use super::*;

fn redacted_error_code(error: &str) -> &'static str {
    if error.contains("version conflict") {
        "version_conflict"
    } else if error.contains("idempotency") {
        "idempotency_conflict"
    } else if error.contains("detail_resolver_unavailable") {
        "detail_resolver_unavailable"
    } else if error.contains("context_view") {
        "invalid_context_view"
    } else if error.contains("projection") {
        "invalid_projection"
    } else if error.contains("node") {
        "invalid_context_node"
    } else if error.contains("journal") {
        "storage_unavailable"
    } else {
        "context_namespace_failed"
    }
}

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    let CoreCommand::ContextNamespace {
        operation,
        namespace_id,
        payload,
        expected_revision,
        idempotency_key,
        reply,
    } = command
    else {
        return;
    };
    let event_operation = operation.clone();
    let event_namespace_id = namespace_id.clone();
    let result = async {
        let journal = state
            .lock()
            .await
            .journal
            .clone()
            .ok_or_else(|| "storage journal is not configured".to_string())?;
        journal
            .context_namespace_command(crate::context_namespace::NamespaceCommand {
                operation,
                namespace_id,
                payload,
                expected_revision,
                idempotency_key,
            })
            .await
            .map_err(|error| error.to_string())
    }
    .await;
    let projection_json = match &result {
        Ok(bytes) => String::from_utf8(bytes.clone()).unwrap_or_else(|_| {
            "{\"status\":\"error\",\"error_code\":\"invalid_projection_encoding\"}".into()
        }),
        Err(error) => serde_json::json!({
            "status": "error",
            "error_code": redacted_error_code(error),
        })
        .to_string(),
    };
    let event = CoreEvent::ContextNamespace {
        namespace_id: event_namespace_id,
        operation: event_operation,
        revision: expected_revision,
        projection_json,
    };
    let journal = state.lock().await.journal.clone();
    if let Some(journal) = journal {
        let _ = journal.record(&event).await;
    }
    TaskCoordinator::emit_state_event(&state, event).await;
    let _ = reply.send(result);
}
