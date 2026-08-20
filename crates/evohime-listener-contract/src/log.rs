use crate::{
    error::AmbientErrorCode,
    ids::{DeviceId, EngineVersion, EpisodeId, ProposalId},
    state::{ListeningReason, ListeningState},
};
use serde::Serialize;

/// Every field name an ambient log record may carry.
///
/// `StructuredLogger::write` accepts arbitrary JSON, so an allow-list cannot
/// be bolted onto it.  The ambient path therefore never touches the raw
/// logger: it emits [`AmbientLogEvent`], whose fields are fixed by the type
/// system.  There is no free-text field to smuggle speech through — passing a
/// phrase is a compile error, not a failing test.
pub const ALLOWED_LOG_FIELDS: &[&str] = &[
    "state",
    "reason",
    "active_device_id",
    "status",
    "engine_version",
    "progress_pct",
    "episode_id",
    "started_at_ms",
    "utterance_count",
    "extraction_state",
    "deleted_count",
    "trigger",
    "proposal_id",
    "kind",
    "proposal_state",
    "code",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineStatus {
    Idle,
    Downloading,
    Verifying,
    Approved,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionState {
    Disabled,
    Pending,
    Done,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionTrigger {
    Manual,
    Policy,
    ForgetWindow,
}

/// The two effects ambient proactivity is allowed to produce.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    Suggestion,
    Reminder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalState {
    Pending,
    Accepted,
    Dismissed,
    Expired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// Typed ambient log/event record.
///
/// Serializes to a flat object of allow-listed fields; the record name comes
/// from [`AmbientLogEvent::event_name`].  Transcript text, its hash, the
/// foreground process name and the window title have no representation here on
/// purpose: a short phrase is brute-forced from its hash in seconds, so a hash
/// counts as the content itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AmbientLogEvent {
    State {
        state: ListeningState,
        reason: ListeningReason,
        active_device_id: Option<DeviceId>,
    },
    Engine {
        status: EngineStatus,
        engine_version: Option<EngineVersion>,
        progress_pct: Option<u8>,
    },
    Transcript {
        episode_id: EpisodeId,
        started_at_ms: u64,
        utterance_count: u32,
        extraction_state: ExtractionState,
    },
    Retention {
        deleted_count: u32,
        trigger: RetentionTrigger,
    },
    Proposal {
        proposal_id: ProposalId,
        kind: ProposalKind,
        proposal_state: ProposalState,
    },
    Error {
        code: AmbientErrorCode,
        state: ListeningState,
    },
}

impl AmbientLogEvent {
    pub const fn event_name(&self) -> &'static str {
        match self {
            AmbientLogEvent::State { .. } => "ambient.state",
            AmbientLogEvent::Engine { .. } => "ambient.engine",
            AmbientLogEvent::Transcript { .. } => "ambient.transcript",
            AmbientLogEvent::Retention { .. } => "ambient.retention",
            AmbientLogEvent::Proposal { .. } => "ambient.proposal",
            AmbientLogEvent::Error { .. } => "ambient.error",
        }
    }

    /// Field names this variant emits, in serialization order.
    pub const fn field_names(&self) -> &'static [&'static str] {
        match self {
            AmbientLogEvent::State { .. } => &["state", "reason", "active_device_id"],
            AmbientLogEvent::Engine { .. } => &["status", "engine_version", "progress_pct"],
            AmbientLogEvent::Transcript { .. } => &[
                "episode_id",
                "started_at_ms",
                "utterance_count",
                "extraction_state",
            ],
            AmbientLogEvent::Retention { .. } => &["deleted_count", "trigger"],
            AmbientLogEvent::Proposal { .. } => &["proposal_id", "kind", "proposal_state"],
            AmbientLogEvent::Error { .. } => &["code", "state"],
        }
    }

    pub const fn level(&self) -> LogLevel {
        match self {
            AmbientLogEvent::Error { .. } => LogLevel::Error,
            AmbientLogEvent::State { state, .. } => {
                if state.is_degraded() {
                    LogLevel::Warn
                } else {
                    LogLevel::Info
                }
            }
            AmbientLogEvent::Engine {
                status: EngineStatus::Failed,
                ..
            } => LogLevel::Warn,
            _ => LogLevel::Info,
        }
    }
}

/// Sink implemented by the listener and by Core over their own journals.
///
/// The ambient path holds one of these instead of a raw structured logger, so
/// there is no reachable call that writes arbitrary JSON from ambient code.
pub trait AmbientLogSink {
    fn record(&self, event: &AmbientLogEvent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn samples() -> Vec<AmbientLogEvent> {
        vec![
            AmbientLogEvent::State {
                state: ListeningState::Listening,
                reason: ListeningReason::UserRequest,
                active_device_id: Some(DeviceId::new("mic-0").unwrap()),
            },
            AmbientLogEvent::State {
                state: ListeningState::DeviceConflict,
                reason: ListeningReason::DeviceConflict,
                active_device_id: None,
            },
            AmbientLogEvent::Engine {
                status: EngineStatus::Downloading,
                engine_version: Some(EngineVersion::new("whisper-base-q5_1").unwrap()),
                progress_pct: Some(42),
            },
            AmbientLogEvent::Transcript {
                episode_id: EpisodeId::new("ep-1").unwrap(),
                started_at_ms: 1_700_000_000_000,
                utterance_count: 7,
                extraction_state: ExtractionState::Pending,
            },
            AmbientLogEvent::Retention {
                deleted_count: 3,
                trigger: RetentionTrigger::ForgetWindow,
            },
            AmbientLogEvent::Proposal {
                proposal_id: ProposalId::new("prop-1").unwrap(),
                kind: ProposalKind::Reminder,
                proposal_state: ProposalState::Pending,
            },
            AmbientLogEvent::Error {
                code: AmbientErrorCode::StorageFailed,
                state: ListeningState::Listening,
            },
        ]
    }

    #[test]
    fn every_variant_serializes_only_allow_listed_fields() {
        for event in samples() {
            let value = serde_json::to_value(&event).unwrap();
            let object = value.as_object().expect("ambient event is an object");
            for key in object.keys() {
                assert!(
                    ALLOWED_LOG_FIELDS.contains(&key.as_str()),
                    "{} emitted field outside the allow-list: {key}",
                    event.event_name()
                );
            }
            let mut emitted: Vec<&str> = object.keys().map(String::as_str).collect();
            let mut declared: Vec<&str> = event.field_names().to_vec();
            emitted.sort_unstable();
            declared.sort_unstable();
            assert_eq!(
                emitted,
                declared,
                "{} field list drifted",
                event.event_name()
            );
        }
    }

    #[test]
    fn allow_list_has_no_content_bearing_field() {
        for forbidden in [
            "text",
            "text_hash",
            "transcript",
            "utterance",
            "process_name",
            "window_title",
            "message",
            "details",
            "summary",
            "canonical_subject",
        ] {
            assert!(
                !ALLOWED_LOG_FIELDS.contains(&forbidden),
                "allow-list leaks {forbidden}"
            );
        }
    }

    #[test]
    fn every_string_value_is_an_opaque_token() {
        for event in samples() {
            let value = serde_json::to_value(&event).unwrap();
            for (key, field) in value.as_object().unwrap() {
                if let Value::String(text) = field {
                    assert!(
                        text.chars().all(|ch| ch.is_ascii_alphanumeric()
                            || matches!(ch, '-' | '_' | '.' | ':' | '+')),
                        "field {key} carries free text"
                    );
                }
            }
        }
    }

    #[test]
    fn event_names_are_stable_and_unique() {
        let mut names: Vec<&str> = samples().iter().map(|e| e.event_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names,
            vec![
                "ambient.engine",
                "ambient.error",
                "ambient.proposal",
                "ambient.retention",
                "ambient.state",
                "ambient.transcript",
            ]
        );
    }

    #[test]
    fn degraded_and_failed_records_are_not_info() {
        assert_eq!(
            AmbientLogEvent::State {
                state: ListeningState::DeviceDisconnected,
                reason: ListeningReason::DeviceDisconnected,
                active_device_id: None,
            }
            .level(),
            LogLevel::Warn
        );
        assert_eq!(
            AmbientLogEvent::Error {
                code: AmbientErrorCode::PermissionDenied,
                state: ListeningState::Denied,
            }
            .level(),
            LogLevel::Error
        );
        assert_eq!(
            AmbientLogEvent::Retention {
                deleted_count: 0,
                trigger: RetentionTrigger::Policy,
            }
            .level(),
            LogLevel::Info
        );
    }
}
