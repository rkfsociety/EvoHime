//! Platform-neutral Core protocol client used by the Windows transport.
//!
//! Keeping framing, authentication and command envelopes independent from the
//! operating-system endpoint lets Linux CI exercise the real CLI/Core
//! contract without pretending that the Windows named pipe is portable.

mod auth;
mod client;
mod commands;
mod read;

#[cfg(test)]
mod tests;

pub use client::CoreClient;
