use super::*;

impl IpcBridge {
    pub(crate) async fn dispatch_agentic_browser_session(
        &self,
        request: generated::AgenticBrowserSessionCommand,
    ) -> serde_json::Value {
        use crate::agentic_browser_session::{
            BrowserSession, ControlOwner, SessionState, CONTRACT_VERSION,
        };
        let rejected = |code: &str| serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":code,"projection_json":{"raw_payload":false,"credentials":false,"cdp_endpoint":false}});
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return rejected("invalid_request");
        }
        let payload_json: serde_json::Value = match serde_json::from_slice(&request.payload) {
            Ok(value) => value,
            Err(_) => return rejected("invalid_payload"),
        };
        let payload: AgenticBrowserSessionPayload =
            match serde_json::from_value(payload_json.clone()) {
                Ok(value) => value,
                Err(_) => return rejected("invalid_payload"),
            };
        if matches!(
            request.operation.as_str(),
            "legacy" | "legacy_selector" | "raw_cdp"
        ) {
            return rejected("legacy_disabled");
        }
        let database = self.journal.database();
        let now = chrono::Utc::now().timestamp_millis();
        let (mut session, is_create) = if request.operation == "create" {
            let conversation_id = payload
                .conversation_id
                .as_deref()
                .unwrap_or(request.owner_scope.as_str());
            let Ok(mut session) = BrowserSession::new(
                conversation_id,
                payload.run_id,
                payload.policy_hash.as_deref().unwrap_or("unknown"),
            ) else {
                return rejected("invalid_scope");
            };
            if session.transition(SessionState::Starting).is_err() {
                return rejected("invalid_state");
            }
            (session, true)
        } else {
            let Some(session_id) = payload.session_id.as_deref() else {
                return rejected("session_required");
            };
            let Ok(database) = database.try_lock() else {
                return rejected("storage_busy");
            };
            let Ok(Some(record)) = evohime_local_storage::browser_session_store::get(
                database.connection(),
                session_id,
            ) else {
                return rejected("session_not_found");
            };
            let Ok(session) = BrowserSession::from_metadata(
                crate::agentic_browser_session::BrowserSessionMetadata {
                    session_id: record.session_id,
                    conversation_id: record.conversation_id,
                    run_id: record.run_id,
                    state: record.state,
                    revision: record.revision,
                    control_generation: record.control_generation,
                    control_owner: record.control_owner,
                    profile_policy: record.profile_policy,
                    network_policy: record.network_policy,
                    policy_hash: record.policy_hash,
                },
            ) else {
                return rejected("invalid_session");
            };
            (session, false)
        };
        let mut operation_projection = serde_json::json!({});
        if !is_create
            && payload
                .expected_revision
                .is_some_and(|revision| revision != session.revision)
        {
            return rejected("stale_revision");
        }
        if matches!(
            request.operation.as_str(),
            "click" | "fill" | "select" | "press" | "download" | "upload"
        ) {
            let page_ref = payload.page_ref.as_deref().unwrap_or_default();
            let element_ref = payload.element_ref.as_deref().unwrap_or_default();
            if page_ref.is_empty() || page_ref.len() > 512 {
                return rejected("invalid_page_ref");
            }
            if element_ref
                .strip_prefix('e')
                .and_then(|value| value.parse::<u16>().ok())
                .is_none()
            {
                return rejected("invalid_element_ref");
            }
        }
        if !is_create && request.operation == "take_control" {
            if session.take_control().is_err() {
                return rejected("control_unavailable");
            }
        } else if !is_create && request.operation == "return_control" {
            if session.return_control().is_err() {
                return rejected("control_unavailable");
            }
        } else if !is_create && request.operation == "close" {
            let _ = session.transition(SessionState::Closing);
            let _ = session.transition(SessionState::Closed);
        } else if !is_create
            && session.control_owner == ControlOwner::Human
            && request.operation != "snapshot"
        {
            return rejected("control_taken");
        } else if is_create {
            let Ok(backend) = crate::browser_backend::BrowserBackendProcess::spawn().await else {
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unavailable","error_code":"browser_backend_unavailable","projection_json":{"raw_payload":false,"credentials":false,"cdp_endpoint":false}});
            };
            let mut backends = self.browser_backends.lock().await;
            backends.insert(session.session_id.to_string(), backend);
            let _ = session.transition(SessionState::Ready);
        } else if let Some(operation) = matches!(
            request.operation.as_str(),
            "navigate"
                | "snapshot"
                | "click"
                | "fill"
                | "select"
                | "press"
                | "scroll"
                | "wait"
                | "back"
                | "forward"
                | "reload"
                | "screenshot"
                | "download"
                | "upload"
        )
        .then_some(request.operation.as_str())
        {
            let mut backends = self.browser_backends.lock().await;
            let Some(backend) = backends.get_mut(&session.session_id.to_string()) else {
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unavailable","session_id":session.session_id,"revision":session.revision,"error_code":"browser_backend_unavailable","projection_json":{"raw_payload":false}});
            };
            let mut backend_payload = payload_json;
            if let Some(element_ref) = payload.element_ref.as_deref() {
                backend_payload["ref"] = serde_json::Value::String(element_ref.to_string());
            }
            if operation == "upload" {
                let Some(locator) = payload.artifact_ref.as_deref() else {
                    return rejected("artifact_required");
                };
                let Ok(database) = database.try_lock() else {
                    return rejected("storage_busy");
                };
                let Ok(bytes) = evohime_local_storage::artifact_store::ArtifactStore::new(
                    database.connection(),
                )
                .read_bytes(
                    locator,
                    &session.conversation_id,
                    &[],
                    "browser_upload",
                    now,
                ) else {
                    return rejected("artifact_not_readable");
                };
                if bytes.len() > 1024 * 1024 {
                    return rejected("artifact_too_large");
                }
                use base64::Engine;
                backend_payload["fileBase64"] = serde_json::Value::String(
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                );
                backend_payload["fileName"] = serde_json::Value::String("upload.bin".into());
            }
            let response = backend
                .request(crate::browser_backend::BrowserBackendProcess::command(
                    &request.request_id,
                    operation,
                    &backend_payload,
                ))
                .await;
            let Ok(response) = response else {
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unknown_outcome","session_id":session.session_id,"revision":session.revision,"error_code":"browser_backend_unknown_outcome","projection_json":{"raw_payload":false}});
            };
            if response["status"] != "ok" {
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","session_id":session.session_id,"revision":session.revision,"error_code":response["error_code"],"projection_json":{"raw_payload":false}});
            }
            if let Some(revision) = response["revision"].as_u64() {
                session.revision = revision;
            }
            if operation == "snapshot" {
                operation_projection =
                    serde_json::json!({"snapshot": response["snapshot"], "raw_dom": false});
            }
            if operation == "screenshot" || operation == "download" {
                use base64::Engine;
                let Some(encoded) = response["artifact_base64"].as_str() else {
                    return rejected("artifact_store_failed");
                };
                let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
                    return rejected("artifact_store_failed");
                };
                let Ok(database) = database.try_lock() else {
                    return rejected("storage_busy");
                };
                let Ok(offload) = evohime_local_storage::artifact_store::ArtifactStore::new(
                    database.connection(),
                )
                .offload_bytes(
                    if operation == "screenshot" {
                        "browser_screenshot"
                    } else {
                        "browser_download"
                    },
                    &session.conversation_id,
                    &session.conversation_id,
                    &bytes,
                    evohime_context_budget::item::Privacy::Workspace,
                    now,
                ) else {
                    return rejected("artifact_store_failed");
                };
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"ok","session_id":session.session_id,"revision":session.revision,"control_owner":"agent","control_generation":session.control_generation,"error_code":"","projection_json":{"schema_version":1,"session_id":session.session_id,"state":serde_json::to_value(session.state).unwrap_or_default(),"revision":session.revision,"artifact_ref":offload.reference.locator,"artifact_hash":offload.reference.content_hash,"raw_payload":false,"credentials":false,"cdp_endpoint":false}});
            }
        } else if !matches!(
            request.operation.as_str(),
            "create" | "take_control" | "return_control" | "close"
        ) {
            return rejected("unsupported_operation");
        }
        if is_create
            || request.operation == "take_control"
            || request.operation == "return_control"
            || request.operation == "close"
            || matches!(
                request.operation.as_str(),
                "navigate"
                    | "snapshot"
                    | "click"
                    | "fill"
                    | "select"
                    | "press"
                    | "scroll"
                    | "wait"
                    | "back"
                    | "forward"
                    | "reload"
                    | "screenshot"
                    | "download"
                    | "upload"
            )
        {
            if let Ok(database) = database.try_lock() {
                let _ = evohime_local_storage::browser_session_store::upsert(
                    database.connection(),
                    &evohime_local_storage::browser_session_store::BrowserSessionMetadata {
                        session_id: session.session_id.to_string(),
                        conversation_id: session.conversation_id.clone(),
                        run_id: session.run_id.clone(),
                        state: serde_json::to_value(session.state)
                            .unwrap_or_default()
                            .as_str()
                            .unwrap_or("failed")
                            .into(),
                        revision: session.revision,
                        control_generation: session.control_generation,
                        control_owner: match session.control_owner {
                            ControlOwner::Agent => "agent".into(),
                            ControlOwner::Human => "human".into(),
                        },
                        profile_policy: session.profile_policy.clone(),
                        network_policy: session.network_policy.clone(),
                        policy_hash: session.policy_hash.clone(),
                        updated_at_ms: now,
                    },
                );
            }
        }
        if request.operation == "close" {
            self.browser_backends
                .lock()
                .await
                .remove(&session.session_id.to_string());
        }
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"ok","session_id":session.session_id,"revision":session.revision,"control_owner":match session.control_owner { ControlOwner::Agent => "agent", ControlOwner::Human => "human" },"control_generation":session.control_generation,"error_code":"","projection_json":{"schema_version":1,"session_id":session.session_id,"state":serde_json::to_value(session.state).unwrap_or_default(),"revision":session.revision,"profile_policy":session.profile_policy,"network_policy":session.network_policy,"raw_payload":false,"credentials":false,"cdp_endpoint":false,"operation":operation_projection}})
    }

    pub(crate) async fn write_agentic_browser_session_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: AgenticBrowserSessionResponse = serde_json::from_slice(&payload)?;
        let result = generated::AgenticBrowserSessionEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            session_id: value.session_id,
            revision: value.revision,
            control_owner: value.control_owner,
            control_generation: value.control_generation,
            error_code: value.error_code,
            projection_json: serde_json::to_vec(&value.projection_json)?,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "agentic_browser_session.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::AgenticBrowserSession(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_human_work_items_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: HumanWorkItemsResponse = serde_json::from_slice(&payload)?;
        let result = generated::HumanWorkItemsEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            item_id: value.item_id,
            revision: value.revision,
            state: value.state,
            error_code: value.error_code,
            projection_json: serde_json::to_vec(&value.projection_json)?,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "human_work_items.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::HumanWorkItems(result)),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }
}
