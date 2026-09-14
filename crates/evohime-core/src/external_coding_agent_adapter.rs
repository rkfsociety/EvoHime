//! Bounded contract for supervised external coding-agent executors.
//! Raw prompts, output, credentials and executable paths never belong to this
//! contract; the supervisor receives only an opaque validated run specification.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CONTRACT_ID: &str = "evohime.external-agent/v1";
pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 96;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_CREDENTIAL_SLOTS: usize = 16;
pub const ACP_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExternalAgentProtocolKind {
    EvoHimeV1,
    Acp { protocol_version: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalAgentAuthMode {
    ExistingLocalSession,
    DeclaredCredentialSlots,
    Unauthenticated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalAgentBackendClass {
    ExternalAgentBackend,
    ModelLikeBackend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutableIdentity {
    pub executable_ref: String,
    pub version: Option<String>,
    pub hash: Option<String>,
    pub observed_at_ms: u64,
}

#[derive(Debug)]
pub struct ExternalAgentRegistry {
    pub presets: BTreeMap<String, AdapterPreset>,
    pub runs: BTreeMap<String, AgentState>,
}
impl Default for ExternalAgentRegistry {
    fn default() -> Self {
        let preset = AdapterPreset {
            id: "codex.local".into(),
            revision: 1,
            protocol: CONTRACT_ID.into(),
            executable_ref: "codex.local".into(),
            capabilities: vec!["agent.execute".into()],
            credential_slots: vec![CredentialSlot {
                id: "provider".into(),
                required: true,
            }],
            control_level: ControlLevel::SupervisedOpaque,
            enabled: true,
            protocol_kind: ExternalAgentProtocolKind::EvoHimeV1,
            auth_mode: ExternalAgentAuthMode::DeclaredCredentialSlots,
            backend_class: ExternalAgentBackendClass::ExternalAgentBackend,
            executable_identity: None,
        };
        Self {
            presets: BTreeMap::from([(preset.id.clone(), preset)]),
            runs: BTreeMap::new(),
        }
    }
}
impl ExternalAgentRegistry {
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({"contract_id": CONTRACT_ID, "contract_version": CONTRACT_VERSION, "preset_count": self.presets.len(), "active_runs": self.runs.len(), "core_control_level": "supervised_opaque", "raw_payload": false, "credentials": "declared_slots_only"})
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Registered,
    Starting,
    Handshaking,
    Running,
    Cancelling,
    Completed,
    Failed,
    Unknown,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlLevel {
    Full,
    SupervisedOpaque,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialSlot {
    pub id: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterPreset {
    pub id: String,
    pub revision: u64,
    pub protocol: String,
    pub executable_ref: String,
    pub capabilities: Vec<String>,
    pub credential_slots: Vec<CredentialSlot>,
    pub control_level: ControlLevel,
    pub enabled: bool,
    #[serde(default = "default_protocol_kind")]
    pub protocol_kind: ExternalAgentProtocolKind,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: ExternalAgentAuthMode,
    #[serde(default = "default_backend_class")]
    pub backend_class: ExternalAgentBackendClass,
    #[serde(default)]
    pub executable_identity: Option<ExecutableIdentity>,
}

fn default_protocol_kind() -> ExternalAgentProtocolKind {
    ExternalAgentProtocolKind::EvoHimeV1
}
fn default_auth_mode() -> ExternalAgentAuthMode {
    ExternalAgentAuthMode::DeclaredCredentialSlots
}
fn default_backend_class() -> ExternalAgentBackendClass {
    ExternalAgentBackendClass::ExternalAgentBackend
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub preset_id: String,
    pub preset_revision: u64,
    pub protocol: String,
    pub capability_hash: String,
    pub policy_hash: String,
    pub protocol_kind: ExternalAgentProtocolKind,
    pub backend_class: ExternalAgentBackendClass,
    pub executable_identity_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSpec {
    pub run_id: String,
    pub conversation_id: String,
    pub snapshot: AgentSnapshot,
    pub credential_slot_ids: Vec<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExternalAgentFrame {
    Hello {
        protocol: String,
        agent_id: String,
        capabilities: Vec<String>,
    },
    HelloAck {
        protocol: String,
        accepted_capabilities: Vec<String>,
    },
    Run {
        run_id: String,
    },
    Event {
        run_id: String,
        kind: String,
    },
    Result {
        run_id: String,
        outcome: String,
    },
    Cancel {
        run_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpCapabilitySnapshot {
    pub protocol_version: u32,
    pub agent_identity: String,
    pub agent_version: Option<String>,
    pub supports_streaming: bool,
    pub supports_cancel: bool,
    pub supports_resume: bool,
    pub supports_model_selection: bool,
    pub supports_images: bool,
    pub supports_tool_activity: bool,
    pub supports_file_activity: bool,
    pub supports_terminal_activity: bool,
    pub session_modes: Vec<String>,
    pub negotiated_at_ms: u64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpAuthState {
    AuthUnknown,
    Ready,
    LoginRequired,
    Expired,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpSessionState {
    Spawn,
    Initialize,
    Negotiating,
    Ready,
    Running,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
    NeedsReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpToolControl {
    Opaque,
    Observed,
    CoreMediated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpJsonRpcFrame {
    pub jsonrpc: String,
    pub id: Option<String>,
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

pub const MAX_ACP_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_ACP_SESSION_MODES: usize = 16;

pub fn validate_acp_frame(bytes: &[u8]) -> Result<AcpJsonRpcFrame, AdapterError> {
    if bytes.len() > MAX_ACP_FRAME_BYTES {
        return Err(AdapterError::FrameTooLarge);
    }
    let frame: AcpJsonRpcFrame =
        serde_json::from_slice(bytes).map_err(|_| AdapterError::Invalid("malformed_acp_frame"))?;
    if frame.jsonrpc != "2.0" || frame.id.as_deref().is_some_and(|id| !valid_id(id)) {
        return Err(AdapterError::Invalid("invalid_json_rpc"));
    }
    if frame.method.is_none() && frame.result.is_none() && frame.error.is_none() {
        return Err(AdapterError::Invalid("empty_json_rpc"));
    }
    Ok(frame)
}

pub fn capability_snapshot(
    protocol_version: u32,
    agent_identity: String,
    agent_version: Option<String>,
    capabilities: &[&str],
    negotiated_at_ms: u64,
) -> Result<AcpCapabilitySnapshot, AdapterError> {
    if protocol_version != ACP_PROTOCOL_VERSION
        || !valid_id(&agent_identity)
        || capabilities.len() > MAX_CAPABILITIES
    {
        return Err(AdapterError::Invalid("capability_snapshot"));
    }
    let has = |name: &str| capabilities.contains(&name);
    let mut modes = capabilities
        .iter()
        .filter_map(|value| value.strip_prefix("session_mode:"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    modes.sort();
    modes.dedup();
    if modes.len() > MAX_ACP_SESSION_MODES || modes.iter().any(|mode| !valid_id(mode)) {
        return Err(AdapterError::Invalid("session_modes"));
    }
    let snapshot = AcpCapabilitySnapshot {
        protocol_version,
        agent_identity,
        agent_version,
        supports_streaming: has("streaming"),
        supports_cancel: has("cancel"),
        supports_resume: has("resume"),
        supports_model_selection: has("model_selection"),
        supports_images: has("images"),
        supports_tool_activity: has("tool_activity"),
        supports_file_activity: has("file_activity"),
        supports_terminal_activity: has("terminal_activity"),
        session_modes: modes,
        negotiated_at_ms,
        content_hash: String::new(),
    };
    let mut hashed = snapshot.clone();
    hashed.content_hash.clear();
    let content_hash = hex::encode(Sha256::digest(
        serde_json::to_vec(&hashed).map_err(|_| AdapterError::Invalid("hash"))?,
    ));
    Ok(AcpCapabilitySnapshot {
        content_hash,
        ..snapshot
    })
}

pub fn transition_acp(
    current: AcpSessionState,
    next: AcpSessionState,
) -> Result<AcpSessionState, AdapterError> {
    let allowed = matches!(
        (current, next),
        (AcpSessionState::Spawn, AcpSessionState::Initialize)
            | (AcpSessionState::Initialize, AcpSessionState::Negotiating)
            | (AcpSessionState::Negotiating, AcpSessionState::Ready)
            | (AcpSessionState::Ready, AcpSessionState::Running)
            | (
                AcpSessionState::Running,
                AcpSessionState::Completed | AcpSessionState::Cancelled | AcpSessionState::Failed
            )
            | (
                AcpSessionState::Spawn
                    | AcpSessionState::Initialize
                    | AcpSessionState::Negotiating
                    | AcpSessionState::Ready
                    | AcpSessionState::Running,
                AcpSessionState::Interrupted | AcpSessionState::NeedsReview
            )
    );
    if allowed {
        Ok(next)
    } else {
        Err(AdapterError::Invalid("invalid_acp_transition"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    Invalid(&'static str),
    UnsupportedProtocol,
    CapabilityDenied,
    CredentialSlotDenied,
    FrameTooLarge,
    UnknownOutcome,
}
impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Invalid(v) => v,
                Self::UnsupportedProtocol => "unsupported_protocol",
                Self::CapabilityDenied => "capability_denied",
                Self::CredentialSlotDenied => "credential_slot_denied",
                Self::FrameTooLarge => "frame_too_large",
                Self::UnknownOutcome => "unknown_outcome",
            }
        )
    }
}
impl std::error::Error for AdapterError {}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

pub fn validate_preset(mut preset: AdapterPreset) -> Result<AdapterPreset, AdapterError> {
    let protocol_valid = match preset.protocol_kind {
        ExternalAgentProtocolKind::EvoHimeV1 => preset.protocol == CONTRACT_ID,
        ExternalAgentProtocolKind::Acp { .. } => preset.protocol == "acp",
    };
    if !valid_id(&preset.id)
        || !protocol_valid
        || preset.executable_ref.is_empty()
        || preset.executable_ref.len() > MAX_ID_BYTES
    {
        return Err(AdapterError::Invalid("preset"));
    }
    if matches!(preset.protocol_kind, ExternalAgentProtocolKind::Acp { protocol_version } if protocol_version != ACP_PROTOCOL_VERSION)
        || matches!(
            preset.auth_mode,
            ExternalAgentAuthMode::ExistingLocalSession
        ) && !preset.credential_slots.is_empty()
    {
        return Err(AdapterError::Invalid("protocol_or_auth_mode"));
    }
    if preset.capabilities.len() > MAX_CAPABILITIES
        || preset.credential_slots.len() > MAX_CREDENTIAL_SLOTS
    {
        return Err(AdapterError::Invalid("bounds"));
    }
    preset.capabilities.sort();
    preset.capabilities.dedup();
    if preset.capabilities.iter().any(|v| !valid_id(v)) {
        return Err(AdapterError::Invalid("capability"));
    }
    preset.credential_slots.sort_by(|a, b| a.id.cmp(&b.id));
    if preset.credential_slots.iter().any(|s| !valid_id(&s.id))
        || preset
            .credential_slots
            .windows(2)
            .any(|w| w[0].id == w[1].id)
    {
        return Err(AdapterError::Invalid("credential_slot"));
    }
    Ok(preset)
}

pub fn validate_frame(frame: &ExternalAgentFrame) -> Result<(), AdapterError> {
    let bytes = serde_json::to_vec(frame).map_err(|_| AdapterError::Invalid("serialization"))?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(AdapterError::FrameTooLarge);
    }
    match frame {
        ExternalAgentFrame::Hello {
            protocol,
            agent_id,
            capabilities,
        } if protocol != CONTRACT_ID
            || !valid_id(agent_id)
            || capabilities.len() > MAX_CAPABILITIES =>
        {
            Err(AdapterError::UnsupportedProtocol)
        }
        _ => Ok(()),
    }
}

pub fn capability_hash(capabilities: &[String]) -> Result<String, AdapterError> {
    let mut values = capabilities.to_vec();
    values.sort();
    values.dedup();
    serde_json::to_vec(&values)
        .map(|v| hex::encode(Sha256::digest(v)))
        .map_err(|_| AdapterError::Invalid("hash"))
}

pub fn snapshot(
    preset: &AdapterPreset,
    policy_hash: impl Into<String>,
) -> Result<AgentSnapshot, AdapterError> {
    let preset = validate_preset(preset.clone())?;
    Ok(AgentSnapshot {
        preset_id: preset.id,
        preset_revision: preset.revision,
        protocol: preset.protocol,
        capability_hash: capability_hash(&preset.capabilities)?,
        policy_hash: policy_hash.into(),
        protocol_kind: preset.protocol_kind,
        backend_class: preset.backend_class,
        executable_identity_hash: preset
            .executable_identity
            .as_ref()
            .map(|identity| {
                hex::encode(Sha256::digest(
                    serde_json::to_vec(identity).unwrap_or_default(),
                ))
            })
            .unwrap_or_default(),
    })
}

pub fn validate_run_spec(spec: &RunSpec, preset: &AdapterPreset) -> Result<(), AdapterError> {
    if !valid_id(&spec.run_id)
        || !valid_id(&spec.conversation_id)
        || spec.timeout_ms == 0
        || spec.timeout_ms > 3_600_000
    {
        return Err(AdapterError::Invalid("run_spec"));
    }
    if spec.snapshot != snapshot(preset, spec.snapshot.policy_hash.clone())? {
        return Err(AdapterError::Invalid("snapshot"));
    }
    if spec
        .credential_slot_ids
        .iter()
        .any(|id| !preset.credential_slots.iter().any(|slot| slot.id == *id))
    {
        return Err(AdapterError::CredentialSlotDenied);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn preset() -> AdapterPreset {
        AdapterPreset {
            id: "codex.local".into(),
            revision: 1,
            protocol: CONTRACT_ID.into(),
            executable_ref: "codex".into(),
            capabilities: vec!["agent.execute".into()],
            credential_slots: vec![CredentialSlot {
                id: "provider".into(),
                required: true,
            }],
            control_level: ControlLevel::SupervisedOpaque,
            enabled: true,
            protocol_kind: ExternalAgentProtocolKind::EvoHimeV1,
            auth_mode: ExternalAgentAuthMode::DeclaredCredentialSlots,
            backend_class: ExternalAgentBackendClass::ExternalAgentBackend,
            executable_identity: None,
        }
    }
    #[test]
    fn validates_protocol_and_snapshot() {
        let p = validate_preset(preset()).unwrap();
        let s = snapshot(&p, "policy").unwrap();
        assert_eq!(s.preset_id, "codex.local");
    }
    #[test]
    fn rejects_unknown_credential_slot() {
        let p = preset();
        let spec = RunSpec {
            run_id: "r".into(),
            conversation_id: "c".into(),
            snapshot: snapshot(&p, "p").unwrap(),
            credential_slot_ids: vec!["missing".into()],
            timeout_ms: 1000,
        };
        assert_eq!(
            validate_run_spec(&spec, &p),
            Err(AdapterError::CredentialSlotDenied)
        );
    }
    #[test]
    fn rejects_oversized_frame() {
        let frame = ExternalAgentFrame::Event {
            run_id: "r".into(),
            kind: "x".repeat(MAX_FRAME_BYTES),
        };
        assert_eq!(validate_frame(&frame), Err(AdapterError::FrameTooLarge));
    }

    #[test]
    fn acp_capabilities_are_immutable_and_hashed() {
        let snapshot = capability_snapshot(
            1,
            "agent.local".into(),
            Some("1".into()),
            &["streaming", "cancel", "session_mode:isolated"],
            10,
        )
        .unwrap();
        assert!(!snapshot.content_hash.is_empty());
        assert_eq!(snapshot.session_modes, vec!["isolated"]);
    }

    #[test]
    fn malformed_or_oversized_acp_frames_fail_closed() {
        assert_eq!(
            validate_acp_frame(br#"{}"#),
            Err(AdapterError::Invalid("malformed_acp_frame"))
        );
        assert_eq!(
            validate_acp_frame(&vec![b'x'; MAX_ACP_FRAME_BYTES + 1]),
            Err(AdapterError::FrameTooLarge)
        );
    }

    #[test]
    fn acp_state_machine_rejects_skipped_ready_state() {
        assert_eq!(
            transition_acp(AcpSessionState::Spawn, AcpSessionState::Running),
            Err(AdapterError::Invalid("invalid_acp_transition"))
        );
        assert_eq!(
            transition_acp(AcpSessionState::Spawn, AcpSessionState::Initialize),
            Ok(AcpSessionState::Initialize)
        );
    }

    #[test]
    fn acp_preset_is_typed_and_legacy_preset_stays_valid() {
        let mut p = preset();
        p.protocol = "acp".into();
        p.protocol_kind = ExternalAgentProtocolKind::Acp {
            protocol_version: 1,
        };
        p.auth_mode = ExternalAgentAuthMode::ExistingLocalSession;
        p.credential_slots.clear();
        assert!(validate_preset(p).is_ok());
        let legacy: AdapterPreset = serde_json::from_value(serde_json::json!({
            "id":"legacy", "revision":1, "protocol":CONTRACT_ID,
            "executable_ref":"legacy", "capabilities":[], "credential_slots":[],
            "control_level":"supervised_opaque", "enabled":true
        }))
        .unwrap();
        assert_eq!(legacy.protocol_kind, ExternalAgentProtocolKind::EvoHimeV1);
    }
}
