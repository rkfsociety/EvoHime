#[cfg(windows)]
#[tokio::main]
async fn main() {
    let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|path| std::path::PathBuf::from(path).join("EvoHime"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".evohime"));
    let pipe_name =
        std::env::var("EVOHIME_CORE_PIPE").unwrap_or_else(|_| r"\\.\pipe\evohime-core-v1".into());
    let journal = match evohime_core::EventJournal::open(data_dir.join("events.db")) {
        Ok(journal) => journal,
        Err(error) => {
            eprintln!("evohime-core storage failed: {error}");
            std::process::exit(1);
        }
    };
    let executor = evohime_model_gateway::ModelGateway::try_from_env()
        .ok()
        .map(|gateway| {
            std::sync::Arc::new(evohime_core::ToolAgent::new(
                std::sync::Arc::new(gateway),
                std::sync::Arc::new(evohime_tool_runtime::ToolRegistry::bootstrap()),
            )) as std::sync::Arc<dyn evohime_core::TaskExecutor>
        });
    let (coordinator, _events) =
        evohime_core::TaskCoordinator::new_with_journal(256, executor, journal.clone());
    let bridge = evohime_core::IpcBridge::with_coordinator(journal, coordinator);
    let logger = match evohime_core::StructuredLogger::open(data_dir.join("logs/core.jsonl")) {
        Ok(logger) => std::sync::Arc::new(logger),
        Err(error) => {
            eprintln!("evohime-core logging failed: {error}");
            std::process::exit(1);
        }
    };
    let _ = logger.write(
        "info",
        "core.started",
        serde_json::json!({"pipe": pipe_name, "protocol_major": 1, "protocol_minor": 0}),
    );
    if let Err(error) = evohime_core::run_windows_pipe(&pipe_name, bridge, logger).await {
        eprintln!("evohime-core failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    println!("evohime-core {}", evohime_core::CoreVersion::current());
}
