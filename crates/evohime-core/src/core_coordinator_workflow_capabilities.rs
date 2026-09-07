use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::CapabilityWorkbench {
            operation,
            instance_id,
            owner_id,
            payload,
            expected_revision,
            grants,
            reply,
        } => {
            let result = async {
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::capability_workbenches as workbench;
                    use evohime_local_storage::capability_workbenches_store as store;
                    let now = crate::task_memory::now_millis();
                    let mut instance: workbench::WorkbenchInstance = if operation == "create" {
                        let descriptor: workbench::WorkbenchDescriptor =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_descriptor")?;
                        let instance = workbench::WorkbenchInstance::new(
                            instance_id.clone(),
                            owner_id.clone(),
                            descriptor,
                            now,
                        )
                        .map_err(|error| error.to_string())?;
                        let descriptor_json =
                            serde_json::to_vec(&instance).map_err(|_| "serialization_failed")?;
                        store::put_instance(
                            database.connection(),
                            &instance.instance_id,
                            &instance.owner_id,
                            instance.revision as i64,
                            "created",
                            &descriptor_json,
                            now as i64,
                        )
                        .map_err(|_| "instance_exists")?;
                        store::put_lease(
                            database.connection(),
                            &format!("{}:{}", instance.instance_id, instance.owner_id),
                            &instance.instance_id,
                            &instance.owner_id,
                            now.saturating_add(instance.descriptor.lease_ttl_ms) as i64,
                            now as i64,
                        )
                        .map_err(|_| "lease_exists")?;
                        return serde_json::to_vec(&instance)
                            .map_err(|_| "serialization_failed".to_string());
                    } else {
                        let descriptor_json =
                            store::get_instance(database.connection(), &instance_id)
                                .map_err(|_| "storage_failed")?
                                .ok_or_else(|| "instance_not_found".to_string())?;
                        let instance: workbench::WorkbenchInstance = serde_json::from_slice(&descriptor_json)
                            .map_err(|_| "corrupt_instance".to_string())?;
                        if instance.owner_id != owner_id {
                            return Err("owner_denied".into());
                        }
                        instance
                    };
                    if operation == "list_tools" {
                        let tools = instance.visible_tools(&grants);
                        return serde_json::to_vec(&serde_json::json!({
                            "schema_version": workbench::SCHEMA_VERSION,
                            "instance_id": instance_id,
                            "tools": tools,
                        }))
                        .map_err(|_| "serialization_failed".to_string());
                    }
                    let target = match operation.as_str() {
                        "start" => Some(workbench::Lifecycle::Starting),
                        "ready" => Some(workbench::Lifecycle::Ready),
                        "stop" => Some(workbench::Lifecycle::Stopping),
                        "stopped" => Some(workbench::Lifecycle::Stopped),
                        "reset" => Some(workbench::Lifecycle::Resetting),
                        "degraded" => Some(workbench::Lifecycle::Degraded),
                        _ => None,
                    };
                    if let Some(target) = target {
                        instance
                            .transition(target, expected_revision)
                            .map_err(|error| error.to_string())?;
                    } else if operation == "heartbeat" {
                        instance.heartbeat(now);
                        if !store::renew_lease(
                            database.connection(),
                            &format!("{}:{}", instance.instance_id, instance.owner_id),
                            &instance.owner_id,
                            now as i64,
                            now.saturating_add(instance.descriptor.lease_ttl_ms) as i64,
                        )
                        .map_err(|_| "storage_failed")?
                        {
                            return Err("lease_not_found".into());
                        }
                    } else if operation == "recover" {
                        store::expire_leases(database.connection(), now as i64)
                            .map_err(|_| "storage_failed")?;
                        instance.recover_if_expired(now);
                    } else if operation == "call_tool" {
                        let call: WorkbenchCallRequest =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_call")?;
                        let capability = call.capability.as_str();
                        instance
                            .admit_call(capability, &grants)
                            .map_err(|error| error.to_string())?;
                        instance.finish_call();
                        let tool_id = call.tool_id.as_deref().unwrap_or(capability);
                        let result = workbench::WorkbenchCallResult {
                            schema_version: workbench::SCHEMA_VERSION,
                            instance_id: instance.instance_id.clone(),
                            tool_id: tool_id.to_owned(),
                            outcome: workbench::CallOutcome::Unavailable,
                            value: serde_json::Value::Null,
                            error_code: Some("runtime_adapter_unavailable".into()),
                            cancellation: workbench::CancellationOutcome::AlreadyTerminal,
                        };
                        return serde_json::to_vec(&result)
                            .map_err(|_| "serialization_failed".to_string());
                    } else if operation == "cancel" {
                        return serde_json::to_vec(&serde_json::json!({
                            "instance_id": instance.instance_id,
                            "cancellation_outcome": format!("{:?}", workbench::cancellation_outcome(false, false)).to_ascii_lowercase(),
                            "unknown": true,
                        })).map_err(|_| "serialization_failed".to_string());
                    } else if operation == "resource" {
                        let request: WorkbenchResourceRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_resource")?;
                        let resource = instance.descriptor.resources.iter_mut().find(|resource| resource.id == request.resource_id).ok_or_else(|| "resource_not_found".to_string())?;
                        resource.available = request.available;
                        instance.revision = instance.revision.saturating_add(1);
                    } else if operation == "snapshot" {
                        let request: WorkbenchSnapshotRequest = if payload.is_empty() {
                            WorkbenchSnapshotRequest::default()
                        } else {
                            serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot")?
                        };
                        let snapshot = instance
                            .snapshot(request.logical_state, request.credential_refs)
                            .map_err(|error| error.to_string())?;
                        let snapshot_json =
                            serde_json::to_vec(&snapshot).map_err(|_| "serialization_failed")?;
                        let snapshot_id = uuid::Uuid::now_v7().to_string();
                        store::put_snapshot(
                            database.connection(),
                            &snapshot_id,
                            &instance_id,
                            snapshot.revision as i64,
                            &snapshot_json,
                            now as i64,
                        )
                        .map_err(|_| "storage_failed")?;
                        return Ok(serde_json::to_vec(
                            &serde_json::json!({"snapshot_id":snapshot_id,"snapshot":snapshot}),
                        )
                        .map_err(|_| "serialization_failed")?);
                    } else if operation == "restore" {
                        let snapshot: workbench::WorkbenchSnapshot =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot")?;
                        workbench::validate_snapshot(&snapshot)
                            .map_err(|error| error.to_string())?;
                        if snapshot.instance_id != instance_id {
                            return Err("snapshot_instance_mismatch".into());
                        }
                        instance.lifecycle = snapshot.lifecycle;
                        instance.revision = snapshot.revision.saturating_add(1);
                    } else if operation != "get" {
                        return Err("unsupported_workbench_operation".into());
                    }
                    let descriptor_json =
                        serde_json::to_vec(&instance).map_err(|_| "serialization_failed")?;
                    if operation != "get" {
                        let old_revision = if operation == "heartbeat" || operation == "list_tools"
                        {
                            instance.revision
                        } else {
                            instance.revision.saturating_sub(1)
                        } as i64;
                        if !store::replace_instance(
                            database.connection(),
                            &instance_id,
                            old_revision,
                            instance.revision as i64,
                            &format!("{:?}", instance.lifecycle).to_ascii_lowercase(),
                            &descriptor_json,
                            now as i64,
                        )
                        .map_err(|_| "storage_failed")?
                        {
                            return Err("stale_workbench_revision".into());
                        }
                    }
                    serde_json::to_vec(&instance).map_err(|_| "serialization_failed".to_string())
                }
                .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let revision = serde_json::from_slice::<serde_json::Value>(projection_json.as_bytes())
                .ok()
                .and_then(|value| value.get("revision").and_then(serde_json::Value::as_u64))
                .unwrap_or(expected_revision);
            let event = CoreEvent::CapabilityWorkbench {
                instance_id,
                operation,
                revision,
                projection_json,
            };
            let journal = state.lock().await.journal.clone();
            if let Some(journal) = journal {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}
