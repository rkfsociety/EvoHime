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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub preset_id: String,
    pub preset_revision: u64,
    pub protocol: String,
    pub capability_hash: String,
    pub policy_hash: String,
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
    if !valid_id(&preset.id)
        || preset.protocol != CONTRACT_ID
        || preset.executable_ref.is_empty()
        || preset.executable_ref.len() > MAX_ID_BYTES
    {
        return Err(AdapterError::Invalid("preset"));
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
}
