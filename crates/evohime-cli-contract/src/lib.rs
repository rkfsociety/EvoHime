#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
//! Versioned, dependency-light contract shared by the official `eva` client
//! and the Core runtime.

mod events;
mod request;

pub use events::{is_terminal_event, EVENT_SCHEMA};
pub use request::{
    validate_request, ApprovalMode, Error, OutputMode, RunRequest, MAX_PROMPT_BYTES,
    MAX_RUN_ID_BYTES, MAX_WORKSPACE_BYTES, SCHEMA_VERSION,
};
