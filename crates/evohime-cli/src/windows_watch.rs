use evohime_cli::{event_matches_run, terminal_exit_code, ExitCode};
use evohime_cli_contract::ApprovalMode;

use crate::endpoint::{connect, CoreClient};
use crate::windows_event_output;
use evohime_desktop_ipc::generated;
use serde_json::Value;
use std::io::{self, IsTerminal, Write};

struct PendingApproval {
    approval_id: String,
    tool_name: String,
    permission: String,
    scope: String,
    summary: Option<String>,
    path: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    details: Option<String>,
}

pub(crate) fn approval_mode() -> ApprovalMode {
    if io::stdin().is_terminal() {
        ApprovalMode::Interactive
    } else {
        ApprovalMode::DenyIfApprovalRequired
    }
}

fn pending_approval(event: &generated::EventEnvelope) -> Option<PendingApproval> {
    let payload: Value = serde_json::from_slice(&event.payload).ok()?;
    let request = payload.get("ApprovalRequired")?.as_object()?;
    let approval_id = request.get("approval_id")?.as_str()?;
    let approval_id = uuid::Uuid::parse_str(approval_id).ok()?.to_string();
    let text = |name: &str| {
        request
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    };
    let tool_name = text("tool_name")?;
    let permission = text("permission")?;
    let scope = text("scope").unwrap_or_default();
    let preview = request.get("preview").and_then(Value::as_object);
    let preview_text = |name: &str| {
        preview
            .and_then(|preview| preview.get(name))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    };
    Some(PendingApproval {
        approval_id,
        tool_name,
        permission,
        scope,
        summary: preview_text("summary"),
        path: preview_text("path"),
        command: preview_text("command"),
        cwd: preview_text("cwd"),
        details: preview_text("details"),
    })
}

fn clipped(value: &str) -> String {
    value
        .chars()
        .take(1_024)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn ask_approval(request: &PendingApproval) -> bool {
    eprintln!("\nЕва запрашивает разрешение на действие:");
    eprintln!("  Инструмент: {}", clipped(&request.tool_name));
    eprintln!("  Разрешение: {}", clipped(&request.permission));
    if !request.scope.is_empty() {
        eprintln!("  Область: {}", clipped(&request.scope));
    }
    if let Some(summary) = &request.summary {
        eprintln!("  Действие: {}", clipped(summary));
    }
    if let Some(path) = &request.path {
        eprintln!("  Путь: {}", clipped(path));
    }
    if let Some(command) = &request.command {
        eprintln!("  Команда: {}", clipped(command));
    }
    if let Some(cwd) = &request.cwd {
        eprintln!("  Рабочая папка: {}", clipped(cwd));
    }
    if let Some(details) = &request.details {
        eprintln!("  Подробности: {}", clipped(details));
    }
    eprint!("Разрешить? [д/Н]: ");
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(
        answer.trim().to_lowercase().as_str(),
        "д" | "да" | "y" | "yes"
    )
}

pub(crate) async fn watch_events(
    client: &mut CoreClient,
    run_id: &str,
    json: bool,
    approval_mode: ApprovalMode,
) -> ExitCode {
    loop {
        match client.next().await {
            Ok(event) => {
                if !event_matches_run(&event.task_id, run_id) {
                    continue;
                }
                windows_event_output::print_event(&event, run_id, json);
                if event.event_type == "approval.required" {
                    let Some(request) = pending_approval(&event) else {
                        eprintln!(
                            "approval_unavailable: Core прислал некорректный запрос подтверждения"
                        );
                        let _ = client.stop(run_id.to_owned()).await;
                        return ExitCode::ApprovalUnavailable;
                    };
                    if approval_mode != ApprovalMode::Interactive {
                        let _ = client.resolve_approval(request.approval_id, false).await;
                        let _ = client.stop(run_id.to_owned()).await;
                        eprintln!(
                            "approval_unavailable: {} требует подтверждения; запустите eva из интерактивного терминала",
                            request.tool_name
                        );
                        return ExitCode::ApprovalUnavailable;
                    }
                    let granted = ask_approval(&request);
                    if let Err(error) = client.resolve_approval(request.approval_id, granted).await
                    {
                        eprintln!(
                            "core_unavailable: не удалось передать решение подтверждения ({error})"
                        );
                        let _ = client.stop(run_id.to_owned()).await;
                        return ExitCode::CoreUnavailable;
                    }
                }
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
                        Err(_) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_approval_details_from_core_event() {
        let event = generated::EventEnvelope {
            payload: serde_json::to_vec(&serde_json::json!({
                "ApprovalRequired": {
                    "approval_id": "4b67fc86-a06a-4ce6-b08a-4b856e1e4197",
                    "tool_name": "filesystem.write",
                    "permission": "FilesystemWrite",
                    "scope": "/tmp/project/hello.py",
                    "preview": {
                        "summary": "Create hello.py",
                        "path": "/tmp/project/hello.py"
                    }
                }
            }))
            .expect("approval event serializes"),
            ..Default::default()
        };

        let request = pending_approval(&event).expect("valid approval request");

        assert_eq!(request.approval_id, "4b67fc86-a06a-4ce6-b08a-4b856e1e4197");
        assert_eq!(request.tool_name, "filesystem.write");
        assert_eq!(request.permission, "FilesystemWrite");
        assert_eq!(request.scope, "/tmp/project/hello.py");
        assert_eq!(request.summary.as_deref(), Some("Create hello.py"));
        assert_eq!(request.path.as_deref(), Some("/tmp/project/hello.py"));
    }

    #[test]
    fn rejects_malformed_approval_payloads_without_an_approval_id() {
        let event = generated::EventEnvelope {
            payload: br#"{"ApprovalRequired":{"tool_name":"filesystem.write"}}"#.to_vec(),
            ..Default::default()
        };

        assert!(pending_approval(&event).is_none());
    }
}
