//! Versioned contract for the official `eva` Core client.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const EVENT_SCHEMA: &str = "evohime.cli.event/v1";
pub const MAX_PROMPT_BYTES: usize = 128 * 1024;
pub const MAX_WORKSPACE_BYTES: usize = 512;
pub const MAX_RUN_ID_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Ndjson,
    Quiet,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalMode {
    Interactive,
    DenyIfApprovalRequired,
    UseApprovalPolicyProfile,
    DesktopBrokered,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("unsupported CLI contract version")]
    UnsupportedVersion,
    #[error("bounded CLI input is invalid")]
    InvalidInput,
    #[error("non-interactive approval is unavailable")]
    ApprovalUnavailable,
    #[error("Core is unavailable or the protocol is incompatible")]
    CoreUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRequest {
    pub schema_version: u32,
    pub prompt: String,
    pub workspace: String,
    pub output_mode: OutputMode,
    pub approval_mode: ApprovalMode,
    pub detach: bool,
}

pub fn validate_request(request: &RunRequest) -> Result<(), Error> {
    if request.schema_version != SCHEMA_VERSION
        || request.prompt.is_empty()
        || request.prompt.len() > MAX_PROMPT_BYTES
        || request.workspace.is_empty()
        || request.workspace.len() > MAX_WORKSPACE_BYTES
        || request.prompt.bytes().any(|byte| byte.is_ascii_control())
        || request
            .workspace
            .bytes()
            .any(|byte| byte.is_ascii_control())
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

pub fn is_terminal_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "task.completed"
            | "task.failed"
            | "task.stopped"
            | "workflow.completed"
            | "workflow.failed"
            | "workflow.cancelled"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_bounded_request() {
        let request = RunRequest {
            schema_version: 1,
            prompt: "check".into(),
            workspace: "workspace".into(),
            output_mode: OutputMode::Ndjson,
            approval_mode: ApprovalMode::DenyIfApprovalRequired,
            detach: false,
        };
        assert!(validate_request(&request).is_ok());
    }
    #[test]
    fn terminal_events_are_explicit() {
        assert!(is_terminal_event("task.completed"));
        assert!(!is_terminal_event("task.progress"));
    }
}
