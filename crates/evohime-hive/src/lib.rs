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

/// Read-only architecture scanner and versioned city-map contract
/// (plan 199.1).
pub mod city_map;
