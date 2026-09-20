//! Platform-neutral Core protocol client used by the Windows transport.
//!
//! Keeping framing, authentication and command envelopes independent from the
//! operating-system endpoint lets Linux CI exercise the real CLI/Core
//! contract without pretending that the Windows named pipe is portable.

mod auth;
mod client;
mod read;
mod task_commands;
mod workflow_commands;

#[cfg(test)]
mod tests;

pub use client::CoreClient;
