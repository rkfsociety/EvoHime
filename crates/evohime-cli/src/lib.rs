#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![deny(missing_docs)]
//! Stable, redaction-aware contract shared by the `eva` headless client.
//!
//! ```
//! use evohime_cli::{parse_args, Command};
//!
//! let args = vec!["run".to_owned(), "summarize".to_owned(), "--workspace".to_owned(), ".".to_owned()];
//! let command = parse_args(&args).unwrap();
//! assert!(matches!(command, Command::Run { .. }));
//! ```

/// Authenticated Core client and protocol envelope types.
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

/// Parses a headless CLI argument vector into a typed command.
pub use args::parse_args;
/// Parsed command variants and bounded argument errors.
pub use command::{Command, ParseError};
/// Event JSON projection and its byte/schema bounds.
pub use event_model::{CliEvent, CLI_SCHEMA, MAX_EVENT_BYTES};
/// Event serialization and run-correlation helpers.
pub use events::{emit, event_matches_run};
/// Shared limits for prompts, run identifiers, and workspace identifiers.
pub use evohime_cli_contract::{MAX_PROMPT_BYTES, MAX_RUN_ID_BYTES, MAX_WORKSPACE_BYTES};
/// Stable terminal exit code mapping.
pub use exit_code::{terminal_exit_code, ExitCode};
/// Redacts event payloads before emitting or displaying them.
pub use redaction::redact_payload;
