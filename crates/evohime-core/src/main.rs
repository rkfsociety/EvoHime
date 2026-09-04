#[cfg(windows)]
fn main() {
    std::thread::Builder::new()
        // Debug startup has a large async state machine. The default process
        // stack is too small on Windows and can overflow before Core finishes
        // establishing its IPC listener.
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build Tokio runtime")
                .block_on(run());
        })
        .expect("failed to create Core runtime thread")
        .join()
        .expect("evohime-core runtime thread failed");
}

#[cfg(windows)]
async fn run() {
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
    let receipt_keys = std::sync::Arc::new(
        evohime_receipts::key_lifecycle::ReceiptKeyManager::new(&data_dir),
    );
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
    if let Err(error) = journal.recover_persistent_agent_registry().await {
        eprintln!("evohime-core persistent agent registry recovery failed: {error}");
        std::process::exit(1);
    }
    if let Err(error) = journal.recover_model_provenance_on_startup().await {
        eprintln!("evohime-core model provenance recovery failed: {error}");
        std::process::exit(1);
    }
    if let Err(error) = journal.recover_continuation_runs().await {
        eprintln!("evohime-core continuation recovery failed: {error}");
        std::process::exit(1);
    }
    if let Err(error) = journal.recover_retained_children().await {
        eprintln!("evohime-core retained child recovery failed: {error}");
        std::process::exit(1);
    }
    if let Err(error) = journal.recover_analysis_kernels().await {
        eprintln!("evohime-core analysis-kernel recovery failed: {error}");
        std::process::exit(1);
    }
    let _model_provenance_retention_task =
        evohime_core::spawn_model_provenance_retention(journal.clone());
    let heartbeat_task = spawn_heartbeat(data_dir.join("core-heartbeat"));
    let approval_gc_task = evohime_core::spawn_approval_gc(journal.clone(), receipt_keys.clone());
    let receipt_retention_task =
        evohime_core::spawn_receipt_retention(journal.clone(), receipt_keys.clone());
    // Этап 04.2: стартовый прогон ambient-retention выполняется внутри самой
    // задачи до первого ожидания, поэтому просроченные транскрипты исчезают
    // при запуске, а не через час.
    let ambient_retention_task = evohime_core::spawn_ambient_retention(journal.clone());
    let tools = std::sync::Arc::new(evohime_tool_runtime::ToolRegistry::bootstrap());
    let _permission_audit_task =
        evohime_core::attach_permission_audit_sink(journal.clone(), &tools).await;
    evohime_core::permission_rules::apply_rules(tools.permissions(), &data_dir).await;
    let approvals = evohime_core::ApprovalCoordinator::default();
    let routing_approvals = evohime_core::RoutingApprovalRegistry::default();
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
    // Один реестр проактивности на процесс: производитель предложений внутри
    // агента и мост, отвечающий на клик, обязаны считать один и тот же
    // потолок. Координатор подключается ниже — он рождается позже агента.
    let proactivity = evohime_core::ambient::AmbientProactivityRegistry::default();
    proactivity.set_data_dir(data_dir.clone()).await;
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
                .with_routing_approvals(routing_approvals.clone())
                .with_proactivity(proactivity.clone())
                .with_selected_model(selected_model.clone()),
            ) as std::sync::Arc<dyn evohime_core::TaskExecutor>
        });
    if std::env::args().any(|arg| arg == "--list-models") {
        list_console_models(gateway_config).await;
        heartbeat_task.abort();
        approval_gc_task.abort();
        receipt_retention_task.abort();
        ambient_retention_task.abort();
        return;
    }
    if let Some(request) = console_review_request() {
        run_console_review(request, gateway_config).await;
        heartbeat_task.abort();
        approval_gc_task.abort();
        receipt_retention_task.abort();
        ambient_retention_task.abort();
        return;
    }
    if let Some((prompt, workspace_root, approve_writes)) = console_request() {
        let Some(executor) = executor else {
            eprintln!("evohime-core console: модель не настроена; проверьте .env");
            std::process::exit(1);
        };
        let (coordinator, mut events) =
            evohime_core::TaskCoordinator::new_with_journal(256, Some(executor), journal);
        coordinator
            .attach_routing_approvals(routing_approvals.clone())
            .await;
        let task_id = uuid::Uuid::new_v4().to_string();
        if let Err(error) = coordinator
            .dispatch(evohime_core::CoreCommand::StartTask {
                task_id: task_id.clone(),
                prompt,
                workspace_root: Some(workspace_root),
                preferred_route_hint: None,
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
        ambient_retention_task.abort();
        return;
    }
    let (coordinator, _events) =
        evohime_core::TaskCoordinator::new_with_journal(256, executor, journal.clone());
    coordinator
        .attach_routing_approvals(routing_approvals)
        .await;
    proactivity.attach_coordinator(coordinator.clone()).await;
    let bridge = evohime_core::IpcBridge::with_coordinator_and_approvals(
        journal,
        coordinator,
        approvals,
        tools,
        model_snapshot,
        gateway_config,
    )
    .with_selected_model(selected_model)
    .with_proactivity(proactivity)
    .with_ambient_data_dir(data_dir.clone());
    // План 08-2: bounded core_start ledger event, затем reconciliation
    // незавершённых typed actions по dispatch marker в run_effects. Должно
    // идти после конструирования bridge — именно оно фиксирует
    // core_instance_id, который этот Core будет ставить на каждый EventEnvelope.
    if let Err(error) = bridge
        .journal()
        .record_ledger_core_start(bridge.core_instance_id())
        .await
    {
        eprintln!("evohime-core ledger core_start failed: {error}");
    }
    if let Err(error) = bridge.journal().reconcile_ledger_on_startup().await {
        eprintln!("evohime-core ledger reconciliation failed: {error}");
    }
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
    if let Err(error) = probe_supervisor(&context).await {
        eprintln!("evohime-core supervisor lifecycle probe failed: {error}");
    }
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
    let bridge = std::sync::Arc::new(bridge);
    let scheduler_bridge = std::sync::Arc::clone(&bridge);
    let automation_scheduler_task = tokio::spawn(async move {
        loop {
            scheduler_bridge.poll_automation_schedules().await;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
    let listener_bridge = std::sync::Arc::clone(&bridge);
    let listener_context = config.context.clone();
    let listener_logger = std::sync::Arc::clone(&logger);
    let result = tokio::select! {
        result = evohime_core::run_windows_pipe(config, bridge, logger) => result,
        _result = async move {
            loop {
                match evohime_core::run_windows_listener_pipe(listener_context.clone(), std::sync::Arc::clone(&listener_bridge), std::sync::Arc::clone(&listener_logger)).await {
                    Ok(()) => {}
                    Err(error) => {
                        eprintln!("evohime-core listener pipe restarted: {error}");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        } => {
            Err("listener supervision unexpectedly stopped".into())
        },
    };
    if let Err(error) = result {
        eprintln!("evohime-core failed: {error}");
        std::process::exit(1);
    }
    heartbeat_task.abort();
    automation_scheduler_task.abort();
    approval_gc_task.abort();
    receipt_retention_task.abort();
    ambient_retention_task.abort();
}

#[cfg(windows)]
async fn probe_supervisor(
    context: &evohime_desktop_ipc::session::LaunchContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use evohime_desktop_ipc::session::SessionSecret;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::windows::named_pipe::ClientOptions;

    let pipe = context
        .supervisor_pipe_name
        .as_deref()
        .ok_or("supervisor channel unavailable")?;
    let secret = context
        .supervisor_secret
        .as_ref()
        .ok_or("supervisor secret unavailable")?;
    let client_id = format!("core-{}", std::process::id());
    let client = ClientOptions::new().open(pipe)?;
    let mut channel = BufReader::new(client);
    let mut line = Vec::new();
    channel.read_until(b'\n', &mut line).await?;
    let challenge: serde_json::Value = serde_json::from_slice(&line)?;
    let nonce = challenge
        .get("nonce")
        .and_then(serde_json::Value::as_str)
        .ok_or("supervisor nonce missing")?;
    let proof = SessionSecret::parse(secret.expose())?.proof("core", &client_id, nonce);
    let user_sid = evohime_desktop_ipc::windows_security::current_user_sid()?;
    let logon_session = evohime_desktop_ipc::windows_security::current_logon_session()?;
    let request = serde_json::json!({
        "client_id": client_id,
        "client_role": "core",
        "nonce": nonce,
        "proof": proof,
        "peer": {"user_sid": user_sid, "logon_session": logon_session},
    });
    channel
        .get_mut()
        .write_all(serde_json::to_string(&request)?.as_bytes())
        .await?;
    channel.get_mut().write_all(b"\n").await?;
    line.clear();
    channel.read_until(b'\n', &mut line).await?;
    let authenticated: serde_json::Value = serde_json::from_slice(&line)?;
    if authenticated.get("authenticated") != Some(&serde_json::Value::Bool(true)) {
        return Err("supervisor rejected Core authentication".into());
    }
    channel.get_mut().write_all(b"{\"op\":\"probe\"}\n").await?;
    line.clear();
    channel.read_until(b'\n', &mut line).await?;
    let response: serde_json::Value = serde_json::from_slice(&line)?;
    if response.get("accepted") != Some(&serde_json::Value::Bool(true)) {
        return Err("supervisor probe was rejected".into());
    }
    Ok(())
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

/// Каталог моделей провайдера. Без него имена моделей для ревью пришлось бы
/// угадывать, а ключ провайдера в консоль не попадает и попасть не должен.
#[cfg(windows)]
async fn list_console_models(gateway_config: Option<evohime_model_gateway::ModelGatewayConfig>) {
    let Some(config) = gateway_config else {
        eprintln!("evohime-core console: модель не настроена; проверьте .env");
        std::process::exit(1);
    };
    let Some(route) = config.routes.get(&config.default_route) else {
        eprintln!("evohime-core console: маршрут по умолчанию не найден");
        std::process::exit(1);
    };
    match evohime_model_gateway::fetch_model_catalog(route).await {
        Ok(models) => {
            for model in models {
                println!(
                    "{}{}",
                    model.id,
                    model
                        .context_tokens
                        .map(|tokens| format!("  (окно {tokens})"))
                        .unwrap_or_default()
                );
            }
        }
        Err(error) => {
            eprintln!("evohime-core console: каталог не получен: {error}");
            std::process::exit(1);
        }
    }
}

/// Прогон ревью плана и правки по нему без оболочки.
///
/// Тот же код, что ходит через IPC, но без protobuf и без UI: это единственный
/// способ увидеть весь путь целиком в логе, когда проверять нужно ядро, а не
/// панель.
#[cfg(windows)]
struct ConsoleReview {
    plan: std::path::PathBuf,
    reviewers: Vec<String>,
    synthesis: String,
    revise: bool,
    out: Option<std::path::PathBuf>,
}

#[cfg(windows)]
fn console_review_request() -> Option<ConsoleReview> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.iter().any(|arg| arg == "--console") {
        return None;
    }
    let value = |name: &str| -> Option<String> {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    let plan = value("--review-plan")?;
    let reviewers: Vec<String> = value("--reviewers")
        .unwrap_or_default()
        .split(',')
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
        .collect();
    let synthesis = value("--synthesis").unwrap_or_default();
    if reviewers.len() < evohime_core::plan_review::MIN_REVIEWERS || synthesis.is_empty() {
        eprintln!(
            "Использование: evohime-core.exe --console --review-plan <план.md> --reviewers <модель1,модель2> --synthesis <модель> [--revise] [--out <файл.md>]"
        );
        std::process::exit(2);
    }
    Some(ConsoleReview {
        plan: std::path::PathBuf::from(plan),
        reviewers,
        synthesis,
        revise: args.iter().any(|arg| arg == "--revise"),
        out: value("--out").map(std::path::PathBuf::from),
    })
}

#[cfg(windows)]
async fn run_console_review(
    request: ConsoleReview,
    gateway_config: Option<evohime_model_gateway::ModelGatewayConfig>,
) {
    let Some(config) = gateway_config else {
        eprintln!("evohime-core console: модель не настроена; проверьте .env");
        std::process::exit(1);
    };
    let gateway = match evohime_model_gateway::ModelGateway::from_config(&config) {
        Ok(gateway) => std::sync::Arc::new(gateway),
        Err(error) => {
            eprintln!("evohime-core console: провайдер не поднялся: {error}");
            std::process::exit(1);
        }
    };
    let source = match std::fs::read_to_string(&request.plan) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("evohime-core console: план не прочитан: {error}");
            std::process::exit(1);
        }
    };
    let file_name = request
        .plan
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "plan.md".to_string());
    // Консольный прогон читает соседние планы так же, как IPC: иначе проверить
    // правку с контекстом можно было бы только через UI.
    let plan_path = request.plan.to_string_lossy().to_string();
    let context_documents =
        evohime_core::plan_context::read_linked_plans(std::slice::from_ref(&plan_path), &source)
            .await;
    let started = std::time::Instant::now();
    println!(
        "план: {} ({} байт); рецензенты: {}; синтез: {}; соседних планов: {}",
        file_name,
        source.len(),
        request.reviewers.join(", "),
        request.synthesis,
        context_documents.len()
    );

    let review = evohime_core::plan_review::ReviewRequest {
        review_id: format!("review-{}", uuid::Uuid::new_v4()),
        file_name: file_name.clone(),
        file_names: vec![file_name.clone()],
        source_markdown: source.clone(),
        reviewer_models: request.reviewers.clone(),
        synthesis_model: request.synthesis.clone(),
        context_documents: context_documents.clone(),
    };
    let clock = started;
    let review_result = evohime_core::plan_review::run_review_with_progress(
        std::sync::Arc::clone(&gateway),
        review,
        tokio_util::sync::CancellationToken::new(),
        std::sync::Arc::new(move |progress: evohime_core::plan_review::ReviewProgress| {
            println!(
                "[{:>6.1}s] review.progress {} {} {}/{} {}",
                clock.elapsed().as_secs_f32(),
                progress.stage,
                progress.status,
                progress.completed,
                progress.total,
                progress.model.as_deref().unwrap_or("")
            );
        }),
    )
    .await;
    let review_result = match review_result {
        Ok(result) => result,
        Err(error) => {
            eprintln!(
                "[{:>6.1}s] ✕ ревью не удалось: {error}",
                started.elapsed().as_secs_f32()
            );
            std::process::exit(1);
        }
    };
    println!(
        "[{:>6.1}s] ✓ ревью готово: {} байт итога",
        started.elapsed().as_secs_f32(),
        review_result.final_markdown.len()
    );
    if !request.revise {
        println!("\n{}", review_result.final_markdown);
        return;
    }

    let revision = evohime_core::plan_review::RevisionRequest {
        revision_id: format!("revision-{}", uuid::Uuid::new_v4()),
        review_id: review_result.review_id.clone(),
        file_name: file_name.clone(),
        source_markdown: source.clone(),
        // Тот же срез, что делает IPC: провенанс ревью не должен попасть в план.
        review_markdown: review_result
            .final_markdown
            .split_once("\n---\n\n")
            .map_or(review_result.final_markdown.clone(), |(_, body)| {
                body.to_string()
            }),
        model: request.synthesis.clone(),
        context_documents,
    };
    let clock = started;
    let revised = evohime_core::plan_review::run_revision(
        gateway,
        revision,
        tokio_util::sync::CancellationToken::new(),
        std::sync::Arc::new(
            move |progress: evohime_core::plan_review::RevisionProgress| {
                println!(
                    "[{:>6.1}s] revision.progress {} {}",
                    clock.elapsed().as_secs_f32(),
                    progress.status,
                    progress.model
                );
            },
        ),
    )
    .await;
    let revised = match revised {
        Ok(result) => result,
        Err(error) => {
            eprintln!(
                "[{:>6.1}s] ✕ правка не удалась: {error}",
                started.elapsed().as_secs_f32()
            );
            std::process::exit(1);
        }
    };
    println!(
        "[{:>6.1}s] ✓ план исправлен: было {} байт, стало {}",
        started.elapsed().as_secs_f32(),
        source.len(),
        revised.revised_markdown.len()
    );
    if revised.revised_markdown.len() * 2 < source.len() {
        eprintln!("⚠ исправленный план более чем вдвое короче исходного — вероятен обрыв ответа");
    }
    match request.out {
        Some(destination) => {
            if destination.extension().and_then(|value| value.to_str()) != Some("md") {
                eprintln!("evohime-core console: --out принимает только .md");
                std::process::exit(1);
            }
            if let Err(error) = std::fs::write(&destination, &revised.revised_markdown) {
                eprintln!("evohime-core console: план не записан: {error}");
                std::process::exit(1);
            }
            println!("записано: {}", destination.display());
        }
        None => println!("\n{}", revised.revised_markdown),
    }
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
        evohime_core::CoreEvent::RoutingTrace { trace, .. } => println!(
            "routing.terminal: route={} status={:?} fallback={}",
            trace.selected_route.as_deref().unwrap_or("—"),
            trace.terminal_status,
            trace.fallback_count
        ),
        evohime_core::CoreEvent::PendingRoutingApproval { route_id, expires_at_ms, .. } => println!(
            "routing.pending_approval: route={route_id} expires_at_ms={expires_at_ms}"
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
        evohime_core::CoreEvent::ChildWorkflowProjection { projection, .. } => println!(
            "child.workflow {}: {:?} rev={} lease={} dead_letter={}",
            projection.child_task_id,
            projection.state,
            projection.revision,
            projection.lease_live,
            projection.dead_letter
        ),
        evohime_core::CoreEvent::WorkflowProgress { run_id, projection } => println!(
            "workflow.progress {run_id}: {} node={} attempt={} error={}",
            projection.event_type, projection.node_id, projection.attempt, projection.error_code
        ),
        evohime_core::CoreEvent::WorkspaceBootstrapManifest { workspace_id, operation, status, .. } => println!(
            "workspace_bootstrap_manifest.result {workspace_id}: {operation} {status}"
        ),
        evohime_core::CoreEvent::TeamCoordinationPolicies { team_id, operation, status, .. } => println!(
            "team_coordination_policies.result {team_id}: {operation} {status}"
        ),
        evohime_core::CoreEvent::MemoryViewsAndAdaptiveRecall { view_id, operation, .. } => println!(
            "memory_views_and_adaptive_recall.result {view_id}: {operation}"
        ),
        evohime_core::CoreEvent::ModelEditProtocolRegistry { protocol_id, operation, .. } => println!(
            "model_edit_protocol_registry.result {protocol_id}: {operation}"
        ),
        evohime_core::CoreEvent::RemoteConversationChannels { connection_id, operation, .. } => println!(
            "remote_conversation_channels.result {connection_id}: {operation}"
        ),
        evohime_core::CoreEvent::PromptCachePlanner { plan_id, operation, .. } => println!(
            "prompt_cache_planner.result {plan_id}: {operation}"
        ),
        evohime_core::CoreEvent::DeclarativeRuntimeComponents { component_id, operation, version, .. } => println!(
            "declarative_runtime_components.result {component_id}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::GuidedCalibrationSessions { session_id, operation, version, .. } => println!(
            "guided_calibration_sessions.result {session_id}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::ExtensionConformanceKit { subject_id, operation, version, .. } => println!(
            "extension_conformance_kit.result {subject_id}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::TypedAgentHandoffContract { handoff_id, operation, state, .. } => println!(
            "typed_agent_handoff_contract.result {handoff_id}: {operation} {state}"
        ),
        evohime_core::CoreEvent::SchemaDrivenAgentConfiguration { scope, operation, revision, .. } => println!(
            "schema_driven_agent_configuration.result {scope}: {operation} revision={revision}"
        ),
        evohime_core::CoreEvent::ExperienceReplayLibrary { scope, operation, revision, .. } => println!(
            "experience_replay_library.result {scope}: {operation} revision={revision}"
        ),
        evohime_core::CoreEvent::RuntimeInterventionPipeline { run_id, operation, .. } => println!(
            "runtime_intervention_pipeline.result {run_id}: {operation}"
        ),
        evohime_core::CoreEvent::CodeDiagnosticsFeedbackLoop { workspace_root_id, operation, revision, .. } => println!(
            "code_diagnostics_feedback_loop.result {workspace_root_id}: {operation} revision={revision}"
        ),
        evohime_core::CoreEvent::WorkflowOptimizationLab { run_id, operation, revision, .. } => println!(
            "workflow_optimization_lab.result {run_id}: {operation} revision={revision}"
        ),
        evohime_core::CoreEvent::CoreTopicSubscriptionEventBus { operation, .. } => println!(
            "core_topic_subscription_event_bus.result: {operation}"
        ),
        evohime_core::CoreEvent::DependencyAwareTaskGraph { graph_id, operation, revision, .. } => println!(
            "dependency_aware_task_graph.result {graph_id}: {operation} revision={revision}"
        ),
        evohime_core::CoreEvent::DeclarativeAgentComponentRegistry { registry_id, operation, revision, .. } => println!(
            "declarative_agent_component_registry.result {registry_id}: {operation} revision={revision}"
        ),
        evohime_core::CoreEvent::TypedContextReferences { ref_id, operation, .. } => println!(
            "typed_context_references.result {ref_id}: {operation}"
        ),
        evohime_core::CoreEvent::SafeUiExtensionFramework { extension_id, operation, revision, .. } => println!(
            "safe_ui_extension_framework.result {extension_id}: {operation} revision={revision}"
        ),
        evohime_core::CoreEvent::CapabilityWorkbench { instance_id, operation, revision, .. } => println!(
            "capability_workbench.result {instance_id}: {operation} revision={revision}"
        ),
        evohime_core::CoreEvent::TeamCoordinator { work_item_id, operation, revision, .. } => println!(
            "team_coordinator.result {work_item_id}: {operation} revision={revision}"
        ),
    evohime_core::CoreEvent::ProjectInstructionStack { workspace_root, operation, revision, .. } => println!(
            "project_instruction_stack.result {workspace_root}: {operation} revision={revision}"
        ),
    evohime_core::CoreEvent::WorkspaceSets { set_id, operation, version, .. } => println!(
            "workspace_sets.result {set_id}: {operation} version={version}"
        ),
    evohime_core::CoreEvent::KnowledgeSourceRegistryProjectRole { source_id, operation, version, .. } => println!(
            "knowledge_source_registry.result {source_id}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::DurableRemoteTaskBridge { remote_task_id, operation, version, .. } => println!(
            "durable_remote_task_bridge.result {remote_task_id}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::MessageInterventionPolicies { operation, version, .. } => println!(
            "message_intervention_policies.result: {operation} version={version}"
        ),
        evohime_core::CoreEvent::BatchInvocationRuntime { batch_id, operation, version, .. } => println!(
            "batch_invocation_runtime.result {batch_id}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::PolicyAwareToolResultCache { cache_key, operation, version, .. } => println!(
            "policy_aware_tool_result_cache.result {cache_key}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::CodeAnchoredIntentMarkers { operation, version, .. } => println!(
            "code_anchored_intent_markers.result: {operation} version={version}"
        ),
        evohime_core::CoreEvent::ModelPurposeRouting { operation, version, .. } => println!(
            "model_purpose_routing.result: {operation} version={version}"
        ),
        evohime_core::CoreEvent::LocalModelRuntimeManager { operation, version, .. } => println!(
            "local_model_runtime_manager.result: {operation} version={version}"
        ),
        evohime_core::CoreEvent::ArchitectureSnapshot { snapshot_id, operation, version, .. } => println!(
            "architecture_snapshot.result {snapshot_id}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::AgentGitChangeSets { change_set_id, operation, version, .. } => println!(
            "agent_git_change_sets.result {change_set_id}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::ArchitectEditorModelPipeline { pipeline_id, operation, version, .. } => println!(
            "architect_editor_pipeline.result {pipeline_id}: {operation} version={version}"
        ),
        evohime_core::CoreEvent::EventVisualizerRegistry { visualizer_id, operation, version, .. } => println!("event_visualizer_registry.result {visualizer_id}: {operation} version={version}"),
        evohime_core::CoreEvent::ReasoningOperatorLibrary { operator_id, operation, version, .. } => println!("reasoning_operator_library.result {operator_id}: {operation} version={version}"),
        evohime_core::CoreEvent::OutputGuardrailPipeline { pipeline_id, operation, version, .. } => println!("output_guardrail_pipeline.result {pipeline_id}: {operation} version={version}"),
        evohime_core::CoreEvent::CustomizationInventory { item_id, operation, version, .. } => println!("customization_inventory.result {item_id}: {operation} version={version}"),
        evohime_core::CoreEvent::StandingApprovalProfiles { profile_id, operation, version, .. } => println!("standing_approval_profiles.result {profile_id}: {operation} version={version}"),
        evohime_core::CoreEvent::ApprovalPolicyProfiles { profile_id, operation, version, .. } => println!("approval_policy_profiles.result {profile_id}: {operation} version={version}"),
        evohime_core::CoreEvent::CheckpointForking { fork_run_id, operation, version, .. } => println!("checkpoint_forking.result {fork_run_id}: {operation} version={version}"),
        evohime_core::CoreEvent::PrivacyTelemetryGovernance { category, operation, version, .. } => println!("privacy_telemetry_governance.result {category}: {operation} version={version}"),
        evohime_core::CoreEvent::ConversationBridgeAdapters { bridge_id, operation, revision, .. } => println!("conversation_bridge_adapters.result {bridge_id}: {operation} revision={revision}"),
        evohime_core::CoreEvent::PersistentAgentOrganizationRegistry { agent_id, operation, revision, .. } => println!("persistent_agent_organization_registry.result {agent_id}: {operation} revision={revision}"),
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
/// name with a freshly generated secret and no identity binding for local
/// developer launches; such a connection is reported as unauthenticated in the
/// log and in `core.started`.
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
        supervisor_pipe_name: None,
        supervisor_secret: None,
    })
}
