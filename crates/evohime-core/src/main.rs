#[cfg(windows)]
#[tokio::main]
async fn main() {
    let data_dir = normalized_env_path("EVOHIME_DATA_DIR")
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|path| std::path::PathBuf::from(path).join("EvoHime"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".evohime"));
    let journal = match evohime_core::EventJournal::open(data_dir.join("events.db")) {
        Ok(journal) => journal,
        Err(error) => {
            eprintln!("evohime-core storage failed: {error}");
            std::process::exit(1);
        }
    };
    let receipt_keys = std::sync::Arc::new(evohime_receipts::key_lifecycle::ReceiptKeyManager::new(&data_dir));
    {
        let mut database = journal.database().lock().await;
        if let Err(error) = receipt_keys.startup_with_database(database.connection_mut()) {
            eprintln!("evohime-core receipt key lifecycle failed: {error}");
            std::process::exit(1);
        }
        if let Err(error) = evohime_receipts::runtime::recover_database(database.connection_mut()) {
            eprintln!("evohime-core receipt recovery failed: {error}");
        }
        if receipt_keys.scheduled_rotation_due().unwrap_or(false) {
            let trusted = receipt_keys
                .load_history()
                .ok()
                .and_then(|items| {
                    items.first().map(|item| {
                        receipt_keys
                            .trusted_genesis(&item.new_key_id)
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if trusted {
                if let Err(error) = receipt_keys.rotate_with_database(
                    database.connection_mut(),
                    "scheduled",
                    "system",
                ) {
                    eprintln!("scheduled receipt key rotation failed: {error}");
                } else {
                    let _ = receipt_keys.record_rotation_check();
                }
            } else {
                eprintln!("scheduled receipt key rotation blocked: key.trust_required");
            }
        }
    }
    if let Err(error) = journal.recover_and_reconcile_after_restart().await {
        eprintln!("evohime-core recovery failed: {error}");
        std::process::exit(1);
    }
    let heartbeat_task = spawn_heartbeat(data_dir.join("core-heartbeat"));
    let approval_gc_task = evohime_core::spawn_approval_gc(journal.clone(), receipt_keys.clone());
    let receipt_retention_task =
        evohime_core::spawn_receipt_retention(journal.clone(), receipt_keys.clone());
    let tools = std::sync::Arc::new(evohime_tool_runtime::ToolRegistry::bootstrap());
    let _permission_audit_task =
        evohime_core::attach_permission_audit_sink(journal.clone(), &tools).await;
    evohime_core::permission_rules::apply_rules(tools.permissions(), &data_dir).await;
    let approvals = evohime_core::ApprovalCoordinator::default();
    let model_config = evohime_model_gateway::ModelGatewayConfig::from_env().ok();
    let model_snapshot = model_config.as_ref().and_then(|config| {
        config
            .routes
            .get(&config.default_route)
            .map(|route| evohime_core::ModelConfigSnapshot {
                provider: route.provider.as_str().to_string(),
                route: config.default_route.clone(),
                model: route.literouter.model.clone(),
                configured: route.configured(),
            })
    });
    let gateway_config = model_config.clone();
    // One selection shared by the agent and the IPC bridge: the shell changes
    // it, the next request picks it up without a Core restart.
    let selected_model = evohime_core::SelectedModel::default();
    let executor = model_config
        .and_then(|config| evohime_model_gateway::ModelGateway::from_config(&config).ok())
        .map(|gateway| {
            std::sync::Arc::new(
                evohime_core::ToolAgent::new_with_approvals(
                    std::sync::Arc::new(gateway),
                    tools.clone(),
                    approvals.clone(),
                )
                .with_journal(journal.clone())
                .with_receipt_keys(receipt_keys.clone())
                .with_selected_model(selected_model.clone()),
            ) as std::sync::Arc<dyn evohime_core::TaskExecutor>
        });
    if let Some((prompt, workspace_root, approve_writes)) = console_request() {
        let Some(executor) = executor else {
            eprintln!("evohime-core console: модель не настроена; проверьте .env");
            std::process::exit(1);
        };
        let (coordinator, mut events) =
            evohime_core::TaskCoordinator::new_with_journal(256, Some(executor), journal);
        let task_id = uuid::Uuid::new_v4().to_string();
        if let Err(error) = coordinator
            .dispatch(evohime_core::CoreCommand::StartTask {
                task_id: task_id.clone(),
                prompt,
                workspace_root: Some(workspace_root),
            })
            .await
        {
            eprintln!("evohime-core console: не удалось запустить задачу: {error}");
            std::process::exit(1);
        }
        while let Ok(event) = events.recv().await {
            let finished = matches!(
                &event,
                evohime_core::CoreEvent::TaskCompleted { .. }
                    | evohime_core::CoreEvent::TaskFailed { .. }
                    | evohime_core::CoreEvent::TaskStopped { .. }
            );
            print_console_event(&event);
            if let evohime_core::CoreEvent::ApprovalRequired { approval_id, .. } = &event {
                let granted = approve_writes;
                if let Ok(approval_id) = uuid::Uuid::parse_str(approval_id) {
                    let _ = tools.permissions().resolve(approval_id, granted).await;
                    let _ = approvals.resolve(approval_id, granted).await;
                    println!(
                        "{} approval: {}",
                        if granted { "✓" } else { "✕" },
                        if granted {
                            "разрешено"
                        } else {
                            "отклонено"
                        }
                    );
                }
            }
            if finished {
                break;
            }
        }
        heartbeat_task.abort();
        approval_gc_task.abort();
        receipt_retention_task.abort();
        return;
    }
    let (coordinator, _events) =
        evohime_core::TaskCoordinator::new_with_journal(256, executor, journal.clone());
    let bridge = evohime_core::IpcBridge::with_coordinator_and_approvals(
        journal,
        coordinator,
        approvals,
        tools,
        model_snapshot,
        gateway_config,
    )
    .with_selected_model(selected_model);
    let logger = match evohime_core::StructuredLogger::open(data_dir.join("logs/core.jsonl")) {
        Ok(logger) => std::sync::Arc::new(logger),
        Err(error) => {
            eprintln!("evohime-core logging failed: {error}");
            std::process::exit(1);
        }
    };
    let context = match launch_context() {
        Ok(context) => context,
        Err(error) => {
            eprintln!("evohime-core launch context failed: {error}");
            std::process::exit(1);
        }
    };
    let authenticated = context.is_authenticated();
    let _ = logger.write(
        "info",
        "core.started",
        serde_json::json!({
            "protocol_major": 1,
            "protocol_minor": 0,
            "authenticated": authenticated,
        }),
    );
    let config = evohime_core::PipeServerConfig {
        context,
        enforce_authentication: authenticated,
    };
    if let Err(error) = evohime_core::run_windows_pipe(config, bridge, logger).await {
        eprintln!("evohime-core failed: {error}");
        std::process::exit(1);
    }
    heartbeat_task.abort();
    approval_gc_task.abort();
    receipt_retention_task.abort();
}

#[cfg(windows)]
fn normalized_env_path(name: &str) -> Option<std::path::PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
}

#[cfg(windows)]
fn spawn_heartbeat(path: std::path::PathBuf) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let payload = format!("{}\n", heartbeat_timestamp());
            let _ = std::fs::write(&path, payload);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    })
}

#[cfg(windows)]
fn heartbeat_timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(windows)]
fn console_request() -> Option<(String, std::path::PathBuf, bool)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.iter().any(|arg| arg == "--console") {
        return None;
    }
    let mut workspace = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut prompt_parts = Vec::new();
    let mut approve_writes = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--workspace" => {
                if let Some(value) = args.get(index + 1) {
                    workspace = std::path::PathBuf::from(value);
                    index += 1;
                }
            }
            "--prompt" => {
                if let Some(value) = args.get(index + 1) {
                    prompt_parts.push(value.clone());
                    index += 1;
                }
            }
            "--console" => {}
            "--approve-writes" => approve_writes = true,
            "--" => {
                prompt_parts.extend(args.iter().skip(index + 1).cloned());
                break;
            }
            value if !value.starts_with('-') => prompt_parts.push(value.to_string()),
            _ => {}
        }
        index += 1;
    }
    let prompt = prompt_parts.join(" ").trim().to_string();
    if prompt.is_empty() {
        eprintln!("Использование: evohime-core.exe --console --workspace <path> --prompt <текст>");
        std::process::exit(2);
    }
    Some((prompt, workspace, approve_writes))
}

#[cfg(windows)]
fn print_console_event(event: &evohime_core::CoreEvent) {
    match event {
        evohime_core::CoreEvent::ModelContext {
            workspace_path,
            model,
            tools,
            estimated_tokens,
            context_limit_tokens,
            ..
        } => println!(
            "Контекст: модель={model}; workspace={workspace_path}; инструментов={}; токены={estimated_tokens}/{context_limit_tokens}",
            tools.len()
        ),
        evohime_core::CoreEvent::TaskStarted { prompt, .. } => println!("\nЗапрос: {prompt}"),
        evohime_core::CoreEvent::AssistantDelta { content, .. } => print!("{content}"),
        evohime_core::CoreEvent::ToolStarted { tool_name, .. } => {
            println!("\n→ tool.started {tool_name}")
        }
        evohime_core::CoreEvent::ToolOutput {
            tool_name, output, ..
        } => println!("← tool.output {tool_name}\n{output}"),
        evohime_core::CoreEvent::ApprovalRequired {
            tool_name, permission, ..
        } => println!("⚠ approval.required {tool_name}: {permission}"),
        evohime_core::CoreEvent::TaskCompleted { final_message, .. } => {
            println!("\n\n✓ Задача завершена\n{final_message}")
        }
        evohime_core::CoreEvent::TaskFailed { error, .. } => println!("\n\n✕ Задача завершена с ошибкой\n{error}"),
        evohime_core::CoreEvent::TaskStopped { .. } => println!("\n\n■ Задача остановлена"),
        evohime_core::CoreEvent::ReviewProgress {
            review_id,
            stage,
            status,
            model,
            completed,
            total,
        } => println!(
            "review.progress {review_id}: {stage} {status} {}/{} {}",
            completed,
            total,
            model.as_deref().unwrap_or("")
        ),
        evohime_core::CoreEvent::RevisionProgress {
            revision_id,
            status,
            model,
        } => println!("revision.progress {revision_id}: {status} {model}"),
        evohime_core::CoreEvent::StorageProgress { operation_id, progress } => println!(
            "storage.progress {operation_id}: {:?} {}/{}",
            progress.phase,
            progress.completed,
            progress.total.map_or_else(|| "?".into(), |value| value.to_string())
        ),
        evohime_core::CoreEvent::WorkspaceIndexProgress { workspace_path, progress } => println!(
            "workspace.index_progress {workspace_path}: {} {}/{} chunks",
            progress.phase,
            progress.indexed_files,
            progress.chunks
        ),
        evohime_core::CoreEvent::WorkspaceRetrievalProgress { workspace_path, progress } => println!(
            "workspace.retrieval_progress {workspace_path}: {} iteration={} results={}",
            progress.event_type,
            progress.iteration,
            progress.result_count
        ),
        evohime_core::CoreEvent::ReviewHistoryCleared { marker_id } => {
            println!("review.history_cleared {marker_id}")
        }
    }
}

#[cfg(not(windows))]
fn main() {
    println!("evohime-core {}", evohime_core::CoreVersion::current());
}

/// Resolves the launch context that binds this Core generation to one pipe,
/// one session secret and one Windows identity.
///
/// The supervisor passes a protected context file through
/// `EVOHIME_LAUNCH_CONTEXT`. Without it Core keeps serving the legacy pipe
/// name with a freshly generated secret and no identity binding, which is the
/// developer and WinUI-compatibility path; such a connection is reported as
/// unauthenticated in the log and in `core.started`.
#[cfg(windows)]
fn launch_context() -> Result<evohime_desktop_ipc::session::LaunchContext, std::io::Error> {
    use evohime_desktop_ipc::session::{
        read_launch_context, validate_pipe_name, LaunchContext, SessionSecret,
    };

    if let Some(path) = std::env::var_os("EVOHIME_LAUNCH_CONTEXT") {
        return read_launch_context(std::path::Path::new(&path));
    }

    let pipe_name =
        std::env::var("EVOHIME_CORE_PIPE").unwrap_or_else(|_| r"\\.\pipe\evohime-core-v1".into());
    validate_pipe_name(&pipe_name).map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(LaunchContext {
        pipe_name,
        secret: SessionSecret::generate()
            .map_err(|error| std::io::Error::other(error.to_string()))?,
        expected_user_sid: String::new(),
        expected_logon_session: String::new(),
        issued_at_ms: 0,
        supervisor_pid: 0,
        supervisor_liveness_event: String::new(),
    })
}
