use crate::{
    error::AmbientErrorCode,
    ids::{AppId, CommandId, DeviceId, EngineVersion, EpisodeId, ProposalId, SubjectKey},
    state::{ListeningReason, ListeningState},
};
use serde::{Deserialize, Serialize};

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
    "subject_key",
    "proposal_state",
    "command_id",
    "app_id",
    "command_state",
    "code",
];

/// Lifecycle status of the optional speech engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineStatus {
    /// Engine is idle and not performing an update.
    Idle,
    /// Model or engine files are being downloaded.
    Downloading,
    /// Downloaded files are being checked before activation.
    Verifying,
    /// Engine version passed validation and may be used.
    Approved,
    /// Download, validation, or activation failed.
    Failed,
}

/// Extraction lifecycle of one ambient episode.
///
/// The same four values are the `CHECK` constraint of `ambient_episodes`
/// in storage schema v25: the wire form below is the only spelling, so the
/// table and the log cannot drift apart.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionState {
    /// Transcript extraction is disabled by policy.
    Disabled,
    /// Transcript is queued for extraction.
    Pending,
    /// Transcript extraction completed.
    Done,
    /// Transcript extraction failed.
    Failed,
}

impl ExtractionState {
    /// All extraction states in declaration order.
    pub const ALL: [ExtractionState; 4] = [
        ExtractionState::Disabled,
        ExtractionState::Pending,
        ExtractionState::Done,
        ExtractionState::Failed,
    ];

    /// Wire and storage form.
    pub const fn as_str(self) -> &'static str {
        match self {
            ExtractionState::Disabled => "disabled",
            ExtractionState::Pending => "pending",
            ExtractionState::Done => "done",
            ExtractionState::Failed => "failed",
        }
    }

    /// Parses a stored value; an unknown string stays unknown instead of
    /// being coerced into a neighbouring meaning.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == value)
    }
}

/// Reason a retention operation removed ambient records.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionTrigger {
    /// User explicitly requested deletion.
    Manual,
    /// Retention policy expired the record.
    Policy,
    /// Record passed the configured forget window.
    ForgetWindow,
}

/// The two effects ambient proactivity is allowed to produce.
///
/// This is the closed list of 04.7: a card in the UI and a non-executable
/// reminder. Running a task, calling a tool, writing a file or going to the
/// network have no representation here, so a proactive effect outside the
/// list is unrepresentable rather than merely forbidden.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    /// Non-executable suggestion shown as a decision card.
    Suggestion,
    /// Non-executable reminder shown as a decision card.
    Reminder,
}

impl ProposalKind {
    /// All supported proposal kinds in declaration order.
    pub const ALL: [ProposalKind; 2] = [ProposalKind::Suggestion, ProposalKind::Reminder];

    /// Wire and storage form; the same two spellings are the `CHECK`
    /// constraint of `ambient_proposals.kind` in schema v26.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProposalKind::Suggestion => "suggestion",
            ProposalKind::Reminder => "reminder",
        }
    }

    /// Parses a stable wire value; unknown values are rejected.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

/// Что услышанная команда просит сделать.
///
/// Список закрыт по той же причине, что и [`ProposalKind`]: команда, которую
/// Ева распознала на слух, обязана попасть в заранее известную клетку. «Всё
/// остальное» здесь не вариант перечисления, а отсутствие команды — такая
/// фраза остаётся обычным транскриптом.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCommandKind {
    /// Открыть приложение из каталога.
    OpenApp,
}

impl VoiceCommandKind {
    /// All supported ambient voice-command kinds.
    pub const ALL: [VoiceCommandKind; 1] = [VoiceCommandKind::OpenApp];

    /// Returns the stable wire and storage representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            VoiceCommandKind::OpenApp => "open_app",
        }
    }

    /// Parses a stable wire value; unknown values are rejected.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

/// Жизненный цикл одной услышанной команды.
///
/// `Pending` — карточка ждёт клика; всё остальное терминально. Отдельное
/// состояние `Failed` существует потому, что «пользователь отказался» и «не
/// удалось запустить» — разные события для человека: первое он выбрал сам,
/// второе обязано показать причину.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCommandState {
    /// A decision card is waiting for the user.
    Pending,
    /// The requested application was launched.
    Launched,
    /// The user declined the command.
    Declined,
    /// The command expired before a decision.
    Expired,
    /// Launch failed after the user approved the command.
    Failed,
}

impl VoiceCommandState {
    /// All voice-command lifecycle states in declaration order.
    pub const ALL: [VoiceCommandState; 5] = [
        VoiceCommandState::Pending,
        VoiceCommandState::Launched,
        VoiceCommandState::Declined,
        VoiceCommandState::Expired,
        VoiceCommandState::Failed,
    ];

    /// Returns the stable wire and storage representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            VoiceCommandState::Pending => "pending",
            VoiceCommandState::Launched => "launched",
            VoiceCommandState::Declined => "declined",
            VoiceCommandState::Expired => "expired",
            VoiceCommandState::Failed => "failed",
        }
    }

    /// Parses a stable wire value; unknown values are rejected.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == value)
    }

    /// Returns whether this command state has no further transitions.
    pub const fn is_terminal(self) -> bool {
        !matches!(self, VoiceCommandState::Pending)
    }
}

/// Lifecycle of one bounded proposal.
///
/// Five states, and `ambient_proposals.state` in schema v26 carries the same
/// five spellings. `Muted` is terminal for this card *and* records that the
/// subject must not be proposed again; `Expired` is what an unanswered card
/// becomes after 24 hours and what a proposal becomes when its source episode
/// is deleted.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalState {
    /// A proposal card is awaiting the user's decision.
    Proposed,
    /// The user accepted the proposal.
    Accepted,
    /// The user declined the proposal.
    Declined,
    /// The user muted future proposals for this subject.
    Muted,
    /// The proposal expired or its source episode was removed.
    Expired,
}

impl ProposalState {
    /// All proposal lifecycle states in declaration order.
    pub const ALL: [ProposalState; 5] = [
        ProposalState::Proposed,
        ProposalState::Accepted,
        ProposalState::Declined,
        ProposalState::Muted,
        ProposalState::Expired,
    ];

    /// Returns the stable wire and storage representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProposalState::Proposed => "proposed",
            ProposalState::Accepted => "accepted",
            ProposalState::Declined => "declined",
            ProposalState::Muted => "muted",
            ProposalState::Expired => "expired",
        }
    }

    /// Parses a stable wire value; unknown values are rejected.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == value)
    }

    /// A resolved proposal never moves again: a second click cannot turn a
    /// declined card into an accepted one.
    pub const fn is_terminal(self) -> bool {
        !matches!(self, ProposalState::Proposed)
    }
}

/// Severity assigned to a typed ambient event.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Informational event.
    Info,
    /// Degraded or failed subsystem state.
    Warn,
    /// Operation failed and needs error handling.
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
    /// Listening lifecycle state or capture failure changed.
    State {
        /// New listening lifecycle state.
        state: ListeningState,
        /// Closed-set reason for the state.
        reason: ListeningReason,
        /// Active opaque device identifier, if one is selected.
        active_device_id: Option<DeviceId>,
    },
    /// Speech engine status or download progress changed.
    Engine {
        /// Engine lifecycle status.
        status: EngineStatus,
        /// Opaque engine build identifier, when available.
        engine_version: Option<EngineVersion>,
        /// Download progress percentage, when a download is active.
        progress_pct: Option<u8>,
    },
    /// An ambient episode or its transcript extraction changed.
    Transcript {
        /// Opaque identifier of the ambient episode.
        episode_id: EpisodeId,
        /// Unix timestamp in milliseconds when the episode began.
        started_at_ms: u64,
        /// Number of utterances recorded in the episode.
        utterance_count: u32,
        /// Transcript extraction lifecycle state.
        extraction_state: ExtractionState,
    },
    /// Ambient records were removed by a retention operation.
    Retention {
        /// Number of records removed by the retention operation.
        deleted_count: u32,
        /// Closed-set reason for the retention operation.
        trigger: RetentionTrigger,
    },
    /// A non-executable proposal was created or resolved.
    Proposal {
        /// Opaque identifier of the proposal card.
        proposal_id: ProposalId,
        /// Эпизод-источник. `None` означает, что источника уже нет: карточку
        /// решают после удаления транскрипта. Поле существует ради удаления —
        /// `task_id` ambient-строки журнала выводится из него, поэтому
        /// `ambient.proposal` исчезает вместе с эпизодом по тому же индексу,
        /// что и `ambient.transcript`.
        episode_id: Option<EpisodeId>,
        /// Proposal category; proposals are non-executable suggestions.
        kind: ProposalKind,
        /// Bounded, opaque form of the canonical subject. The card's text has
        /// no field here at all: it is read back with a command, exactly as
        /// `memory.pending` withholds `statement`.
        subject_key: SubjectKey,
        /// Current lifecycle state of the proposal card.
        proposal_state: ProposalState,
    },
    /// Услышанная команда. Ни фразы, ни её обрывка здесь нет: `app_id` —
    /// это ключ каталога приложений, то есть выбор Core из заранее известного
    /// списка, а не то, что человек сказал. Заголовок приложения читается
    /// отдельной командой, ровно как текст карточки предложения.
    VoiceCommand {
        /// Opaque identifier of the pending voice command.
        command_id: CommandId,
        /// Closed-set action requested by the command.
        kind: VoiceCommandKind,
        /// Resolved application catalog key.
        app_id: AppId,
        /// Current decision or launch state.
        command_state: VoiceCommandState,
    },
    /// Ambient operation failed with a stable error code.
    Error {
        /// Stable error code safe to emit in an ambient log.
        code: AmbientErrorCode,
        /// Listening state at the time of the error.
        state: ListeningState,
    },
}

impl AmbientLogEvent {
    /// Returns the stable event name associated with this variant.
    pub const fn event_name(&self) -> &'static str {
        match self {
            AmbientLogEvent::State { .. } => "ambient.state",
            AmbientLogEvent::Engine { .. } => "ambient.engine",
            AmbientLogEvent::Transcript { .. } => "ambient.transcript",
            AmbientLogEvent::Retention { .. } => "ambient.retention",
            AmbientLogEvent::Proposal { .. } => "ambient.proposal",
            AmbientLogEvent::VoiceCommand { .. } => "ambient.voice_command",
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
            AmbientLogEvent::Proposal { .. } => &[
                "proposal_id",
                "episode_id",
                "kind",
                "subject_key",
                "proposal_state",
            ],
            AmbientLogEvent::VoiceCommand { .. } => {
                &["command_id", "kind", "app_id", "command_state"]
            }
            AmbientLogEvent::Error { .. } => &["code", "state"],
        }
    }

    /// Returns the severity assigned to this event category and state.
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
    /// Records one typed ambient event.
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
                episode_id: Some(EpisodeId::new("ep-1").unwrap()),
                kind: ProposalKind::Reminder,
                subject_key: SubjectKey::new("a1b2c3d4e5f60718").unwrap(),
                proposal_state: ProposalState::Proposed,
            },
            AmbientLogEvent::VoiceCommand {
                command_id: CommandId::new("cmd-1").unwrap(),
                kind: VoiceCommandKind::OpenApp,
                app_id: AppId::new("chrome").unwrap(),
                command_state: VoiceCommandState::Pending,
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
    fn extraction_states_round_trip_through_their_storage_form() {
        for state in ExtractionState::ALL {
            assert_eq!(ExtractionState::parse(state.as_str()), Some(state));
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                Value::String(state.as_str().to_owned())
            );
        }
        assert_eq!(ExtractionState::parse("extracted"), None);
    }

    #[test]
    fn proposal_kinds_and_states_round_trip_through_their_storage_form() {
        for kind in ProposalKind::ALL {
            assert_eq!(ProposalKind::parse(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                Value::String(kind.as_str().to_owned())
            );
        }
        assert_eq!(ProposalKind::parse("run_the_task"), None);
        for state in ProposalState::ALL {
            assert_eq!(ProposalState::parse(state.as_str()), Some(state));
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                Value::String(state.as_str().to_owned())
            );
        }
        assert_eq!(ProposalState::parse("pending"), None);
        assert!(!ProposalState::Proposed.is_terminal());
        for state in ProposalState::ALL
            .into_iter()
            .filter(|state| *state != ProposalState::Proposed)
        {
            assert!(state.is_terminal(), "{state:?} must not move again");
        }
    }

    #[test]
    fn voice_command_kinds_and_states_round_trip_through_their_wire_form() {
        for kind in VoiceCommandKind::ALL {
            assert_eq!(VoiceCommandKind::parse(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                Value::String(kind.as_str().to_owned())
            );
        }
        assert_eq!(VoiceCommandKind::parse("delete_everything"), None);
        for state in VoiceCommandState::ALL {
            assert_eq!(VoiceCommandState::parse(state.as_str()), Some(state));
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                Value::String(state.as_str().to_owned())
            );
        }
        assert_eq!(VoiceCommandState::parse("proposed"), None);
        assert!(!VoiceCommandState::Pending.is_terminal());
        for state in VoiceCommandState::ALL
            .into_iter()
            .filter(|state| *state != VoiceCommandState::Pending)
        {
            assert!(state.is_terminal(), "{state:?} must not move again");
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
                "ambient.voice_command",
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
