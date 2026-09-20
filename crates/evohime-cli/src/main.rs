#[cfg(windows)]
use evohime_cli::{emit, redact_payload, terminal_exit_code, CliEvent, Command};
use evohime_cli::{parse_args, ExitCode};

#[cfg(windows)]
mod windows_client {
    use super::*;
    use evohime_cli::protocol::CoreClient as ProtocolClient;
    use evohime_desktop_ipc::generated;
    use std::path::PathBuf;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

    type CoreClient = ProtocolClient<NamedPipeClient>;

    async fn connect(after_sequence: u64) -> Result<CoreClient, String> {
        let context_path = std::env::var_os("EVOHIME_LAUNCH_CONTEXT")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .map(|value| PathBuf::from(value).join("EvoHime/runtime/session.json"))
            })
            .ok_or_else(|| "core_unavailable: launch context is not configured".to_string())?;
        let context = evohime_desktop_ipc::session::read_launch_context(&context_path)
            .map_err(|_| "core_unavailable: invalid launch context".to_string())?;
        let pipe = ClientOptions::new()
            .open(&context.pipe_name)
            .map_err(|_| "core_unavailable: named pipe is unavailable".to_string())?;
        ProtocolClient::connect(pipe, &context, after_sequence).await
    }

    pub async fn run(command: Command) -> ExitCode {
        let mut client = match connect(0).await {
            Ok(client) => client,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::CoreUnavailable;
            }
        };
        match command {
            Command::Doctor { json } => {
                if json {
                    println!(
                        "{}",
                        emit(&CliEvent {
                            schema: evohime_cli::CLI_SCHEMA,
                            sequence: 0,
                            kind: "core.ready",
                            run_id: "",
                            payload: serde_json::json!({"status":"ready"})
                        })
                    );
                } else {
                    println!("Core готов");
                }
                ExitCode::Completed
            }
            Command::Run {
                prompt,
                workspace,
                workflow,
                json,
                detach,
            } => {
                let request = evohime_cli_contract::RunRequest {
                    schema_version: evohime_cli_contract::SCHEMA_VERSION,
                    prompt: prompt.clone(),
                    workspace: workspace.clone(),
                    output_mode: if json {
                        evohime_cli_contract::OutputMode::Ndjson
                    } else {
                        evohime_cli_contract::OutputMode::Human
                    },
                    approval_mode: evohime_cli_contract::ApprovalMode::DenyIfApprovalRequired,
                    detach,
                };
                if evohime_cli_contract::validate_request(&request).is_err() {
                    eprintln!("invalid invocation: bounded Core CLI request is invalid");
                    return ExitCode::InvalidInvocation;
                }
                let run_id = uuid::Uuid::new_v4().to_string();
                let start_result = if let Some(template_id) = workflow {
                    client
                        .start_workflow(run_id.clone(), template_id, workspace)
                        .await
                } else {
                    client.start(run_id.clone(), prompt, workspace).await
                };
                if let Err(error) = start_result {
                    eprintln!("{error}");
                    return ExitCode::CoreUnavailable;
                }
                if detach {
                    if json {
                        println!(
                            "{}",
                            emit(&CliEvent {
                                schema: evohime_cli::CLI_SCHEMA,
                                sequence: 0,
                                kind: "run.accepted",
                                run_id: &run_id,
                                payload: serde_json::json!({"detached":true})
                            })
                        );
                    } else {
                        println!("{run_id}");
                    }
                    return ExitCode::Completed;
                }
                watch_events(&mut client, &run_id, json).await
            }
            Command::Watch { task_id, json } => watch_events(&mut client, &task_id, json).await,
            Command::Status { task_id, json } => match client.snapshot(task_id.clone()).await {
                Ok(event) => {
                    print_event(&event, &task_id, json);
                    ExitCode::Completed
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::CoreUnavailable
                }
            },
            Command::Cancel { task_id, json } => match client.stop(task_id.clone()).await {
                Ok(()) => {
                    if json {
                        println!(
                            "{}",
                            emit(&CliEvent {
                                schema: evohime_cli::CLI_SCHEMA,
                                sequence: 0,
                                kind: "run.cancel_requested",
                                run_id: &task_id,
                                payload: serde_json::json!({"accepted":true})
                            })
                        );
                    } else {
                        println!("Отмена запрошена: {task_id}");
                    }
                    ExitCode::Completed
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::CoreUnavailable
                }
            },
            Command::Resume { task_id, json } => watch_events(&mut client, &task_id, json).await,
        }
    }

    async fn watch_events(client: &mut CoreClient, run_id: &str, json: bool) -> ExitCode {
        loop {
            match client.next().await {
                Ok(event) => {
                    print_event(&event, run_id, json);
                    if let Some(code) = terminal_exit_code(&event.event_type) {
                        return code;
                    }
                }
                Err(error) => {
                    eprintln!("{error}; переподключение по cursor={}", client.sequence());
                    let cursor = client.sequence();
                    let mut replacement = None;
                    for _ in 0..5 {
                        match connect(cursor).await {
                            Ok(next) => {
                                replacement = Some(next);
                                break;
                            }
                            Err(_) => {
                                tokio::time::sleep(std::time::Duration::from_millis(250)).await
                            }
                        }
                    }
                    let Some(next) = replacement else {
                        return ExitCode::CoreUnavailable;
                    };
                    *client = next;
                }
            }
        }
    }

    fn print_event(event: &generated::EventEnvelope, run_id: &str, json: bool) {
        let payload = redact_payload(&event.payload);
        if json {
            println!(
                "{}",
                emit(&CliEvent {
                    schema: evohime_cli::CLI_SCHEMA,
                    sequence: event.sequence_id,
                    kind: &event.event_type,
                    run_id,
                    payload
                })
            );
        } else {
            println!("{} {}", event.event_type, run_id);
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse_args(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(ExitCode::InvalidInvocation as i32);
        }
    };
    #[cfg(windows)]
    let code = windows_client::run(command).await;
    #[cfg(not(windows))]
    let code = {
        let _ = command;
        eprintln!("core_unavailable: eva поддерживается только в Windows-сборке EvoHime");
        ExitCode::CoreUnavailable
    };
    std::process::exit(code as i32);
}
