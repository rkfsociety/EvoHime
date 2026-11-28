//! Bounded contract for supervised external coding-agent executors.
//! Raw prompts, output, credentials and executable paths never belong to this
//! contract; the supervisor receives only an opaque validated run specification.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Stable contract identifier for external coding-agent adapters.
pub const CONTRACT_ID: &str = "evohime.external-agent/v1";
/// Current contract schema version.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum byte length for adapter, preset, and run identifiers.
pub const MAX_ID_BYTES: usize = 96;
/// Maximum serialized frame size in bytes.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// Maximum number of declared capabilities per adapter preset.
pub const MAX_CAPABILITIES: usize = 32;
/// Maximum number of declared credential slots per adapter preset.
pub const MAX_CREDENTIAL_SLOTS: usize = 16;
/// Supported ACP protocol version.
pub const ACP_PROTOCOL_VERSION: u32 = 1;
/// Supported A2A protocol version.
pub const A2A_PROTOCOL_VERSION: u32 = 1;
/// Protocol label identifying A2A-capable adapter presets.
pub const A2A_PROTOCOL_LABEL: &str = "a2a";

/// Wire protocol family supported by an external coding-agent adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExternalAgentProtocolKind {
    /// Native EvoHime v1 supervised protocol.
    EvoHimeV1,
    /// Agent Client Protocol (ACP) with explicit version.
    Acp {
        /// ACP protocol version number.
        protocol_version: u32,
    },
    /// Agent-to-Agent (A2A) protocol with explicit version.
    A2a {
        /// A2A protocol version number.
        protocol_version: u32,
    },
}

/// Authentication mode used by an external coding-agent adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalAgentAuthMode {
    /// Reuse an already-authenticated local CLI/agent session.
    ExistingLocalSession,
    /// Require explicitly declared Core credential slots.
    DeclaredCredentialSlots,
    /// No authentication required.
    Unauthenticated,
}

/// Classification of the external backend execution model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalAgentBackendClass {
    /// Full autonomous external coding-agent backend.
    ExternalAgentBackend,
    /// Model-like backend exposing structured turn execution.
    ModelLikeBackend,
}

/// Observed executable identity metadata for an external adapter binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutableIdentity {
    /// Logical executable reference (never a raw host secret path).
    pub executable_ref: String,
    /// Reported executable version string, if known.
    pub version: Option<String>,
    /// Observed content hash of the executable, if known.
    pub hash: Option<String>,
    /// Observation timestamp in Unix milliseconds.
    pub observed_at_ms: u64,
}

/// In-memory registry of configured adapter presets and active run states.
#[derive(Debug)]
pub struct ExternalAgentRegistry {
    /// Registered adapter presets keyed by preset identifier.
    pub presets: BTreeMap<String, AdapterPreset>,
    /// Active or recent run states keyed by run identifier.
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
    /// Returns a redacted status projection for the external agent registry.
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({"contract_id": CONTRACT_ID, "contract_version": CONTRACT_VERSION, "preset_count": self.presets.len(), "active_runs": self.runs.len(), "core_control_level": "supervised_opaque", "raw_payload": false, "credentials": "declared_slots_only"})
    }
}

/// Lifecycle state of an external coding-agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Adapter preset is registered.
    Registered,
    /// Process launch has been initiated.
    Starting,
    /// Protocol handshake is in progress.
    Handshaking,
    /// External agent is actively running.
    Running,
    /// Cancellation has been requested.
    Cancelling,
    /// Run completed successfully.
    Completed,
    /// Run failed.
    Failed,
    /// Outcome could not be determined safely.
    Unknown,
    /// Adapter or backend is unavailable.
    Unavailable,
}

/// Supervision and mediation level enforced over an external agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlLevel {
    /// Full Core-mediated control.
    Full,
    /// Supervised opaque execution with bounded metadata.
    SupervisedOpaque,
    /// Adapter is unavailable.
    Unavailable,
}

/// Named credential slot declared by an adapter preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialSlot {
    /// Stable slot identifier.
    pub id: String,
    /// Whether this credential slot must be supplied before launch.
    pub required: bool,
}

/// Validated configuration preset for an external coding-agent adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterPreset {
    /// Stable preset identifier.
    pub id: String,
    /// Monotonic preset revision.
    pub revision: u64,
    /// Protocol identifier string.
    pub protocol: String,
    /// Logical executable reference.
    pub executable_ref: String,
    /// Capabilities declared by the adapter.
    pub capabilities: Vec<String>,
    /// Credential slots required or supported by the adapter.
    pub credential_slots: Vec<CredentialSlot>,
    /// Supervision control level.
    pub control_level: ControlLevel,
    /// Whether this preset is enabled.
    pub enabled: bool,
    /// Typed protocol kind.
    #[serde(default = "default_protocol_kind")]
    pub protocol_kind: ExternalAgentProtocolKind,
    /// Authentication mode for the adapter.
    #[serde(default = "default_auth_mode")]
    pub auth_mode: ExternalAgentAuthMode,
    /// Backend execution class.
    #[serde(default = "default_backend_class")]
    pub backend_class: ExternalAgentBackendClass,
    /// Optional observed executable identity.
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

/// Immutable provenance snapshot captured for a validated adapter preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    /// Identifier of the source preset.
    pub preset_id: String,
    /// Revision of the source preset.
    pub preset_revision: u64,
    /// Protocol label.
    pub protocol: String,
    /// Canonical hash of sorted capabilities.
    pub capability_hash: String,
    /// Hash of the governing Core policy snapshot.
    pub policy_hash: String,
    /// Typed protocol kind.
    pub protocol_kind: ExternalAgentProtocolKind,
    /// Backend execution class.
    pub backend_class: ExternalAgentBackendClass,
    /// Hash of the observed executable identity, if present.
    pub executable_identity_hash: String,
}

/// Opaque, bounded run specification handed to the supervisor boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSpec {
    /// Stable run identifier.
    pub run_id: String,
    /// Conversation identifier associated with the run.
    pub conversation_id: String,
    /// Immutable preset and policy snapshot.
    pub snapshot: AgentSnapshot,
    /// Approved credential slot identifiers for this run.
    pub credential_slot_ids: Vec<String>,
    /// Maximum execution duration in milliseconds.
    pub timeout_ms: u64,
}

/// Bounded wire frame exchanged over the EvoHime external-agent protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExternalAgentFrame {
    /// Initial handshake message from an external agent.
    Hello {
        /// Protocol identifier.
        protocol: String,
        /// Agent identifier.
        agent_id: String,
        /// Advertised capabilities.
        capabilities: Vec<String>,
    },
    /// Handshake acknowledgement from Core.
    HelloAck {
        /// Protocol identifier.
        protocol: String,
        /// Capabilities accepted by Core.
        accepted_capabilities: Vec<String>,
    },
    /// Request to start a run.
    Run {
        /// Run identifier.
        run_id: String,
    },
    /// Progress event emitted during a run.
    Event {
        /// Run identifier.
        run_id: String,
        /// Event kind label.
        kind: String,
    },
    /// Terminal result notification for a run.
    Result {
        /// Run identifier.
        run_id: String,
        /// Outcome summary label.
        outcome: String,
    },
    /// Request to cancel an active run.
    Cancel {
        /// Run identifier.
        run_id: String,
    },
}

/// Immutable, hashed snapshot of negotiated ACP capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpCapabilitySnapshot {
    /// Negotiated ACP protocol version.
    pub protocol_version: u32,
    /// Reported agent identity.
    pub agent_identity: String,
    /// Reported agent version, if any.
    pub agent_version: Option<String>,
    /// Whether streaming output is supported.
    pub supports_streaming: bool,
    /// Whether cooperative cancellation is supported.
    pub supports_cancel: bool,
    /// Whether session resumption is supported.
    pub supports_resume: bool,
    /// Whether model selection is supported.
    pub supports_model_selection: bool,
    /// Whether image inputs are supported.
    pub supports_images: bool,
    /// Whether tool-activity events are emitted.
    pub supports_tool_activity: bool,
    /// Whether file-activity events are emitted.
    pub supports_file_activity: bool,
    /// Whether terminal-activity events are emitted.
    pub supports_terminal_activity: bool,
    /// Sorted list of supported session modes.
    pub session_modes: Vec<String>,
    /// Negotiation timestamp in Unix milliseconds.
    pub negotiated_at_ms: u64,
    /// SHA-256 digest of the canonical capability fields.
    pub content_hash: String,
}

/// Authentication readiness state for an ACP session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpAuthState {
    /// Authentication state has not been determined.
    AuthUnknown,
    /// Session is authenticated and ready.
    Ready,
    /// Interactive login is required.
    LoginRequired,
    /// Credentials or session expired.
    Expired,
    /// Authentication was rejected.
    Rejected,
    /// Authentication mechanism is unavailable.
    Unavailable,
}

/// Lifecycle state machine for an ACP session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpSessionState {
    /// Process is being spawned.
    Spawn,
    /// Initialization handshake is running.
    Initialize,
    /// Capabilities are being negotiated.
    Negotiating,
    /// Session is ready for work.
    Ready,
    /// Session is actively executing.
    Running,
    /// Session completed cleanly.
    Completed,
    /// Session was cancelled.
    Cancelled,
    /// Session failed.
    Failed,
    /// Session was interrupted.
    Interrupted,
    /// Session requires human review.
    NeedsReview,
}

/// Tool mediation mode for an ACP session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpToolControl {
    /// Tools run inside the external agent without Core mediation.
    Opaque,
    /// Tools are observed via activity events.
    Observed,
    /// Tools are mediated by Core permissions.
    CoreMediated,
}

/// Bounded JSON-RPC 2.0 frame exchanged with an ACP agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpJsonRpcFrame {
    /// JSON-RPC version string (must be `"2.0"`).
    pub jsonrpc: String,
    /// Optional request/response identifier.
    pub id: Option<String>,
    /// Optional RPC method name.
    pub method: Option<String>,
    /// Optional RPC parameters payload.
    pub params: Option<serde_json::Value>,
    /// Optional RPC result payload.
    pub result: Option<serde_json::Value>,
    /// Optional RPC error payload.
    pub error: Option<serde_json::Value>,
}

/// Maximum serialized ACP JSON-RPC frame size in bytes.
pub const MAX_ACP_FRAME_BYTES: usize = 64 * 1024;
/// Maximum number of session modes permitted in an ACP capability snapshot.
pub const MAX_ACP_SESSION_MODES: usize = 16;

/// Validates a raw ACP JSON-RPC frame against size and schema bounds.
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

/// Builds and hashes an immutable [`AcpCapabilitySnapshot`].
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

/// Validates and advances an [`AcpSessionState`] transition.
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

/// Validation or protocol failure returned by external-agent adapter contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    /// Generic validation error with a static reason code.
    Invalid(&'static str),
    /// Protocol or handshake fields are unsupported.
    UnsupportedProtocol,
    /// Requested capability is denied.
    CapabilityDenied,
    /// Requested credential slot is not permitted by the preset.
    CredentialSlotDenied,
    /// Frame exceeds the maximum byte limit.
    FrameTooLarge,
    /// Transport outcome could not be determined.
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

/// Validates and canonicalizes an [`AdapterPreset`].
pub fn validate_preset(mut preset: AdapterPreset) -> Result<AdapterPreset, AdapterError> {
    let protocol_valid = match preset.protocol_kind {
        ExternalAgentProtocolKind::EvoHimeV1 => preset.protocol == CONTRACT_ID,
        ExternalAgentProtocolKind::Acp { .. } => preset.protocol == "acp",
        ExternalAgentProtocolKind::A2a { .. } => preset.protocol == A2A_PROTOCOL_LABEL,
    };
    if !valid_id(&preset.id)
        || !protocol_valid
        || preset.executable_ref.is_empty()
        || preset.executable_ref.len() > MAX_ID_BYTES
    {
        return Err(AdapterError::Invalid("preset"));
    }
    if matches!(preset.protocol_kind, ExternalAgentProtocolKind::Acp { protocol_version } if protocol_version != ACP_PROTOCOL_VERSION)
        || matches!(preset.protocol_kind, ExternalAgentProtocolKind::A2a { protocol_version } if protocol_version != A2A_PROTOCOL_VERSION)
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

/// Validates an [`ExternalAgentFrame`] against size and handshake bounds.
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

/// Computes a canonical SHA-256 hash of sorted, deduplicated capabilities.
pub fn capability_hash(capabilities: &[String]) -> Result<String, AdapterError> {
    let mut values = capabilities.to_vec();
    values.sort();
    values.dedup();
    serde_json::to_vec(&values)
        .map(|v| hex::encode(Sha256::digest(v)))
        .map_err(|_| AdapterError::Invalid("hash"))
}

/// Validates a preset and builds an immutable [`AgentSnapshot`].
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

/// Validates a [`RunSpec`] against its governing [`AdapterPreset`].
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

    #[test]
    fn a2a_preset_is_typed_and_rejects_wrong_protocol_version() {
        let mut p = preset();
        p.protocol = A2A_PROTOCOL_LABEL.into();
        p.protocol_kind = ExternalAgentProtocolKind::A2a {
            protocol_version: A2A_PROTOCOL_VERSION,
        };
        p.auth_mode = ExternalAgentAuthMode::ExistingLocalSession;
        p.credential_slots.clear();
        assert!(validate_preset(p.clone()).is_ok());
        p.protocol_kind = ExternalAgentProtocolKind::A2a {
            protocol_version: A2A_PROTOCOL_VERSION + 1,
        };
        assert_eq!(
            validate_preset(p),
            Err(AdapterError::Invalid("protocol_or_auth_mode"))
        );
    }
}
