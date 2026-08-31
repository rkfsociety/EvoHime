pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/evohime.desktop.v1.rs"));
}
pub mod session;
pub mod transport;
#[cfg(windows)]
pub mod windows_security;

pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_RESYNC_MAX_EVENTS: u32 = 512;
pub const MAX_RESYNC_SNAPSHOT_BYTES: usize = MAX_FRAME_BYTES - 1024;
pub const MAX_CAPABILITIES: usize = 64;
pub const MAX_CAPABILITY_NAME_BYTES: usize = 64;
pub const MAX_CORE_INFO_STRING_BYTES: usize = MAX_CAPABILITY_NAME_BYTES;
pub const MAX_REPLAY_EVENTS: usize = 512;
pub const MAX_REPLAY_EVENT_BYTES: usize = MAX_FRAME_BYTES - 1024;
pub const MAX_REPLAY_LOG_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveLimits {
    pub max_frame_bytes: usize,
    pub max_replay_events: usize,
    pub max_snapshot_bytes: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreInfoError {
    #[error("CoreInfo string is empty, oversized, or contains control characters")]
    InvalidString,
    #[error("CoreInfo capability or feature list is invalid")]
    InvalidCapabilities,
    #[error("CoreInfo limits are zero or exceed local hard limits")]
    InvalidLimits,
}

fn valid_bounded_string(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CORE_INFO_STRING_BYTES
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

pub fn validate_core_info(info: &generated::CoreInfo) -> Result<EffectiveLimits, CoreInfoError> {
    for value in [
        &info.core_version,
        &info.build_revision,
        &info.runtime_revision,
    ] {
        if !valid_bounded_string(value) {
            return Err(CoreInfoError::InvalidString);
        }
    }
    if info.capabilities.len() > MAX_CAPABILITIES
        || info.feature_flags.len() > MAX_CAPABILITIES
        || info
            .capabilities
            .iter()
            .chain(info.feature_flags.iter())
            .any(|value| !valid_bounded_string(value))
    {
        return Err(CoreInfoError::InvalidCapabilities);
    }
    let max_frame_bytes = info.max_frame_bytes as usize;
    let max_replay_events = info.max_replay_events as usize;
    let max_snapshot_bytes = info.max_snapshot_bytes as usize;
    if max_frame_bytes == 0
        || max_replay_events == 0
        || max_snapshot_bytes == 0
        || max_frame_bytes > MAX_FRAME_BYTES
        || max_replay_events > MAX_REPLAY_EVENTS
        || max_snapshot_bytes > MAX_RESYNC_SNAPSHOT_BYTES
    {
        return Err(CoreInfoError::InvalidLimits);
    }
    Ok(EffectiveLimits {
        max_frame_bytes,
        max_replay_events,
        max_snapshot_bytes,
    })
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResyncError {
    #[error("resync batch exceeds the {DEFAULT_RESYNC_MAX_EVENTS} event limit")]
    TooManyEvents,
    #[error("full snapshot exceeds the bounded IPC snapshot limit")]
    SnapshotTooLarge,
}

pub fn validate_resync_request(request: &generated::ResyncRequest) -> Result<(), ResyncError> {
    if request.max_events > DEFAULT_RESYNC_MAX_EVENTS {
        return Err(ResyncError::TooManyEvents);
    }
    Ok(())
}

pub fn validate_full_snapshot(snapshot: &generated::FullSnapshot) -> Result<(), ResyncError> {
    if snapshot.snapshot_json.len() > MAX_RESYNC_SNAPSHOT_BYTES {
        return Err(ResyncError::SnapshotTooLarge);
    }
    Ok(())
}

pub fn normalize_task_status(value: i32) -> generated::TaskStatus {
    generated::TaskStatus::try_from(value).unwrap_or(generated::TaskStatus::Unknown)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

impl ProtocolVersion {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    pub const fn is_compatible(self, peer: Self) -> bool {
        self.major == peer.major
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NegotiationError {
    #[error("protocol major versions are incompatible: local={local}, peer={peer}")]
    MajorMismatch { local: u32, peer: u32 },
    #[error("capability list exceeds the {MAX_CAPABILITIES} item limit")]
    TooManyCapabilities,
    #[error("capability name is empty or exceeds the {MAX_CAPABILITY_NAME_BYTES} byte limit")]
    InvalidCapability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegotiatedProtocol {
    pub version: ProtocolVersion,
    pub capabilities: Vec<String>,
}

pub fn negotiate_protocol(
    local: ProtocolVersion,
    peer: ProtocolVersion,
    local_capabilities: &[String],
    peer_capabilities: &[String],
) -> Result<NegotiatedProtocol, NegotiationError> {
    if !local.is_compatible(peer) {
        return Err(NegotiationError::MajorMismatch {
            local: local.major,
            peer: peer.major,
        });
    }

    let local_caps = normalize_capabilities(local_capabilities)?;
    let peer_caps = normalize_capabilities(peer_capabilities)?;
    let capabilities = local_caps
        .into_iter()
        .filter(|capability| peer_caps.binary_search(capability).is_ok())
        .collect();

    Ok(NegotiatedProtocol {
        version: ProtocolVersion::new(local.major, local.minor.min(peer.minor)),
        capabilities,
    })
}

fn normalize_capabilities(values: &[String]) -> Result<Vec<String>, NegotiationError> {
    if values.len() > MAX_CAPABILITIES {
        return Err(NegotiationError::TooManyCapabilities);
    }
    let mut normalized = values.to_vec();
    for capability in &normalized {
        if capability.is_empty()
            || capability.len() > MAX_CAPABILITY_NAME_BYTES
            || capability.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(NegotiationError::InvalidCapability);
        }
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEvent {
    pub sequence_id: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayResult {
    Events(Vec<ReplayEvent>),
    Gap {
        requested_after_sequence: u64,
        earliest_available_sequence: u64,
        latest_available_sequence: u64,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error("replay log capacity must be between 1 and {MAX_REPLAY_EVENTS} events")]
    InvalidCapacity,
    #[error("replay event payload exceeds the bounded IPC event limit")]
    EventTooLarge,
    #[error("replay log exceeds the bounded {MAX_REPLAY_LOG_BYTES} byte limit")]
    LogTooLarge,
    #[error("replay sequence must be strictly contiguous")]
    NonContiguous,
    #[error("replay request exceeds the {MAX_REPLAY_EVENTS} event limit")]
    RequestTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedReplayLog {
    max_events: usize,
    events: Vec<ReplayEvent>,
    bytes: usize,
}

impl BoundedReplayLog {
    pub fn new(max_events: usize) -> Result<Self, ReplayError> {
        if !(1..=MAX_REPLAY_EVENTS).contains(&max_events) {
            return Err(ReplayError::InvalidCapacity);
        }
        Ok(Self {
            max_events,
            events: Vec::new(),
            bytes: 0,
        })
    }

    pub fn from_events(
        max_events: usize,
        events: impl IntoIterator<Item = ReplayEvent>,
    ) -> Result<Self, ReplayError> {
        let mut log = Self::new(max_events)?;
        for event in events {
            log.append(event)?;
        }
        Ok(log)
    }

    pub fn append(&mut self, event: ReplayEvent) -> Result<(), ReplayError> {
        if event.payload.len() > MAX_REPLAY_EVENT_BYTES {
            return Err(ReplayError::EventTooLarge);
        }
        if let Some(previous) = self.events.last() {
            if event.sequence_id != previous.sequence_id.saturating_add(1) {
                return Err(ReplayError::NonContiguous);
            }
        }
        let event_bytes = event.payload.len();
        if self.bytes.saturating_add(event_bytes) > MAX_REPLAY_LOG_BYTES {
            return Err(ReplayError::LogTooLarge);
        }
        self.events.push(event);
        self.bytes += event_bytes;
        while self.events.len() > self.max_events {
            let removed = self.events.remove(0);
            self.bytes -= removed.payload.len();
        }
        Ok(())
    }

    pub fn replay_after(
        &self,
        after_sequence: u64,
        max_events: u32,
    ) -> Result<ReplayResult, ReplayError> {
        let requested = if max_events == 0 {
            self.max_events
        } else {
            max_events as usize
        };
        if requested > MAX_REPLAY_EVENTS {
            return Err(ReplayError::RequestTooLarge);
        }
        let Some(first) = self.events.first() else {
            return Ok(ReplayResult::Events(Vec::new()));
        };
        let latest = self.events.last().expect("first implies last");
        if after_sequence.saturating_add(1) < first.sequence_id {
            return Ok(ReplayResult::Gap {
                requested_after_sequence: after_sequence,
                earliest_available_sequence: first.sequence_id,
                latest_available_sequence: latest.sequence_id,
            });
        }
        Ok(ReplayResult::Events(
            self.events
                .iter()
                .filter(|event| event.sequence_id > after_sequence)
                .take(requested)
                .cloned()
                .collect(),
        ))
    }

    pub fn events(&self) -> &[ReplayEvent] {
        &self.events
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FrameError {
    #[error("frame is truncated")]
    Truncated,
    #[error("frame exceeds the {MAX_FRAME_BYTES} byte limit")]
    TooLarge,
    #[error("frame length prefix does not match payload")]
    LengthMismatch,
    #[error("IPC I/O error: {0}")]
    Io(String),
}

pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }

    let length = u32::try_from(payload.len()).map_err(|_| FrameError::TooLarge)?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

pub fn decode_frame(frame: &[u8]) -> Result<&[u8], FrameError> {
    if frame.len() < 4 {
        return Err(FrameError::Truncated);
    }

    let length = u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    if frame.len() < 4 + length {
        return Err(FrameError::Truncated);
    }
    if frame.len() != 4 + length {
        return Err(FrameError::LengthMismatch);
    }

    Ok(&frame[4..])
}

#[cfg(test)]
mod tests {
    use super::{
        decode_frame, encode_frame, negotiate_protocol, transport, validate_core_info,
        validate_full_snapshot, validate_resync_request, BoundedReplayLog, FrameError,
        NegotiationError, ProtocolVersion, ReplayError, ReplayEvent, ReplayResult, ResyncError,
        DEFAULT_RESYNC_MAX_EVENTS, MAX_FRAME_BYTES, MAX_RESYNC_SNAPSHOT_BYTES,
    };
    use crate::generated;
    use crate::normalize_task_status;

    #[test]
    fn accepts_additive_minor_versions() {
        assert!(ProtocolVersion::new(1, 0).is_compatible(ProtocolVersion::new(1, 1)));
        assert!(ProtocolVersion::new(1, 1).is_compatible(ProtocolVersion::new(1, 0)));
    }

    #[test]
    fn rejects_major_version_mismatch() {
        assert!(!ProtocolVersion::new(1, 0).is_compatible(ProtocolVersion::new(2, 0)));
    }

    #[test]
    fn negotiates_minor_version_and_capability_intersection() {
        let negotiated = negotiate_protocol(
            ProtocolVersion::new(1, 4),
            ProtocolVersion::new(1, 2),
            &["replay".into(), "resync".into(), "future".into()],
            &["resync".into(), "replay".into()],
        )
        .expect("same major versions negotiate");
        assert_eq!(negotiated.version, ProtocolVersion::new(1, 2));
        assert_eq!(negotiated.capabilities, vec!["replay", "resync"]);
    }

    #[test]
    fn negotiation_rejects_major_mismatch_and_unbounded_capabilities() {
        assert_eq!(
            negotiate_protocol(
                ProtocolVersion::new(1, 0),
                ProtocolVersion::new(2, 0),
                &[],
                &[],
            ),
            Err(NegotiationError::MajorMismatch { local: 1, peer: 2 })
        );
        let capabilities = vec!["cap".to_string(); super::MAX_CAPABILITIES + 1];
        assert_eq!(
            negotiate_protocol(
                ProtocolVersion::new(1, 0),
                ProtocolVersion::new(1, 0),
                &capabilities,
                &[],
            ),
            Err(NegotiationError::TooManyCapabilities)
        );
    }

    #[test]
    fn protocol_offer_fixture_round_trips_additive_capabilities() {
        use prost::Message;

        let offer = generated::ProtocolOffer {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 2 }),
            capabilities: vec!["replay".into(), "resync".into()],
        };
        let decoded = generated::ProtocolOffer::decode(offer.encode_to_vec().as_slice())
            .expect("protocol offer decodes");
        assert_eq!(decoded.capabilities, vec!["replay", "resync"]);
    }

    #[test]
    fn core_info_validates_and_clamps_to_declared_limits() {
        let info = generated::CoreInfo {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            core_version: "1.0.0".into(),
            build_revision: "unknown".into(),
            runtime_revision: "rust-core".into(),
            capabilities: vec!["replay".into(), "resync".into()],
            feature_flags: vec!["authenticated-ipc".into()],
            max_frame_bytes: (MAX_FRAME_BYTES / 2) as u32,
            max_replay_events: 128,
            max_snapshot_bytes: (MAX_RESYNC_SNAPSHOT_BYTES / 2) as u32,
        };
        let limits = validate_core_info(&info).expect("bounded CoreInfo");
        assert_eq!(limits.max_frame_bytes, MAX_FRAME_BYTES / 2);
        assert_eq!(limits.max_replay_events, 128);
    }

    #[test]
    fn legacy_ready_without_core_info_remains_valid() {
        use prost::Message;
        let ready = generated::Ready {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            core_version: "legacy".into(),
            core_info: None,
        };
        let decoded = generated::Ready::decode(ready.encode_to_vec().as_slice())
            .expect("legacy Ready decodes");
        assert!(decoded.core_info.is_none());
    }

    #[test]
    fn rejects_malformed_frame() {
        assert!(matches!(
            decode_frame(&[1, 2, 3]),
            Err(FrameError::Truncated)
        ));
    }

    #[test]
    fn round_trips_a_bounded_frame() {
        let frame = encode_frame(b"hello").expect("small frame");
        assert_eq!(decode_frame(&frame).expect("valid frame"), b"hello");
    }

    #[test]
    fn rejects_oversized_frame() {
        let payload = vec![0; MAX_FRAME_BYTES + 1];
        assert_eq!(encode_frame(&payload), Err(FrameError::TooLarge));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut frame = encode_frame(b"hello").expect("small frame");
        frame.push(0);
        assert_eq!(decode_frame(&frame), Err(FrameError::LengthMismatch));
    }

    #[tokio::test]
    async fn async_transport_round_trips_a_frame() {
        let (mut writer, mut reader) = tokio::io::duplex(64);
        let write = tokio::spawn(async move {
            transport::write_frame(&mut writer, b"hello")
                .await
                .expect("write frame");
        });

        let payload = transport::read_frame(&mut reader)
            .await
            .expect("read frame");
        write.await.expect("writer task");
        assert_eq!(payload, b"hello");
    }

    #[test]
    fn legacy_command_envelope_decodes_after_additive_fields() {
        use prost::Message;

        let legacy = generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: "legacy-request".into(),
            client_id: String::new(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 7 },
            )),
        };
        let decoded = generated::CommandEnvelope::decode(legacy.encode_to_vec().as_slice())
            .expect("legacy envelope decodes");
        assert_eq!(decoded.request_id, "legacy-request");
        assert_eq!(decoded.session_epoch, 0);
        assert!(matches!(
            decoded.command,
            Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 7 }
            ))
        ));
    }

    #[test]
    fn unknown_additive_field_is_ignored() {
        use prost::Message;

        let mut encoded = generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: "request".into(),
            client_id: String::new(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: None,
        }
        .encode_to_vec();
        // Field 9999, varint wire type — chosen far above any field number
        // currently defined in the schema so this stays a genuinely unknown field.
        encoded.extend_from_slice(&[0xf8, 0xf0, 0x04, 0x01]);
        let decoded = generated::CommandEnvelope::decode(encoded.as_slice())
            .expect("unknown field is ignored");
        assert_eq!(decoded.request_id, "request");
    }

    #[test]
    fn unknown_task_status_maps_to_unknown() {
        use prost::Message;

        let task = generated::CreateTask {
            task_id: "task".into(),
            project_id: "project".into(),
            parent_id: String::new(),
            title: "Task".into(),
            description: String::new(),
            source_ref: String::new(),
            acceptance_criteria: String::new(),
            non_goals: String::new(),
            status: String::new(),
            priority: 0,
            estimate: 0,
            complexity: String::new(),
            status_code: 99,
        };
        let decoded =
            generated::CreateTask::decode(task.encode_to_vec().as_slice()).expect("task decodes");
        assert_eq!(decoded.status_code, 99);
        assert_eq!(
            normalize_task_status(decoded.status_code),
            generated::TaskStatus::Unknown
        );
    }

    #[test]
    fn bounded_resync_rejects_unbounded_batch() {
        let request = generated::ResyncRequest {
            after_sequence: 1,
            max_events: DEFAULT_RESYNC_MAX_EVENTS + 1,
            include_full_snapshot: false,
        };
        assert_eq!(
            validate_resync_request(&request),
            Err(ResyncError::TooManyEvents)
        );
    }

    #[test]
    fn full_snapshot_validation_is_bounded() {
        let snapshot = generated::FullSnapshot {
            sequence_id: 1,
            snapshot_json: vec![0; super::MAX_RESYNC_SNAPSHOT_BYTES + 1],
        };
        assert_eq!(
            validate_full_snapshot(&snapshot),
            Err(ResyncError::SnapshotTooLarge)
        );
    }

    #[test]
    fn bounded_replay_returns_gap_then_replays_contiguous_events() {
        let mut log = BoundedReplayLog::new(2).expect("bounded log");
        log.append(ReplayEvent {
            sequence_id: 1,
            payload: b"one".to_vec(),
        })
        .expect("first event");
        log.append(ReplayEvent {
            sequence_id: 2,
            payload: b"two".to_vec(),
        })
        .expect("second event");
        log.append(ReplayEvent {
            sequence_id: 3,
            payload: b"three".to_vec(),
        })
        .expect("third event evicts oldest");

        assert!(matches!(
            log.replay_after(0, 0),
            Ok(ReplayResult::Gap {
                earliest_available_sequence: 2,
                latest_available_sequence: 3,
                ..
            })
        ));
        assert_eq!(
            log.replay_after(1, super::MAX_REPLAY_EVENTS as u32 + 1),
            Err(ReplayError::RequestTooLarge)
        );
        assert_eq!(
            log.replay_after(1, 2).expect("replay after gap boundary"),
            ReplayResult::Events(vec![
                ReplayEvent {
                    sequence_id: 2,
                    payload: b"two".to_vec()
                },
                ReplayEvent {
                    sequence_id: 3,
                    payload: b"three".to_vec()
                }
            ])
        );
    }

    #[test]
    fn replay_export_import_preserves_sequence_and_rejects_noncontiguous_events() {
        let events = vec![
            ReplayEvent {
                sequence_id: 4,
                payload: b"four".to_vec(),
            },
            ReplayEvent {
                sequence_id: 5,
                payload: b"five".to_vec(),
            },
        ];
        let log = BoundedReplayLog::from_events(4, events.clone()).expect("import events");
        assert_eq!(log.events(), events.as_slice());
        assert_eq!(
            BoundedReplayLog::from_events(
                4,
                vec![
                    ReplayEvent {
                        sequence_id: 1,
                        payload: vec![]
                    },
                    ReplayEvent {
                        sequence_id: 3,
                        payload: vec![]
                    }
                ]
            ),
            Err(ReplayError::NonContiguous)
        );
    }

    #[test]
    fn sensitive_data_guardrails_additive_messages_round_trip() {
        use prost::Message;
        let command = generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: "guardrail-request".into(),
            client_id: "shell".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(
                generated::command_envelope::Command::SensitiveDataGuardrails(
                    generated::SensitiveDataGuardrailsCommand {
                        schema_version: 1,
                        request_id: "guardrail-request".into(),
                        owner_scope: "test".into(),
                        operation: "status".into(),
                        payload: Vec::new(),
                        idempotency_key: "once".into(),
                    },
                ),
            ),
        };
        let decoded = generated::CommandEnvelope::decode(command.encode_to_vec().as_slice())
            .expect("guardrail command decodes");
        assert!(matches!(
            decoded.command,
            Some(generated::command_envelope::Command::SensitiveDataGuardrails(_))
        ));
    }
}
