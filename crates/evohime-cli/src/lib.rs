#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
//! Stable, redaction-aware contract shared by the `eva` headless client.

pub use evohime_cli_protocol as protocol;

mod args;
mod command;
mod event_model;
mod events;
mod exit_code;
mod input;
mod redaction;
mod redaction_policy;

#[cfg(test)]
mod args_tests;
#[cfg(test)]
mod event_tests;
#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod redaction_tests;

pub use args::parse_args;
pub use command::{Command, ParseError};
pub use event_model::{CliEvent, CLI_SCHEMA, MAX_EVENT_BYTES};
pub use events::{emit, event_matches_run};
pub use evohime_cli_contract::{MAX_PROMPT_BYTES, MAX_RUN_ID_BYTES, MAX_WORKSPACE_BYTES};
pub use exit_code::{terminal_exit_code, ExitCode};
pub use redaction::redact_payload;
