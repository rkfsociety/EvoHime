use serde::{Deserialize, Serialize};

/// Current wire-contract version for [`RunRequest`].
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum UTF-8 byte length of a task prompt.
pub const MAX_PROMPT_BYTES: usize = 128 * 1024;
/// Maximum UTF-8 byte length of a workspace identifier/path.
pub const MAX_WORKSPACE_BYTES: usize = 512;
/// Maximum UTF-8 byte length reserved for a run identifier.
pub const MAX_RUN_ID_BYTES: usize = 128;

/// Selects how the CLI renders events and terminal output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputMode {
    /// Render readable human-oriented output.
    Human,
    /// Emit one JSON event per line.
    Ndjson,
    /// Suppress non-terminal output.
    Quiet,
}

/// Selects the policy for tool actions that require user approval.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalMode {
    /// Request approvals interactively when a UI is available.
    Interactive,
    /// Fail the action when it requires approval.
    DenyIfApprovalRequired,
    /// Resolve approvals using the configured Core policy profile.
    UseApprovalPolicyProfile,
    /// Delegate approval decisions to the authenticated desktop broker.
    DesktopBrokered,
}

/// Validation and availability failures for a headless CLI request.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The request uses an unsupported schema version.
    #[error("unsupported CLI contract version")]
    UnsupportedVersion,
    /// A request field is empty, oversized, or contains control characters.
    #[error("bounded CLI input is invalid")]
    InvalidInput,
    /// An approval was required but no interactive route was available.
    #[error("non-interactive approval is unavailable")]
    ApprovalUnavailable,
    /// The Core process or compatible Core protocol is unavailable.
    #[error("Core is unavailable or the protocol is incompatible")]
    CoreUnavailable,
}

/// Versioned, bounded input to a headless Core task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRequest {
    /// Contract version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Non-empty task prompt, bounded by [`MAX_PROMPT_BYTES`].
    pub prompt: String,
    /// Non-empty workspace selector, bounded by [`MAX_WORKSPACE_BYTES`].
    pub workspace: String,
    /// Output projection requested from the client.
    pub output_mode: OutputMode,
    /// Approval handling policy requested for this run.
    pub approval_mode: ApprovalMode,
    /// Whether the client may detach after the run starts.
    pub detach: bool,
}

/// Checks the schema version and bounded, control-character-free text fields.
///
/// This validation does not resolve the workspace or grant permissions; Core
/// must perform its own authorization and path checks.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_request() -> RunRequest {
        RunRequest {
            schema_version: SCHEMA_VERSION,
            prompt: "check".into(),
            workspace: "workspace".into(),
            output_mode: OutputMode::Ndjson,
            approval_mode: ApprovalMode::DenyIfApprovalRequired,
            detach: false,
        }
    }

    #[test]
    fn validates_bounded_request() {
        assert!(validate_request(&valid_request()).is_ok());
    }

    #[test]
    fn rejects_schema_and_byte_bound_violations() {
        let mut schema = valid_request();
        schema.schema_version = SCHEMA_VERSION + 1;
        assert_eq!(validate_request(&schema), Err(Error::InvalidInput));

        let mut prompt = valid_request();
        prompt.prompt = "x".repeat(MAX_PROMPT_BYTES + 1);
        assert_eq!(validate_request(&prompt), Err(Error::InvalidInput));

        let mut workspace = valid_request();
        workspace.workspace = "x".repeat(MAX_WORKSPACE_BYTES + 1);
        assert_eq!(validate_request(&workspace), Err(Error::InvalidInput));
    }

    #[test]
    fn rejects_control_bytes_in_text_fields() {
        let mut prompt = valid_request();
        prompt.prompt = "safe\nunsafe".into();
        assert_eq!(validate_request(&prompt), Err(Error::InvalidInput));

        let mut workspace = valid_request();
        workspace.workspace = "workspace\tname".into();
        assert_eq!(validate_request(&workspace), Err(Error::InvalidInput));
    }
}
