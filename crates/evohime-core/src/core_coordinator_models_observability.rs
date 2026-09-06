use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::ReasoningOperatorLibrary {
            operation,
            operator_id,
            payload,
            expected_version,
            idempotency_key: _,
            reply,
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::reasoning_operator_library as operators;
                    use evohime_local_storage::reasoning_operator_library_store as store;
                    match operation.as_str() {
                        "list" => { let mut defs=operators::builtins(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? { if let Ok(d)=serde_json::from_slice(&row){defs.push(d)} } serde_json::to_vec(&serde_json::json!({"schema_version":1,"operators":defs,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "register" => { let d:operators::ReasoningOperatorDefinition=serde_json::from_slice(&payload).map_err(|_|"invalid_operator_definition".to_string())?; operators::validate(&d).map_err(|e|e.to_string())?; let j=serde_json::to_vec(&d).map_err(|_|"serialization_failed".to_string())?; store::put(database.connection(),&d.id,d.version,&d.content_hash,&j,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"operator_id":d.id,"status":"registered","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "execute" => { let req:operators::OperatorRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_operator_request".to_string())?; operators::validate_request(&req).map_err(|e|e.to_string())?; if req.operator_id!=operator_id{return Err("operator_id_mismatch".into())}; if expected_version>3{return Err("operator_stale_version".into())}; serde_json::to_vec(&serde_json::json!({"schema_version":1,"operator_id":operator_id,"status":"proposed","output_contract":"typed_json","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        _=>Err("unsupported_reasoning_operator_operation".into())
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::ReasoningOperatorLibrary {
                operator_id,
                operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        CoreCommand::OutputGuardrailPipeline {
            operation,
            pipeline_id,
            payload,
            expected_version,
            idempotency_key: _,
            reply,
        } => {
            let result = async { if operation != "evaluate" { return Err("unsupported_output_guardrail_operation".into()); } let p: crate::output_guardrail_pipeline::GuardrailPipeline = serde_json::from_slice(&payload).map_err(|_| "invalid_guardrail_pipeline".to_string())?; let r=crate::output_guardrail_pipeline::evaluate(&p, &payload).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"pipeline_id":pipeline_id,"version":expected_version,"result":r,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::OutputGuardrailPipeline {
                pipeline_id,
                operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        CoreCommand::CustomizationInventory {
            operation,
            item_id,
            payload,
            expected_version,
            reply,
            ..
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::customization_inventory as inventory;
                    use evohime_local_storage::customization_inventory_store as store;
                    match operation.as_str() {
                        "list" => {
                            let mut items = Vec::new();
                            for row in store::list(database.connection()).map_err(|_| "storage_failed".to_string())? {
                                if let Ok(item) = serde_json::from_slice(&row) { items.push(item); }
                            }
                            inventory::sort(&mut items).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"items":items,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "register" => {
                            let item: inventory::CustomizationItem = serde_json::from_slice(&payload).map_err(|_| "invalid_customization_item".to_string())?;
                            inventory::validate(&item).map_err(|e| e.to_string())?;
                            if !item_id.is_empty() && item.id != item_id { return Err("item_id_mismatch".into()); }
                            let data = serde_json::to_vec(&item).map_err(|_| "serialization_failed".to_string())?;
                            store::put(database.connection(), &item.id, &format!("{:?}", item.kind), item.version, &data, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"item_id":item.id,"version":item.version,"status":"registered","redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "remove" => {
                            if expected_version == 0 { return Err("expected_version_required".into()); }
                            database.connection().execute("DELETE FROM customization_inventory WHERE id=?1 AND version=?2", rusqlite::params![item_id, expected_version]).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"item_id":item_id,"version":expected_version,"status":"removed","redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_customization_inventory_operation".into())
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::CustomizationInventory {
                item_id,
                operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        CoreCommand::StandingApprovalProfiles {
            operation,
            profile_id,
            payload,
            expected_version,
            reply,
            ..
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::standing_approval_profiles as profiles;
                    use evohime_local_storage::standing_approval_profiles_store as store;
                    match operation.as_str() {
                        "list" => { let mut values:Vec<profiles::StandingApprovalProfile>=Vec::new(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? { if let Ok(p)=serde_json::from_slice(&row){values.push(p)} } serde_json::to_vec(&serde_json::json!({"schema_version":1,"profiles":values,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "create"|"update" => { let p:profiles::StandingApprovalProfile=serde_json::from_slice(&payload).map_err(|_|"invalid_profile".to_string())?; profiles::validate(&p).map_err(|e|e.to_string())?; if !profile_id.is_empty()&&p.id!=profile_id{return Err("profile_id_mismatch".into())}; let j=serde_json::to_vec(&p).map_err(|_|"serialization_failed".to_string())?; store::put(database.connection(),&p.id,p.version,p.enabled,&j,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":p.id,"version":p.version,"status":"saved","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "revoke" => { if expected_version==0{return Err("expected_version_required".into())}; database.connection().execute("UPDATE standing_approval_profiles SET enabled=0, version=version+1 WHERE id=?1 AND version=?2",rusqlite::params![profile_id,expected_version]).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":profile_id,"version":expected_version+1,"status":"revoked","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "match" => { let req:profiles::ApprovalRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_approval_request".to_string())?; let mut decisions=Vec::new(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? { if let Ok(p)=serde_json::from_slice(&row){ if let Ok(d)=profiles::match_request(&p,&req){decisions.push(d)} } } let approved=decisions.iter().find(|d|d.approved).cloned(); serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":approved.as_ref().and_then(|d|d.profile_id.clone()),"approved":approved.is_some(),"reason":approved.map(|d|d.reason).unwrap_or_else(||"no_match".into()),"execution_policy_required":true,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        _=>Err("unsupported_standing_approval_operation".into())
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::StandingApprovalProfiles {
                profile_id,
                operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        CoreCommand::ApprovalPolicyProfiles {
            operation,
            profile_id,
            payload,
            expected_version,
            reply,
            ..
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::approval_policy_profiles as policy;
                    use evohime_local_storage::approval_policy_profiles_store as store;
                    match operation.as_str() {
                        "list" => { let mut values:Vec<policy::ApprovalPolicyProfile>=Vec::new(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? { if let Ok(p)=serde_json::from_slice(&row){values.push(p)} } serde_json::to_vec(&serde_json::json!({"schema_version":1,"profiles":values,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "create"|"update" => { let p:policy::ApprovalPolicyProfile=serde_json::from_slice(&payload).map_err(|_|"invalid_policy_profile".to_string())?; policy::validate(&p).map_err(|e|e.to_string())?; if !profile_id.is_empty()&&p.id!=profile_id{return Err("profile_id_mismatch".into())}; let j=serde_json::to_vec(&p).map_err(|_|"serialization_failed".to_string())?; store::put(database.connection(),&p.id,p.version,p.enabled,&j,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":p.id,"version":p.version,"status":"saved","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "revoke" => { if expected_version==0{return Err("expected_version_required".into())}; database.connection().execute("UPDATE approval_policy_profiles SET enabled=0, version=version+1 WHERE id=?1 AND version=?2",rusqlite::params![profile_id,expected_version]).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":profile_id,"version":expected_version+1,"status":"revoked","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "decide" => { let req:PolicyDecisionRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_policy_request".to_string())?; let mut decisions=Vec::new(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? {if let Ok(p)=serde_json::from_slice::<policy::ApprovalPolicyProfile>(&row){decisions.push(policy::decide(&p,&req.scope_id,&req.action_class,&req.resource,req.risk,req.now_ms).map_err(|e|e.to_string())?)}} let d=decisions.into_iter().find(|x|!x.require_prompt).unwrap_or(policy::PolicyDecision{require_prompt:true,profile_id:None,reason:"prompt_required".into(),hard_requirement:req.risk>=3}); serde_json::to_vec(&serde_json::json!({"schema_version":1,"require_prompt":d.require_prompt,"profile_id":d.profile_id,"reason":d.reason,"hard_requirement":d.hard_requirement,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        _=>Err("unsupported_approval_policy_operation".into())
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::ApprovalPolicyProfiles {
                profile_id,
                operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        CoreCommand::CheckpointForking {
            operation,
            fork_run_id,
            payload,
            reply,
        } => {
            let result = async {
                if operation != "fork" {
                    return Err("unsupported_checkpoint_fork_operation".into());
                };
                let request: crate::checkpoint_forking_and_replay::ForkRequest =
                    serde_json::from_slice(&payload)
                        .map_err(|_| "invalid_fork_request".to_string())?;
                let lineage =
                    crate::checkpoint_forking_and_replay::create(request, fork_run_id.clone())
                        .map_err(|e| e.to_string())?;
                let journal = state
                    .lock()
                    .await
                    .journal
                    .clone()
                    .ok_or_else(|| "storage journal is not configured".to_string())?;
                let db = journal.database().lock().await;
                let json =
                    serde_json::to_vec(&lineage).map_err(|_| "serialization_failed".to_string())?;
                evohime_local_storage::checkpoint_forking_store::put(
                    db.connection(),
                    &lineage.fork_run_id,
                    &lineage.source_checkpoint_id,
                    &lineage.parent_run_id,
                    &json,
                    crate::task_memory::now_millis() as i64,
                )
                .map_err(|_| "storage_failed".to_string())?;
                Ok(json)
            }
            .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::CheckpointForking {
                fork_run_id,
                operation,
                version: 1,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        CoreCommand::PrivacyTelemetryGovernance {
            operation,
            category,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            #[expect(clippy::possible_missing_else, clippy::needless_question_mark, reason = "compact privacy command state machine uses sequential guards")]
                let result = async { use crate::privacy_and_telemetry_governance as g; use evohime_local_storage::privacy_telemetry_store as store; let journal=state.lock().await.journal.clone().ok_or_else(||"storage journal is not configured".to_string())?; let db=journal.database().lock().await; if idempotency_key.is_empty() || idempotency_key.len()>128 { return Err("invalid_idempotency_key".to_string()); } if expected_version != 0 { let current=store::consent_revision(db.connection()).map_err(|_|"storage_failed".to_string())?.unwrap_or(0); if current != expected_version { return Err("stale_version".to_string()); } } if !store::claim_idempotency(db.connection(),&idempotency_key,&operation).map_err(|_|"storage_failed".to_string())? { return Ok(serde_json::to_vec(&serde_json::json!({"status":"replayed","redacted":true})).map_err(|_|"serialization_failed".to_string())?); } match operation.as_str(){"consent"=>{let c:g::ConsentState=serde_json::from_slice(&payload).map_err(|_|"invalid_consent".to_string())?;g::validate_consent(&c).map_err(|e|e.to_string())?;let j=serde_json::to_vec(&c).map_err(|_|"serialization_failed".to_string())?;store::put_consent(db.connection(),&j,c.revision).map_err(|_|"storage_failed".to_string())?;serde_json::to_vec(&serde_json::json!({"status":"consent_saved","redacted":true})).map_err(|_|"serialization_failed".to_string())},"enqueue"=>{let request:g::TelemetryEnqueueRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_enqueue_request".to_string())?;let e=request.event;g::enqueue(&request.consent,e.clone()).map_err(|e|e.to_string())?;let j=serde_json::to_vec(&e).map_err(|_|"serialization_failed".to_string())?;let inserted=store::put_event(db.connection(),&e.event_id,&format!("{:?}",e.category),&j,e.created_at_ms).map_err(|_|"storage_failed".to_string())?;serde_json::to_vec(&serde_json::json!({"queued":inserted,"redacted":true})).map_err(|_|"serialization_failed".to_string())},"list"=>serde_json::to_vec(&serde_json::json!({"events":store::list(db.connection()).map_err(|_|"storage_failed".to_string())?,"redacted":true})).map_err(|_|"serialization_failed".to_string()),"clear"=>{store::clear(db.connection()).map_err(|_|"storage_failed".to_string())?;serde_json::to_vec(&serde_json::json!({"status":"cleared","redacted":true})).map_err(|_|"serialization_failed".to_string())},_=>Err("unsupported_privacy_telemetry_operation".into())}}.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::PrivacyTelemetryGovernance {
                operation,
                category,
                version: 1,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}
