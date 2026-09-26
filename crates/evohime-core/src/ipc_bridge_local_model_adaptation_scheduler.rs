use super::*;

impl IpcBridge {
    /// Retries durable adaptation jobs waiting for local resources in FIFO order.
    ///
    /// The normal Core start path revalidates source identity, host pressure,
    /// disk reservations and Supervisor capacity before dispatching a process.
    pub(crate) async fn dispatch_next_waiting_adaptation(&self) -> Result<bool, String> {
        let rows = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::local_model_adaptation_store::list_jobs(
                database.connection(),
                crate::local_model_adaptation::MAX_JOBS as u32,
            )
            .map_err(|_| "adaptation_queue_read_failed".to_string())?
        };
        let coordinator = self.coordinator.as_ref()
            .ok_or_else(|| "adaptation_queue_coordinator_unavailable".to_string())?;
        for (job_id, _, state, snapshot) in rows {
            if state != crate::local_model_adaptation::AdaptationState::WaitingForResources.storage_key() {
                continue;
            }
            let job: crate::local_model_adaptation::AdaptationJob =
                serde_json::from_slice(&snapshot).map_err(|_| "adaptation_queue_corrupt_job".to_string())?;
            job.validate().map_err(|_| "adaptation_queue_corrupt_job".to_string())?;
            if job.request.job_id != job_id
                || job.state != crate::local_model_adaptation::AdaptationState::WaitingForResources
            {
                return Err("adaptation_queue_identity_mismatch".into());
            }
            let (reply, response) = oneshot::channel();
            coordinator.dispatch(CoreCommand::LocalModelRuntimeManager {
                operation: "adaptation_start".into(),
                payload: serde_json::to_vec(&serde_json::json!({"job_id":job_id}))
                    .map_err(|_| "adaptation_queue_request_failed".to_string())?,
                expected_version: 0,
                idempotency_key: job.request.idempotency_key,
                reply,
            }).await.map_err(|_| "adaptation_queue_dispatch_failed".to_string())?;
            let result = response.await.map_err(|_| "adaptation_queue_response_lost".to_string())?;
            let projection = match result {
                Ok(projection) => projection,
                Err(error) if error == "adaptation_start_state_conflict" => continue,
                Err(_) => {
                    let database = self.journal.database().lock().await;
                    evohime_local_storage::local_model_adaptation_store::defer_waiting_job(
                        database.connection(),
                        &job_id,
                        crate::task_memory::now_millis() as i64,
                    ).map_err(|_| "adaptation_queue_defer_failed".to_string())?;
                    return Ok(false);
                }
            };
            let status = serde_json::from_slice::<serde_json::Value>(&projection)
                .ok().and_then(|value| value.get("status").and_then(serde_json::Value::as_str).map(str::to_owned));
            if status.as_deref() == Some("running") {
                return Ok(true);
            }
            if status.as_deref() == Some("waiting_for_resources") {
                let database = self.journal.database().lock().await;
                evohime_local_storage::local_model_adaptation_store::defer_waiting_job(
                    database.connection(),
                    &job_id,
                    crate::task_memory::now_millis() as i64,
                ).map_err(|_| "adaptation_queue_defer_failed".to_string())?;
            }
            return Ok(false);
        }
        Ok(false)
    }
}
