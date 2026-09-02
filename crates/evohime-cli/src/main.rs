use evohime_cli::{emit, parse_args, redact_payload, CliEvent, Command, ExitCode};

#[cfg(windows)]
mod windows_client {
    use super::*;
    use evohime_desktop_ipc::{generated, session, transport};
    use prost::Message;
    use std::path::PathBuf;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

    pub struct CoreClient {
        pipe: NamedPipeClient,
        sequence: u64,
        client_id: String,
        core_instance_id: String,
        session_epoch: u64,
    }

    impl CoreClient {
        pub async fn connect(after_sequence: u64) -> Result<Self, String> {
            let context_path = std::env::var_os("EVOHIME_LAUNCH_CONTEXT")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("LOCALAPPDATA")
                        .map(|value| PathBuf::from(value).join("EvoHime/runtime/session.json"))
                })
                .ok_or_else(|| "core_unavailable: launch context is not configured".to_string())?;
            let context = session::read_launch_context(&context_path)
                .map_err(|_| "core_unavailable: invalid launch context".to_string())?;
            let pipe = ClientOptions::new()
                .open(&context.pipe_name)
                .map_err(|_| "core_unavailable: named pipe is unavailable".to_string())?;
            let client_id = format!("cli-{}", uuid::Uuid::new_v4());
            let mut client = Self {
                pipe,
                sequence: after_sequence,
                client_id: client_id.clone(),
                core_instance_id: String::new(),
                session_epoch: 0,
            };
            let challenge = client.read_event().await?;
            let nonce = challenge
                .event
                .and_then(|event| match event {
                    generated::event_envelope::Event::AuthChallenge(value) => Some(value.nonce),
                    _ => None,
                })
                .ok_or_else(|| "authentication_failed: challenge missing".to_string())?;
            let proof = context.secret.proof("cli", &client_id, &nonce);
            client
                .write(generated::CommandEnvelope {
                    protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                    request_id: uuid::Uuid::new_v4().to_string(),
                    client_id: client_id.clone(),
                    core_instance_id: String::new(),
                    session_epoch: 0,
                    command: Some(generated::command_envelope::Command::Handshake(
                        generated::Handshake {
                            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                            client_id: client_id.clone(),
                            session_id: client_id,
                            session_epoch: 0,
                            last_event_sequence: after_sequence,
                            capabilities: vec![
                                "headless-cli".into(),
                                "replay".into(),
                                "resync".into(),
                            ],
                            client_role: "cli".into(),
                            nonce,
                            proof,
                        },
                    )),
                })
                .await?;
            let ready = client.read_event().await?;
            if !matches!(
                ready.event,
                Some(generated::event_envelope::Event::Ready(_))
            ) {
                return Err("authentication_failed: Core did not become ready".into());
            }
            client.core_instance_id = ready.core_instance_id;
            client.session_epoch = ready.session_epoch;
            Ok(client)
        }

        async fn write(&mut self, command: generated::CommandEnvelope) -> Result<(), String> {
            transport::write_frame(&mut self.pipe, &command.encode_to_vec())
                .await
                .map_err(|error| error.to_string())
        }

        async fn read_event(&mut self) -> Result<generated::EventEnvelope, String> {
            let payload = transport::read_frame(&mut self.pipe)
                .await
                .map_err(|error| error.to_string())?;
            let event = generated::EventEnvelope::decode(payload.as_slice())
                .map_err(|error| format!("protocol_error: {error}"))?;
            self.sequence = self.sequence.max(event.sequence_id);
            Ok(event)
        }

        pub async fn start(
            &mut self,
            task_id: String,
            prompt: String,
            workspace: String,
        ) -> Result<(), String> {
            self.write(generated::CommandEnvelope {
                protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                request_id: uuid::Uuid::new_v4().to_string(),
                client_id: self.client_id.clone(),
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                command: Some(generated::command_envelope::Command::StartTask(
                    generated::StartTask {
                        task_id,
                        prompt,
                        workspace_path: workspace,
                        preferred_route_hint: String::new(),
                        execution_kind: "agent".into(),
                        conversation_id: String::new(),
                        client_message_id: String::new(),
                    },
                )),
            })
            .await
        }

        pub async fn start_workflow(
            &mut self,
            task_id: String,
            template_id: String,
            workspace: String,
        ) -> Result<(), String> {
            self.write(generated::CommandEnvelope {
                protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                request_id: uuid::Uuid::new_v4().to_string(),
                client_id: self.client_id.clone(),
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                command: Some(generated::command_envelope::Command::StartWorkflow(
                    generated::StartWorkflow {
                        template_id,
                        task_id,
                        workspace_path: workspace,
                        inputs: Vec::new(),
                        idempotency_key: uuid::Uuid::new_v4().to_string(),
                    },
                )),
            })
            .await
        }

        pub async fn stop(&mut self, task_id: String) -> Result<(), String> {
            self.write(generated::CommandEnvelope {
                protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                request_id: uuid::Uuid::new_v4().to_string(),
                client_id: self.client_id.clone(),
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                command: Some(generated::command_envelope::Command::StopTask(
                    generated::StopTask { task_id },
                )),
            })
            .await
        }

        pub async fn snapshot(
            &mut self,
            task_id: String,
        ) -> Result<generated::EventEnvelope, String> {
            self.write(generated::CommandEnvelope {
                protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                request_id: uuid::Uuid::new_v4().to_string(),
                client_id: self.client_id.clone(),
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                command: Some(generated::command_envelope::Command::GetTaskSnapshot(
                    generated::GetTaskSnapshot {
                        project_id: String::new(),
                        task_id,
                    },
                )),
            })
            .await?;
            self.read_event().await
        }

        pub async fn next(&mut self) -> Result<generated::EventEnvelope, String> {
            self.read_event().await
        }
    }

    pub async fn run(command: Command) -> ExitCode {
        if matches!(command, Command::Resume { .. }) {
            eprintln!("unavailable: безопасный resume для этого Core-контракта не объявлен");
            return ExitCode::CoreUnavailable;
        }
        let mut client = match CoreClient::connect(0).await {
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
                let request = evohime_core::headless_core_cli::RunRequest {
                    schema_version: evohime_core::headless_core_cli::SCHEMA_VERSION,
                    prompt: prompt.clone(),
                    workspace: workspace.clone(),
                    output_mode: if json {
                        evohime_core::headless_core_cli::OutputMode::Ndjson
                    } else {
                        evohime_core::headless_core_cli::OutputMode::Human
                    },
                    approval_mode:
                        evohime_core::headless_core_cli::ApprovalMode::DenyIfApprovalRequired,
                    detach,
                };
                if evohime_core::headless_core_cli::validate_request(&request).is_err() {
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
            Command::Resume { .. } => unreachable!(),
        }
    }

    async fn watch_events(client: &mut CoreClient, run_id: &str, json: bool) -> ExitCode {
        loop {
            match client.next().await {
                Ok(event) => {
                    print_event(&event, run_id, json);
                    if evohime_core::headless_core_cli::is_terminal_event(&event.event_type) {
                        return if matches!(
                            event.event_type.as_str(),
                            "task.completed" | "workflow.completed"
                        ) {
                            ExitCode::Completed
                        } else if matches!(
                            event.event_type.as_str(),
                            "task.stopped" | "workflow.cancelled"
                        ) {
                            ExitCode::Cancelled
                        } else {
                            ExitCode::RunFailed
                        };
                    }
                }
                Err(error) => {
                    eprintln!("{error}; переподключение по cursor={}", client.sequence);
                    let cursor = client.sequence;
                    let mut replacement = None;
                    for _ in 0..5 {
                        match CoreClient::connect(cursor).await {
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
