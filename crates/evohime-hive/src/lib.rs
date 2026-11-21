#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Hive domain: autonomous Hive observation daemon, Hive settings, observation projections,
//! worktree isolation, and dedicated Hive permission policy.

pub mod observation;
pub mod permission_session;
pub mod pulse;
pub mod pulse_boundary;
pub mod settings;
pub mod state;
pub mod worktree;
