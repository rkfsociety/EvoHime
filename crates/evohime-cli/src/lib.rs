//! Stable, redaction-aware contract shared by the `eva` headless client.

pub use evohime_cli_protocol as protocol;

mod args;
mod command;
mod events;
mod input;
mod redaction;

pub use args::parse_args;
pub use command::{Command, ParseError};
pub use events::{
    emit, event_matches_run, terminal_exit_code, CliEvent, ExitCode, CLI_SCHEMA, MAX_EVENT_BYTES,
};
pub use evohime_cli_contract::{MAX_PROMPT_BYTES, MAX_RUN_ID_BYTES, MAX_WORKSPACE_BYTES};
pub use redaction::redact_payload;
