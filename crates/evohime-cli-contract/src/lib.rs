#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![deny(missing_docs)]
//! Versioned, dependency-light contract shared by the official `eva` client
//! and the Core runtime.
//!
//! Validate a request before handing it to the Core client:
//!
//! ```
//! use evohime_cli_contract::{validate_request, ApprovalMode, OutputMode, RunRequest, SCHEMA_VERSION};
//!
//! let request = RunRequest {
//!     schema_version: SCHEMA_VERSION,
//!     prompt: "Summarize this workspace".into(),
//!     workspace: "project".into(),
//!     output_mode: OutputMode::Ndjson,
//!     approval_mode: ApprovalMode::DenyIfApprovalRequired,
//!     detach: false,
//! };
//! validate_request(&request).unwrap();
//! ```

mod events;
mod request;

/// Event schema identifiers and terminal-event classification.
pub use events::{is_terminal_event, EVENT_SCHEMA};
/// Request limits, validation, and the versioned request types.
pub use request::{
    validate_request, ApprovalMode, Error, OutputMode, RunRequest, MAX_PROMPT_BYTES,
    MAX_RUN_ID_BYTES, MAX_WORKSPACE_BYTES, SCHEMA_VERSION,
};
