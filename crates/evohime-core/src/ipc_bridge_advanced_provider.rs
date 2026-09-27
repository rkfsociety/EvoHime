use super::*;

impl IpcBridge {
    pub(crate) async fn dispatch_integration_provider_sdk(
        &self,
        request: generated::IntegrationProviderSdkCommand,
    ) -> serde_json::Value {
        let operation = request.operation.as_str();
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.request_id.len() > 128
            || request.owner_scope != "settings"
            || request.payload.len() > 8 * 1024
        {
            return integration_provider_result(
                &request,
                "rejected",
                "invalid_request",
                serde_json::Value::Null,
            );
        }
        if operation == "list_catalog" || operation == "get_provider" {
            let manifest = match crate::github_public_repository::manifest() {
                Ok(manifest) => manifest,
                Err(_) => {
                    return integration_provider_result(
                        &request,
                        "error",
                        "manifest_unavailable",
                        serde_json::Value::Null,
                    )
                }
            };
            let fixture = match crate::integration_provider_sdk::fixture_echo_manifest() {
                Ok(manifest) => manifest,
                Err(_) => {
                    return integration_provider_result(
                        &request,
                        "error",
                        "fixture_unavailable",
                        serde_json::Value::Null,
                    )
                }
            };
            return integration_provider_result(
                &request,
                "ok",
                "",
                serde_json::json!({"providers":[manifest, fixture]}),
            );
        }
        if operation == "invoke_fixture" {
            let input = match serde_json::from_slice(&request.payload) {
                Ok(value) => value,
                Err(_) => serde_json::Value::Null,
            };
            let result = crate::integration_provider_runtime::invoke_fixture(
                "fixture.echo",
                "echo",
                input,
            );
            return integration_provider_result(
                &request,
                "ok",
                "",
                serde_json::json!({"result":result}),
            );
        }

        match operation {
            "list_repositories" => {
                let database = self.journal.database().lock().await;
                match evohime_local_storage::github_repository_store::list(database.connection()) {
                    Ok(repositories) => integration_provider_result(
                        &request,
                        "ok",
                        "",
                        serde_json::json!({"repositories":repositories}),
                    ),
                    Err(_) => integration_provider_result(
                        &request,
                        "error",
                        "storage_error",
                        serde_json::Value::Null,
                    ),
                }
            }
            "add_repository" | "remove_repository" | "refresh_repository" => {
                let input: serde_json::Value = match serde_json::from_slice(&request.payload) {
                    Ok(value) => value,
                    Err(_) => {
                        return integration_provider_result(
                            &request,
                            "rejected",
                            "invalid_payload",
                            serde_json::Value::Null,
                        )
                    }
                };
                let owner = input.get("owner").and_then(serde_json::Value::as_str);
                let repo = input.get("repo").and_then(serde_json::Value::as_str);
                let (Some(owner), Some(repo)) = (owner, repo) else {
                    return integration_provider_result(
                        &request,
                        "rejected",
                        "repository_required",
                        serde_json::Value::Null,
                    );
                };
                let repository = match crate::github_public_repository::RepositoryId::parse(
                    owner.to_owned(),
                    repo.to_owned(),
                ) {
                    Ok(repository) => repository,
                    Err(_) => {
                        return integration_provider_result(
                            &request,
                            "rejected",
                            "invalid_repository",
                            serde_json::Value::Null,
                        )
                    }
                };
                match operation {
                    "add_repository" => {
                        let database = self.journal.database().lock().await;
                        let now_ms = match std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                        {
                            Ok(duration) => duration.as_millis().min(i64::MAX as u128) as i64,
                            Err(_) => 0,
                        };
                        match evohime_local_storage::github_repository_store::save(
                            database.connection(),
                            &repository.owner,
                            &repository.repo,
                            now_ms,
                        ) {
                            Ok(inserted) => integration_provider_result(
                                &request,
                                "ok",
                                "",
                                serde_json::json!({"added":inserted}),
                            ),
                            Err(rusqlite::Error::InvalidParameterName(name))
                                if name == "github_repository_limit" =>
                            {
                                integration_provider_result(
                                    &request,
                                    "rejected",
                                    "repository_limit",
                                    serde_json::Value::Null,
                                )
                            }
                            Err(_) => integration_provider_result(
                                &request,
                                "error",
                                "storage_error",
                                serde_json::Value::Null,
                            ),
                        }
                    }
                    "remove_repository" => {
                        let database = self.journal.database().lock().await;
                        match evohime_local_storage::github_repository_store::remove(
                            database.connection(),
                            &repository.owner,
                            &repository.repo,
                        ) {
                            Ok(removed) => integration_provider_result(
                                &request,
                                "ok",
                                "",
                                serde_json::json!({"removed":removed}),
                            ),
                            Err(_) => integration_provider_result(
                                &request,
                                "error",
                                "storage_error",
                                serde_json::Value::Null,
                            ),
                        }
                    }
                    "refresh_repository" => {
                        let is_saved = {
                            let database = self.journal.database().lock().await;
                            match evohime_local_storage::github_repository_store::list(
                                database.connection(),
                            ) {
                                Ok(saved) => saved.iter().any(|entry| {
                                    entry.owner.eq_ignore_ascii_case(&repository.owner)
                                        && entry.repo.eq_ignore_ascii_case(&repository.repo)
                                }),
                                Err(_) => {
                                    return integration_provider_result(
                                        &request,
                                        "error",
                                        "storage_error",
                                        serde_json::Value::Null,
                                    )
                                }
                            }
                        };
                        if !is_saved {
                            return integration_provider_result(
                                &request,
                                "rejected",
                                "repository_not_saved",
                                serde_json::Value::Null,
                            );
                        }
                        match crate::github_public_repository::GitHubPublicRepositoryClient::new() {
                            Err(error) => integration_provider_result(
                                &request,
                                "unavailable",
                                fetch_error_code(error),
                                serde_json::Value::Null,
                            ),
                            Ok(client) => match client.fetch(&repository).await {
                                Ok(projection) => integration_provider_result(
                                    &request,
                                    "ok",
                                    "",
                                    serde_json::json!({"repository":projection}),
                                ),
                                Err(error) => integration_provider_result(
                                    &request,
                                    "unavailable",
                                    fetch_error_code(error),
                                    serde_json::Value::Null,
                                ),
                            },
                        }
                    }
                    _ => integration_provider_result(
                        &request,
                        "unavailable",
                        "unsupported_operation",
                        serde_json::Value::Null,
                    ),
                }
            }
            _ => integration_provider_result(
                &request,
                "unavailable",
                "unsupported_operation",
                serde_json::Value::Null,
            ),
        }
    }

    pub(crate) fn dispatch_event_trigger_runtime(
        &self,
        request: generated::EventTriggerRuntimeCommand,
    ) -> serde_json::Value {
        let operation = request.operation.as_str();
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":operation,"status":"rejected","error_code":"invalid_request"});
        }
        match operation {
            "list" | "get" => serde_json::json!({
                "schema_version": 1, "request_id": request.request_id, "operation": operation,
                "status": "ok", "triggers": [], "mvp_sources": ["local_workspace_event", "system_event"],
                "provider_webhook": "unavailable", "error_code": ""
            }),
            "reconcile" | "pause" | "resume" => serde_json::json!({
                "schema_version": 1, "request_id": request.request_id, "operation": operation,
                "status": "unavailable", "error_code": "no_trigger_configured"
            }),
            _ => serde_json::json!({
                "schema_version": 1, "request_id": request.request_id, "operation": operation,
                "status": "unavailable", "error_code": "unsupported_operation"
            }),
        }
    }

    pub(crate) async fn dispatch_invocation_preset(
        &self,
        request: generated::InvocationPresetCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        if request.operation == "run" {
            let envelope: InvocationPresetRunPayload = match serde_json::from_slice(
                &request.payload,
            ) {
                Ok(value) => value,
                Err(_) => {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"rejected","error_code":"invalid_payload"});
                }
            };
            let preset_id = envelope.preset_id.as_str();
            let revision = envelope.revision;
            let workspace = envelope.workspace_path;
            let idempotency = if request.idempotency_key.is_empty() {
                request.request_id.clone()
            } else {
                request.idempotency_key.clone()
            };
            let mut preset = {
                let database = self.journal.database().lock().await;
                let Some((content, stored_hash, state)) =
                    evohime_local_storage::invocation_presets_store::read_revision(
                        database.connection(),
                        &request.owner_scope,
                        preset_id,
                        revision,
                    )
                    .ok()
                    .flatten()
                else {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"rejected","error_code":"unknown_preset_revision"});
                };
                if state != "ready" {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"blocked","error_code":"needs_rebinding_or_migration"});
                }
                let Ok(preset) =
                    serde_json::from_str::<crate::invocation_presets::InvocationPreset>(&content)
                else {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"rejected","error_code":"corrupt_preset"});
                };
                if preset.content_hash != stored_hash
                    || preset.canonical_content_hash().ok().as_deref() != Some(stored_hash.as_str())
                {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"rejected","error_code":"preset_hash_mismatch"});
                }
                preset
            };
            for (key, value) in envelope.temporary_overrides {
                if preset.input_values.contains_key(&key) {
                    preset.input_values.insert(key, value);
                }
            }
            return match self
                .start_invocation_preset(preset, workspace, idempotency)
                .await
            {
                Ok(run_id) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"started","run_id":run_id,"error_code":""})
                }
                Err(error) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"blocked","error_code":error})
                }
            };
        }
        let database = self.journal.database().lock().await;
        let connection = database.connection();
        match request.operation.as_str() {
            "list" => {
                let mut statement = match connection.prepare("SELECT id, revision, content_hash, state FROM invocation_presets WHERE owner_scope=?1 ORDER BY id, revision DESC LIMIT ?2") { Ok(statement) => statement, Err(_) => return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","error_code":"storage_error"}) };
                let limit = if request.expected_revision == 0 {
                    50
                } else {
                    request.expected_revision.min(100)
                };
                let rows = statement.query_map(rusqlite::params![request.owner_scope, limit as i64], |row| Ok(serde_json::json!({"id":row.get::<_,String>(0)?,"revision":row.get::<_,i64>(1)? as u64,"content_hash":row.get::<_,String>(2)?,"state":row.get::<_,String>(3)?}))).and_then(|rows| rows.collect::<Result<Vec<_>, _>>());
                match rows {
                    Ok(presets) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"ok","presets":presets,"error_code":""})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"error","presets":[],"error_code":"storage_error"})
                    }
                }
            }
            "create" | "save" => {
                let mut preset: crate::invocation_presets::InvocationPreset =
                    match serde_json::from_slice(&request.payload) {
                        Ok(value) => value,
                        Err(_) => {
                            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":"invalid_payload"})
                        }
                    };
                if preset.owner_scope != request.owner_scope {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":"owner_scope_mismatch"});
                }
                if let Err(error) = preset.validate() {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":error.to_string()});
                }
                preset.content_hash = match preset.canonical_content_hash() {
                    Ok(hash) => hash,
                    Err(_) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":"preset_serialization_failed"});
                    }
                };
                let content = serde_json::to_string(&preset).unwrap_or_default();
                let state = serde_json::to_value(preset.state)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("ready")
                    .to_string();
                match evohime_local_storage::invocation_presets_store::save_revision(
                    connection,
                    evohime_local_storage::invocation_presets_store::SaveRevisionInput {
                        owner_scope: &preset.owner_scope,
                        id: &preset.id,
                        revision: preset.revision,
                        content_json: &content,
                        content_hash: &preset.content_hash,
                        state: &state,
                        now_ms: crate::task_memory::now_millis() as i64,
                    },
                ) {
                    Ok(true) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"saved","preset_id":preset.id,"revision":preset.revision,"content_hash":preset.content_hash,"error_code":""})
                    }
                    Ok(false) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"conflict","preset_id":preset.id,"revision":preset.revision,"error_code":"duplicate_revision"})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"error","error_code":"storage_error"})
                    }
                }
            }
            "sanitize" => match crate::invocation_presets::sanitize_completed_run(
                &serde_json::from_slice(&request.payload).unwrap_or_default(),
            ) {
                Ok(preview) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"sanitize","status":"preview","preview":preview,"error_code":""})
                }
                Err(error) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"sanitize","status":"rejected","error_code":error.to_string()})
                }
            },
            "preview_migration" | "migrate" => {
                let envelope: serde_json::Value =
                    serde_json::from_slice(&request.payload).unwrap_or_default();
                let preset_id = envelope
                    .get("preset_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let source_revision = envelope
                    .get("source_revision")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let migration: crate::invocation_presets::PresetMigrationRequest =
                    match serde_json::from_value(
                        envelope.get("migration").cloned().unwrap_or_default(),
                    ) {
                        Ok(value) => value,
                        Err(_) => {
                            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_migration"})
                        }
                    };
                let Some((content, stored_hash, _state)) =
                    evohime_local_storage::invocation_presets_store::read_revision(
                        connection,
                        &request.owner_scope,
                        preset_id,
                        source_revision,
                    )
                    .ok()
                    .flatten()
                else {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"unknown_preset_revision"});
                };
                let source: crate::invocation_presets::InvocationPreset = match serde_json::from_str(
                    &content,
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"corrupt_preset"})
                    }
                };
                if source.content_hash != stored_hash
                    || source.canonical_content_hash().ok().as_deref() != Some(stored_hash.as_str())
                    || migration.source_revision != source_revision
                {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"preset_hash_mismatch"});
                }
                if request.operation == "preview_migration" {
                    return match crate::invocation_presets::preview_migration(&source, &migration) {
                        Ok(preview) => {
                            serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"preview_migration","status":"preview","preview":preview,"error_code":""})
                        }
                        Err(error) => {
                            serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"preview_migration","status":"rejected","error_code":error.to_string()})
                        }
                    };
                }
                let migrated = match crate::invocation_presets::migrate_preset(
                    &source,
                    &migration,
                    crate::task_memory::now_millis() as i64,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"migrate","status":"rejected","error_code":error.to_string()})
                    }
                };
                let content = serde_json::to_string(&migrated).unwrap_or_default();
                let state = serde_json::to_value(migrated.state)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("ready")
                    .to_string();
                match evohime_local_storage::invocation_presets_store::save_revision(
                    connection,
                    evohime_local_storage::invocation_presets_store::SaveRevisionInput {
                        owner_scope: &migrated.owner_scope,
                        id: &migrated.id,
                        revision: migrated.revision,
                        content_json: &content,
                        content_hash: &migrated.content_hash,
                        state: &state,
                        now_ms: migrated.updated_at_ms,
                    },
                ) {
                    Ok(true) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"migrate","status":"migrated","preset_id":migrated.id,"revision":migrated.revision,"content_hash":migrated.content_hash,"error_code":""})
                    }
                    Ok(false) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"migrate","status":"conflict","error_code":"duplicate_revision"})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"migrate","status":"error","error_code":"storage_error"})
                    }
                }
            }
            _ => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unavailable","error_code":"unsupported_operation"})
            }
        }
    }
}

fn integration_provider_result(
    request: &generated::IntegrationProviderSdkCommand,
    status: &str,
    error_code: &str,
    projection: serde_json::Value,
) -> serde_json::Value {
    let mut result = serde_json::json!({
        "schema_version":1,
        "request_id":request.request_id,
        "status":status,
        "operation":request.operation,
        "error_code":error_code,
    });
    if let (Some(target), Some(source)) = (result.as_object_mut(), projection.as_object()) {
        target.extend(source.iter().map(|(key, value)| (key.clone(), value.clone())));
    }
    result
}

fn fetch_error_code(error: crate::github_public_repository::FetchError) -> &'static str {
    use crate::github_public_repository::FetchError;
    match error {
        FetchError::Busy => "refresh_in_progress",
        FetchError::NotFound => "repository_not_found",
        FetchError::RateLimited => "github_rate_limited",
        FetchError::TimedOut => "github_timeout",
        FetchError::Network => "github_unavailable",
        FetchError::Oversized => "github_response_too_large",
        FetchError::InvalidResponse => "github_invalid_response",
    }
}
