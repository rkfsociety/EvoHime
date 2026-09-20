use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
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
