//! Core-owned, bounded middleware pipeline around agent/model/tool phases.
//!
//! Middleware is a typed policy description, not executable imported code. It
//! may observe, narrow or block an already-authorized operation; the caller's
//! capability set is never expanded here.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const CONTRACT_VERSION: u32 = 1;
pub const CONTRACT_ID: &str = "agent-middleware-pipeline-v1";
pub const MAX_MIDDLEWARE: usize = 32;
pub const MAX_PHASES: usize = 8;
pub const MAX_ID_CHARS: usize = 128;
pub const MAX_TEXT_CHARS: usize = 512;
pub const MAX_EVENTS: usize = 256;
pub const MAX_EVENT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    BeforeAgent,
    AfterAgent,
    BeforeModel,
    AfterModel,
    WrapModelCall,
    BeforeTool,
    WrapToolCall,
    AfterTool,
}

impl HookPhase {
    pub const ALL: [Self; 8] = [
        Self::BeforeAgent,
        Self::AfterAgent,
        Self::BeforeModel,
        Self::AfterModel,
        Self::WrapModelCall,
        Self::BeforeTool,
        Self::WrapToolCall,
        Self::AfterTool,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateClass {
    Private,
    Checkpoint,
    Public,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum BuiltinPolicy {
    Observe,
    Narrow { max_bytes: u32 },
    Redact { fields: Vec<String> },
    Block { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MiddlewareSpec {
    pub id: String,
    pub version: u32,
    pub priority: u16,
    pub phases: Vec<HookPhase>,
    pub state_class: StateClass,
    pub policy: BuiltinPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineDefinition {
    pub schema_version: u32,
    pub definition_id: String,
    pub revision: u64,
    pub middleware: Vec<MiddlewareSpec>,
    pub contract_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineRunSnapshot {
    pub run_id: String,
    pub definition_id: String,
    pub definition_revision: u64,
    pub contract_hash: String,
    pub policy_hash: String,
    pub capability_snapshot_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MiddlewareRequest {
    pub run_id: String,
    pub correlation_id: String,
    pub idempotency_key: String,
    pub phase: HookPhase,
    pub input_hash: String,
    pub capability_snapshot_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImmutableOverride {
    pub input_hash: String,
    pub source_middleware_id: String,
    pub provenance: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PipelineOutcome {
    Allowed,
    Overridden(ImmutableOverride),
    Blocked { reason: String },
    Duplicate,
    StaleSnapshot,
    Cancelled,
    LimitExceeded,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineEvent {
    pub event_id: String,
    pub run_id: String,
    pub correlation_id: String,
    pub sequence: u64,
    pub phase: HookPhase,
    pub state_class: StateClass,
    pub outcome: PipelineOutcome,
    pub redaction_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    Invalid(&'static str),
    Limit(&'static str),
    UnsupportedVersion(u32),
    CapabilityExpansion,
    EventTooLarge,
}
impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(v) => write!(f, "invalid middleware field: {v}"),
            Self::Limit(v) => write!(f, "middleware limit exceeded: {v}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported middleware version: {v}"),
            Self::CapabilityExpansion => write!(f, "middleware cannot expand capabilities"),
            Self::EventTooLarge => write!(f, "middleware event is too large"),
        }
    }
}
impl std::error::Error for PipelineError {}

fn text(field: &'static str, value: &str) -> Result<(), PipelineError> {
    if value.trim().is_empty() || value.chars().count() > MAX_ID_CHARS {
        Err(PipelineError::Invalid(field))
    } else {
        Ok(())
    }
}
fn hash<T: Serialize>(value: &T) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).expect("contract serializes"),
    ))
}

impl PipelineDefinition {
    pub fn new(
        definition_id: impl Into<String>,
        revision: u64,
        middleware: Vec<MiddlewareSpec>,
    ) -> Result<Self, PipelineError> {
        let mut value = Self {
            schema_version: CONTRACT_VERSION,
            definition_id: definition_id.into(),
            revision,
            middleware,
            contract_hash: String::new(),
        };
        value.contract_hash = value.compute_hash();
        value.validate()?;
        Ok(value)
    }
    pub fn compute_hash(&self) -> String {
        let mut copy = self.clone();
        copy.contract_hash.clear();
        hash(&copy)
    }
    pub fn validate(&self) -> Result<(), PipelineError> {
        if self.schema_version != CONTRACT_VERSION {
            return Err(PipelineError::UnsupportedVersion(self.schema_version));
        }
        text("definition_id", &self.definition_id)?;
        if self.middleware.is_empty() || self.middleware.len() > MAX_MIDDLEWARE {
            return Err(PipelineError::Limit("middleware"));
        }
        let mut ids = BTreeSet::new();
        for item in &self.middleware {
            text("middleware.id", &item.id)?;
            if !ids.insert(&item.id) {
                return Err(PipelineError::Invalid("duplicate middleware"));
            }
            if item.version == 0 || item.phases.is_empty() || item.phases.len() > MAX_PHASES {
                return Err(PipelineError::Limit("phases"));
            }
            if let BuiltinPolicy::Block { reason } = &item.policy {
                if reason.chars().count() > MAX_TEXT_CHARS {
                    return Err(PipelineError::Limit("reason"));
                }
            }
            if let BuiltinPolicy::Narrow { max_bytes } = item.policy {
                if max_bytes == 0 {
                    return Err(PipelineError::Limit("max_bytes"));
                }
            }
        }
        if self.contract_hash != self.compute_hash() {
            return Err(PipelineError::Invalid("contract_hash"));
        }
        Ok(())
    }
}

impl PipelineRunSnapshot {
    pub fn validate_against(
        &self,
        definition: &PipelineDefinition,
        capability_hash: &str,
    ) -> Result<(), PipelineError> {
        definition.validate()?;
        if self.definition_id != definition.definition_id
            || self.definition_revision != definition.revision
            || self.contract_hash != definition.contract_hash
            || self.capability_snapshot_hash != capability_hash
        {
            return Err(PipelineError::Invalid("run snapshot"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AgentMiddlewarePipelineService {
    definition: PipelineDefinition,
    snapshot: PipelineRunSnapshot,
    seen: BTreeSet<String>,
    next_sequence: u64,
}

impl AgentMiddlewarePipelineService {
    pub fn new(
        definition: PipelineDefinition,
        snapshot: PipelineRunSnapshot,
        capability_hash: &str,
    ) -> Result<Self, PipelineError> {
        snapshot.validate_against(&definition, capability_hash)?;
        Ok(Self {
            definition,
            snapshot,
            seen: BTreeSet::new(),
            next_sequence: 0,
        })
    }
    pub fn contract_hash(&self) -> &str {
        &self.snapshot.contract_hash
    }
    pub fn evaluate(
        &mut self,
        request: &MiddlewareRequest,
    ) -> Result<(PipelineOutcome, Vec<PipelineEvent>), PipelineError> {
        if request.run_id != self.snapshot.run_id
            || request.capability_snapshot_hash != self.snapshot.capability_snapshot_hash
        {
            return Err(PipelineError::Invalid("request snapshot"));
        }
        text("correlation_id", &request.correlation_id)?;
        text("idempotency_key", &request.idempotency_key)?;
        text("input_hash", &request.input_hash)?;
        if !self.seen.insert(request.idempotency_key.clone()) {
            return Ok((PipelineOutcome::Duplicate, Vec::new()));
        }
        let mut events = Vec::new();
        let mut outcome = PipelineOutcome::Allowed;
        let mut ordered = self
            .definition
            .middleware
            .iter()
            .filter(|m| m.phases.contains(&request.phase))
            .collect::<Vec<_>>();
        ordered.sort_by_key(|m| (m.priority, m.id.as_str()));
        for middleware in ordered {
            outcome = match &middleware.policy {
                BuiltinPolicy::Observe => PipelineOutcome::Allowed,
                BuiltinPolicy::Narrow { max_bytes } => {
                    PipelineOutcome::Overridden(ImmutableOverride {
                        input_hash: request.input_hash.clone(),
                        source_middleware_id: middleware.id.clone(),
                        provenance: format!("middleware:{}", middleware.version),
                        reason: format!("max_bytes:{max_bytes}"),
                    })
                }
                BuiltinPolicy::Redact { fields } => {
                    PipelineOutcome::Overridden(ImmutableOverride {
                        input_hash: request.input_hash.clone(),
                        source_middleware_id: middleware.id.clone(),
                        provenance: format!("middleware:{}", middleware.version),
                        reason: format!("redact_fields:{}", fields.len()),
                    })
                }
                BuiltinPolicy::Block { reason } => PipelineOutcome::Blocked {
                    reason: reason.clone(),
                },
            };
            self.next_sequence += 1;
            let event = PipelineEvent {
                event_id: format!("{}:{}", request.run_id, self.next_sequence),
                run_id: request.run_id.clone(),
                correlation_id: request.correlation_id.clone(),
                sequence: self.next_sequence,
                phase: request.phase,
                state_class: middleware.state_class,
                outcome: outcome.clone(),
                redaction_status: "metadata_only".into(),
            };
            if serde_json::to_vec(&event)
                .map_err(|_| PipelineError::EventTooLarge)?
                .len()
                > MAX_EVENT_BYTES
            {
                return Err(PipelineError::EventTooLarge);
            }
            events.push(event);
            if matches!(outcome, PipelineOutcome::Blocked { .. }) {
                break;
            }
        }
        if events.len() > MAX_EVENTS {
            return Err(PipelineError::Limit("events"));
        }
        Ok((outcome, events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition(policy: BuiltinPolicy) -> PipelineDefinition {
        PipelineDefinition::new(
            "definition",
            1,
            vec![MiddlewareSpec {
                id: "first".into(),
                version: 1,
                priority: 1,
                phases: vec![HookPhase::BeforeTool],
                state_class: StateClass::Public,
                policy,
            }],
        )
        .unwrap()
    }
    fn service() -> AgentMiddlewarePipelineService {
        let d = definition(BuiltinPolicy::Narrow { max_bytes: 64 });
        let s = PipelineRunSnapshot {
            run_id: "run".into(),
            definition_id: d.definition_id.clone(),
            definition_revision: d.revision,
            contract_hash: d.contract_hash.clone(),
            policy_hash: "policy".into(),
            capability_snapshot_hash: "caps".into(),
        };
        AgentMiddlewarePipelineService::new(d, s, "caps").unwrap()
    }
    #[test]
    fn all_phases_are_versioned() {
        assert_eq!(HookPhase::ALL.len(), 8);
    }
    #[test]
    fn ordering_and_override_are_deterministic() {
        let mut s = service();
        let request = MiddlewareRequest {
            run_id: "run".into(),
            correlation_id: "c".into(),
            idempotency_key: "i".into(),
            phase: HookPhase::BeforeTool,
            input_hash: "h".into(),
            capability_snapshot_hash: "caps".into(),
        };
        let a = s.evaluate(&request).unwrap();
        assert!(matches!(a.0, PipelineOutcome::Overridden(_)));
        assert_eq!(a.1[0].sequence, 1);
    }
    #[test]
    fn duplicate_is_not_replayed() {
        let mut s = service();
        let request = MiddlewareRequest {
            run_id: "run".into(),
            correlation_id: "c".into(),
            idempotency_key: "i".into(),
            phase: HookPhase::BeforeTool,
            input_hash: "h".into(),
            capability_snapshot_hash: "caps".into(),
        };
        s.evaluate(&request).unwrap();
        assert_eq!(s.evaluate(&request).unwrap().0, PipelineOutcome::Duplicate);
    }
    #[test]
    fn snapshot_drift_is_rejected() {
        let d = definition(BuiltinPolicy::Observe);
        let s = PipelineRunSnapshot {
            run_id: "run".into(),
            definition_id: d.definition_id.clone(),
            definition_revision: d.revision,
            contract_hash: d.contract_hash.clone(),
            policy_hash: "policy".into(),
            capability_snapshot_hash: "other".into(),
        };
        assert!(matches!(
            AgentMiddlewarePipelineService::new(d, s, "caps"),
            Err(PipelineError::Invalid("run snapshot"))
        ));
    }
}
