use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::ClearTaskScratchpad { task_id, reply } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let removed = journal
                    .clear_task_scratchpad(&task_id)
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({ "removed": removed }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::SummarizeContextNow { task_id, reply } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                journal
                    .request_context_summarize(&task_id)
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "requested": true,
                    "scope": "task_context",
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::PinContextItem {
            task_id,
            item_id,
            pinned,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                journal
                    .set_context_pin(&task_id, &item_id, pinned)
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "item_id": item_id,
                    "pinned": pinned,
                    // Pin повышает приоритет, но не гарантирует включение:
                    // при нехватке бюджета item отбрасывается последним.
                    "guaranteed": false,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ReadContextArtifact {
            task_id,
            locator,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let content = journal
                    .read_context_artifact(&task_id, &locator)
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "locator": locator,
                    "content": content,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}
