impl IpcBridge {
    pub(crate) async fn dispatch_artifact_handoff_registry(
        &self,
        request: generated::ArtifactHandoffRegistryCommand,
    ) -> serde_json::Value {
        use crate::artifact_handoff_registry::{validate, ArtifactState, ProjectArtifactRevision};
        let invalid = |code: &str| serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","artifact_id":"","revision":0,"state":"","error_code":code,"projection_json":{"raw_payload":false}});
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.project_id.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return invalid("invalid_request");
        }
        let database = self.journal.database().lock().await;
        let operation = request.operation.as_str();
        let now = chrono::Utc::now().timestamp_millis();
        if !request.idempotency_key.is_empty() {
            if let Ok(Some(outcome)) =
                evohime_local_storage::artifact_handoff_registry_store::command_outcome(
                    database.connection(),
                    &request.idempotency_key,
                )
            {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&outcome) {
                    return value;
                }
            }
        }
        let mut artifact_id = String::new();
        let mut revision = 0_u64;
        let mut state = String::new();
        let result: Result<serde_json::Value, &str> = (|| match operation {
            "list" => {
                let rows = evohime_local_storage::artifact_handoff_registry_store::list(
                    database.connection(),
                    &request.project_id,
                    128,
                )
                .map_err(|_| "storage_error")?;
                Ok(
                    serde_json::json!({"artifacts":rows.into_iter().map(|r| serde_json::json!({"artifact_id":r.artifact_id,"revision":r.revision,"state":r.state,"content_hash":r.content_hash,"raw_payload":false})).collect::<Vec<_>>(),"truncated":false}),
                )
            }
            "publish" => {
                let revision_value: ProjectArtifactRevision =
                    serde_json::from_slice(&request.payload).map_err(|_| "invalid_payload")?;
                validate(&revision_value).map_err(|_| "invalid_artifact")?;
                if revision_value.project_id != request.project_id {
                    return Err("scope_mismatch");
                }
                artifact_id = revision_value.artifact_id.clone();
                revision = revision_value.revision;
                state = revision_value.state.as_str().into();
                if revision_value.revision == 0 {
                    return Err("invalid_revision");
                }
                if database.connection().query_row("SELECT COUNT(*) FROM task_artifact_refs WHERE locator=?1 AND content_hash=?2", rusqlite::params![revision_value.content_locator, revision_value.content_hash], |r| r.get::<_, i64>(0)).map_err(|_| "storage_error")? != 1 { return Err("artifact_ref_not_found"); }
                let metadata =
                    serde_json::to_vec(&revision_value.metadata).map_err(|_| "invalid_payload")?;
                let row = evohime_local_storage::artifact_handoff_registry_store::RegistryRow {
                    artifact_id: revision_value.artifact_id.clone(),
                    project_id: revision_value.project_id.clone(),
                    revision: revision_value.revision,
                    state: revision_value.state.as_str().into(),
                    content_locator: revision_value.content_locator.clone(),
                    content_hash: revision_value.content_hash.clone(),
                    metadata_json: metadata,
                    created_at_ms: now,
                };
                evohime_local_storage::artifact_handoff_registry_store::insert_revision_atomic(
                    database.connection(),
                    &row,
                    &[],
                )
                .map_err(|_| "duplicate_or_storage_error")?;
                Ok(
                    serde_json::json!({"artifact_id":artifact_id,"revision":revision,"state":state,"content_hash":revision_value.content_hash,"raw_payload":false}),
                )
            }
            "get" => {
                let input: ArtifactRevisionPayload =
                    serde_json::from_slice(&request.payload).map_err(|_| "invalid_payload")?;
                let row = evohime_local_storage::artifact_handoff_registry_store::get(
                    database.connection(),
                    &input.artifact_id,
                    input.revision,
                )
                .map_err(|_| "storage_error")?
                .ok_or("not_found")?;
                artifact_id = row.artifact_id;
                revision = row.revision;
                state = row.state.clone();
                Ok(
                    serde_json::json!({"artifact_id":artifact_id,"revision":revision,"state":state,"content_hash":row.content_hash,"metadata":serde_json::from_slice::<serde_json::Value>(&row.metadata_json).unwrap_or(serde_json::json!({})),"raw_payload":false}),
                )
            }
            "mark_stale" | "accept" | "revise" => {
                let input: ArtifactRevisionPayload =
                    serde_json::from_slice(&request.payload).map_err(|_| "invalid_payload")?;
                artifact_id = input.artifact_id;
                revision = input.revision;
                if request.expected_revision != revision {
                    return Err("stale_revision");
                }
                let target = if operation == "mark_stale" {
                    ArtifactState::Stale.as_str()
                } else if operation == "accept" {
                    ArtifactState::Accepted.as_str()
                } else {
                    ArtifactState::NeedsRevision.as_str()
                };
                if !evohime_local_storage::artifact_handoff_registry_store::transition(
                    database.connection(),
                    &artifact_id,
                    revision,
                    target,
                )
                .map_err(|_| "storage_error")?
                {
                    return Err("not_found_or_stale");
                }
                state = target.into();
                Ok(
                    serde_json::json!({"artifact_id":artifact_id,"revision":revision,"state":state,"raw_payload":false}),
                )
            }
            "handoff" => {
                let input: ArtifactHandoffPayload =
                    serde_json::from_slice(&request.payload).map_err(|_| "invalid_payload")?;
                artifact_id = input.artifact_id;
                revision = input.revision;
                evohime_local_storage::artifact_handoff_registry_store::insert_handoff(
                    database.connection(),
                    &input.handoff_id,
                    &artifact_id,
                    revision,
                    &input.producer_identity,
                    &input.consumer_identity,
                    now,
                )
                .map_err(|_| "duplicate_or_storage_error")?;
                Ok(
                    serde_json::json!({"artifact_id":artifact_id,"revision":revision,"state":"handoff_pending","handoff_id":input.handoff_id,"raw_payload":false}),
                )
            }
            _ => Err("unsupported_operation"),
        })();
        let (status, error_code, projection) = match result {
            Ok(v) => ("ok", "", v),
            Err(e) => ("rejected", e, serde_json::json!({"raw_payload":false})),
        };
        let response = serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":status,"artifact_id":artifact_id,"revision":revision,"state":state,"error_code":error_code,"projection_json":projection});
        if !request.idempotency_key.is_empty() {
            if let Ok(bytes) = serde_json::to_vec(&response) {
                let digest = format!(
                    "sha256:{}",
                    hex::encode(sha2::Sha256::digest(&request.payload))
                );
                let _ = evohime_local_storage::artifact_handoff_registry_store::record_command(
                    database.connection(),
                    &request.idempotency_key,
                    &request.correlation_id,
                    operation,
                    &digest,
                    &bytes,
                    now,
                );
            }
        }
        response
    }

    pub(crate) async fn write_artifact_handoff_registry_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value = serde_json::from_slice(&payload)?;
        let result = generated::ArtifactHandoffRegistryEvent {
            schema_version: 1,
            request_id: value["request_id"].as_str().unwrap_or_default().into(),
            operation: value["operation"].as_str().unwrap_or_default().into(),
            status: value["status"].as_str().unwrap_or_default().into(),
            artifact_id: value["artifact_id"].as_str().unwrap_or_default().into(),
            revision: value["revision"].as_u64().unwrap_or_default(),
            state: value["state"].as_str().unwrap_or_default().into(),
            error_code: value["error_code"].as_str().unwrap_or_default().into(),
            projection_json: serde_json::to_vec(&value["projection_json"])?,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "artifact_handoff_registry.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::ArtifactHandoffRegistry(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }
}
