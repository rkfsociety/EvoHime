//! Stable, redaction-aware contract shared by the `eva` headless client.

pub mod protocol;

mod input;
mod redaction;

use serde::Serialize;
use serde_json::Value;

pub use redaction::redact_payload;

pub const CLI_SCHEMA: &str = "evohime.cli.event/v1";
pub const MAX_PROMPT_BYTES: usize = 128 * 1024;
pub const MAX_WORKSPACE_BYTES: usize = 512;
pub const MAX_EVENT_BYTES: usize = 256 * 1024;
const STDIN_PREFIX: &str = "\n\nInput from stdin:\n";

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Completed = 0,
    RunFailed = 1,
    InvalidInvocation = 2,
    ApprovalUnavailable = 3,
    CredentialsUnavailable = 4,
    PolicyDenied = 5,
    TimeoutOrBudget = 6,
    CoreUnavailable = 7,
    Cancelled = 8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run {
        prompt: String,
        workspace: String,
        workflow: Option<String>,
        json: bool,
        detach: bool,
    },
    Status {
        task_id: String,
        json: bool,
    },
    Watch {
        task_id: String,
        json: bool,
    },
    Cancel {
        task_id: String,
        json: bool,
    },
    Resume {
        task_id: String,
        json: bool,
    },
    Doctor {
        json: bool,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("usage: eva run [<prompt>] [--workflow <id>] [--workspace <path>] [--stdin] [--json] [--detach]")]
    Usage,
    #[error("unknown command or option")]
    UnknownOption,
    #[error("value is missing or exceeds the CLI bound")]
    InvalidValue,
}

fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.bytes().any(|byte| byte.is_ascii_control())
}

pub fn parse_args(args: &[String]) -> Result<Command, ParseError> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(ParseError::Usage);
    };
    let json = args.iter().any(|arg| arg == "--json");
    match command {
        "run" => {
            let mut prompt = None;
            let mut workspace = std::env::current_dir()
                .map_err(|_| ParseError::InvalidValue)?
                .display()
                .to_string();
            let mut detach = false;
            let mut stdin_requested = false;
            let mut workflow = None;
            let mut index = 1;
            while index < args.len() {
                match args[index].as_str() {
                    "--json" => {}
                    "--detach" => detach = true,
                    "--stdin" => {
                        stdin_requested = true;
                    }
                    "--workspace" => {
                        index += 1;
                        workspace = args
                            .get(index)
                            .filter(|value| !value.starts_with('-'))
                            .cloned()
                            .ok_or(ParseError::InvalidValue)?;
                    }
                    "--workflow" => {
                        index += 1;
                        workflow = Some(
                            args.get(index)
                                .filter(|value| !value.starts_with('-') && bounded(value, 128))
                                .cloned()
                                .ok_or(ParseError::InvalidValue)?,
                        );
                    }
                    value if value.starts_with('-') => return Err(ParseError::UnknownOption),
                    value if prompt.is_none() => prompt = Some(value.to_string()),
                    _ => return Err(ParseError::Usage),
                }
                index += 1;
            }
            let mut prompt = prompt
                .or_else(|| workflow.as_ref().map(|id| format!("Run workflow {id}")))
                .filter(|value| bounded(value, MAX_PROMPT_BYTES))
                .ok_or(ParseError::InvalidValue)?;
            if stdin_requested {
                let remaining = MAX_PROMPT_BYTES
                    .checked_sub(prompt.len() + STDIN_PREFIX.len())
                    .ok_or(ParseError::InvalidValue)?;
                let stdin = std::io::stdin();
                let mut reader = stdin.lock();
                let input = input::read_bounded(&mut reader, remaining)
                    .map_err(|_| ParseError::InvalidValue)?;
                if !bounded(&input, remaining) {
                    return Err(ParseError::InvalidValue);
                }
                prompt = format!("{prompt}{STDIN_PREFIX}{input}");
                if !bounded(&prompt, MAX_PROMPT_BYTES) {
                    return Err(ParseError::InvalidValue);
                }
            }
            if !bounded(&workspace, MAX_WORKSPACE_BYTES) {
                return Err(ParseError::InvalidValue);
            }
            Ok(Command::Run {
                prompt,
                workspace,
                workflow,
                json,
                detach,
            })
        }
        "status" | "watch" | "cancel" | "resume" => {
            let task_id = args
                .get(1)
                .filter(|value| bounded(value, 128))
                .cloned()
                .ok_or(ParseError::InvalidValue)?;
            if args.iter().skip(2).any(|arg| arg != "--json") {
                return Err(ParseError::UnknownOption);
            }
            Ok(match command {
                "status" => Command::Status { task_id, json },
                "watch" => Command::Watch { task_id, json },
                "cancel" => Command::Cancel { task_id, json },
                _ => Command::Resume { task_id, json },
            })
        }
        "doctor" if args.iter().skip(1).all(|arg| arg == "--json") => Ok(Command::Doctor { json }),
        _ => Err(ParseError::Usage),
    }
}

#[derive(Debug, Serialize)]
pub struct CliEvent<'a> {
    pub schema: &'static str,
    pub sequence: u64,
    pub kind: &'a str,
    pub run_id: &'a str,
    pub payload: Value,
}

pub fn emit(event: &CliEvent<'_>) -> String {
    serde_json::to_string(event).unwrap_or_else(|_| {
        "{\"schema\":\"evohime.cli.event/v1\",\"kind\":\"internal_error\"}".to_string()
    })
}

pub fn terminal_exit_code(event_type: &str) -> Option<ExitCode> {
    if !evohime_cli_contract::is_terminal_event(event_type) {
        return None;
    }
    Some(match event_type {
        "task.completed" | "workflow.completed" => ExitCode::Completed,
        "task.stopped" | "workflow.cancelled" => ExitCode::Cancelled,
        "task.failed" | "workflow.failed" => ExitCode::RunFailed,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_bounded_run_and_modes() {
        let args = vec![
            "run".into(),
            "hello".into(),
            "--json".into(),
            "--detach".into(),
        ];
        assert_eq!(
            parse_args(&args).unwrap(),
            Command::Run {
                prompt: "hello".into(),
                workspace: std::env::current_dir().unwrap().display().to_string(),
                workflow: None,
                json: true,
                detach: true
            }
        );
    }
    #[test]
    fn redacts_sensitive_projection_keys() {
        let value = redact_payload(br#"{"prompt":"x","secret":"y","status":"done"}"#);
        assert_eq!(value, serde_json::json!({"status":"done"}));
    }

    #[test]
    fn maps_terminal_events_to_stable_exit_codes() {
        for (event_type, expected) in [
            ("task.completed", ExitCode::Completed),
            ("workflow.completed", ExitCode::Completed),
            ("task.stopped", ExitCode::Cancelled),
            ("workflow.cancelled", ExitCode::Cancelled),
            ("task.failed", ExitCode::RunFailed),
            ("workflow.failed", ExitCode::RunFailed),
        ] {
            assert_eq!(terminal_exit_code(event_type), Some(expected));
        }
        assert_eq!(terminal_exit_code("task.progress"), None);
    }

    #[test]
    fn parses_read_only_run_controls() {
        for (name, expected) in [
            (
                "status",
                Command::Status {
                    task_id: "run-1".into(),
                    json: true,
                },
            ),
            (
                "watch",
                Command::Watch {
                    task_id: "run-1".into(),
                    json: true,
                },
            ),
            (
                "cancel",
                Command::Cancel {
                    task_id: "run-1".into(),
                    json: true,
                },
            ),
            (
                "resume",
                Command::Resume {
                    task_id: "run-1".into(),
                    json: true,
                },
            ),
        ] {
            let args = vec![name.into(), "run-1".into(), "--json".into()];
            assert_eq!(parse_args(&args).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_option_names_as_missing_values() {
        for args in [
            vec!["run", "prompt", "--workspace", "--json"],
            vec!["run", "--workflow", "--json"],
            vec!["run", "prompt", "--workflow", "--detach"],
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(parse_args(&args), Err(ParseError::InvalidValue));
        }
    }
}
