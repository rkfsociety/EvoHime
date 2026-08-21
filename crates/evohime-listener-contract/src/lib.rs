//! Bounded ambient-listening contract shared by Core, the listener process and
//! the shell.
//!
//! This crate is deliberately side-effect free: no filesystem, no clock, no
//! processes, no logging sink.  It describes the listening state machine, the
//! immutable capture limits, the ambient policy schema, the proactivity budget
//! snapshot, the closed set of error codes and the typed logging facade.
//!
//! Privacy invariants encoded by types, not by discipline:
//!
//! - no contract type carries recognized speech, a hash of speech, a process
//!   name or a window title, so ambient logging cannot leak content even by
//!   accident — passing free text is a compile error, not a failing test;
//! - identifiers are bounded newtypes with a restricted charset, so a caller
//!   cannot smuggle a sentence through an `id` field;
//! - the renderer may display a snapshot, but cannot raise a limit: every
//!   bound is validated here before it reaches the listener.

mod error;
mod ids;
mod limits;
mod log;
mod policy;
mod proactivity;
mod state;

pub use error::{AmbientErrorCode, ContractError};
pub use ids::{DeviceId, EngineVersion, EpisodeId, ProposalId, SubjectKey, MAX_ID_BYTES};
pub use limits::{
    AmbientLimits, MAX_DEDUP_WINDOW_MS, MAX_EPISODE_MS, MAX_FRAME_MS, MAX_UTTERANCE_MS,
    MAX_WINDOW_MS, MIN_FRAME_MS,
};
pub use log::{
    AmbientLogEvent, AmbientLogSink, EngineStatus, ExtractionState, LogLevel, ProposalKind,
    ProposalState, RetentionTrigger, ALLOWED_LOG_FIELDS,
};
pub use policy::{
    AmbientPolicy, QuietHours, DEFAULT_RETENTION_DAYS, MAX_BLOCKLIST_ENTRIES, MAX_PATTERN_BYTES,
    MAX_PATTERN_WILDCARDS, MAX_QUIET_HOURS, MAX_RETENTION_DAYS, MINUTES_PER_DAY,
};
pub use proactivity::{
    ProactivityBudget, ProactivityCounters, ProactivityDenial, MAX_PROPOSALS_PER_DAY,
    MAX_PROPOSALS_PER_HOUR, MIN_PROPOSAL_INTERVAL_MS,
};
pub use state::{ListeningReason, ListeningState};
