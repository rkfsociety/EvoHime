//! Core-owned, bounded middleware pipeline around agent/model/tool phases.
//!
//! Middleware is a typed policy description, not executable imported code. It
//! may observe, narrow or block an already-authorized operation; the caller's
//! capability set is never expanded here.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Serialized schema version for middleware pipeline definitions.
pub const CONTRACT_VERSION: u32 = 1;
/// Stable identifier for the middleware pipeline contract.
pub const CONTRACT_ID: &str = "agent-middleware-pipeline-v1";
/// Maximum middleware specifications in one pipeline.
pub const MAX_MIDDLEWARE: usize = 32;
/// Maximum hook phases attached to one middleware specification.
pub const MAX_PHASES: usize = 8;
/// Maximum character count for pipeline identifiers and keys.
pub const MAX_ID_CHARS: usize = 128;
/// Maximum character count for policy reason text.
pub const MAX_TEXT_CHARS: usize = 512;
/// Maximum events emitted for one pipeline evaluation.
pub const MAX_EVENTS: usize = 256;
/// Maximum serialized size of one pipeline event.
pub const MAX_EVENT_BYTES: usize = 16 * 1024;
/// Maximum nested middleware intervention depth.
pub const MAX_INTERVENTION_DEPTH: u8 = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
/// Lifecycle hook at which a middleware policy may observe or constrain work.
pub enum HookPhase {
    /// Before agent processing begins.
    BeforeAgent,
    /// After agent processing completes.
    AfterAgent,
    /// Before model invocation.
    BeforeModel,
    /// After model response processing.
    AfterModel,
    /// Around a model call while preserving the model authorization boundary.
    WrapModelCall,
    /// Before a tool call is admitted.
    BeforeTool,
    /// Around a tool call while preserving existing tool grants.
    WrapToolCall,
    /// After a tool call completes.
    AfterTool,
    /// Before handing work to another agent.
    BeforeHandoff,
    /// Before workflow state becomes durable.
    BeforeWorkflowStateCommit,
    /// Before content is sent to an external destination.
    BeforeExternalPublish,
}

impl HookPhase {
    /// All hook phases supported by this contract in stable order.
    pub const ALL: [Self; 11] = [
        Self::BeforeAgent,
        Self::AfterAgent,
        Self::BeforeModel,
        Self::AfterModel,
        Self::WrapModelCall,
        Self::BeforeTool,
        Self::WrapToolCall,
        Self::AfterTool,
        Self::BeforeHandoff,
        Self::BeforeWorkflowStateCommit,
        Self::BeforeExternalPublish,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Visibility classification for middleware-produced state and events.
pub enum StateClass {
    /// Visible only to the owning execution context.
    Private,
    /// Persistable state intended for recovery.
    Checkpoint,
    /// Safe metadata that may be exposed to the parent or UI.
    Public,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
/// Non-executable policy action supported by the middleware contract.
pub enum BuiltinPolicy {
    /// Record metadata without changing the operation.
    Observe,
    /// Reduce the input to a bounded byte size.
    Narrow {
        /// Maximum byte length retained after applying this policy.
        max_bytes: u32,
    },
    /// Remove the named fields from the visible input.
    Redact {
        /// Field names removed from the middleware-visible value.
        fields: Vec<String>,
    },
    /// Prevent the operation with a stable reason.
    Block {
        /// Bounded reason recorded for the blocked operation.
        reason: String,
    },
    /// Operation is waiting for an approval decision.
    PauseForApproval {
        /// Bounded reason attached to the approval request.
        reason: String,
    },
    /// Terminate the current operation with a stable reason.
    Abort {
        /// Bounded reason recorded for the aborted operation.
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Declared middleware interaction mode.
pub enum HandlerMode {
    /// Observe execution without transforming inputs or results.
    ObserveOnly,
    /// Apply a built-in allow, narrow, or block decision.
    Policy,
    /// Apply a declared deterministic transformation.
    Transform,
    /// Pause at an explicit approval boundary.
    ApprovalGate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Behavior when middleware evaluation fails.
pub enum FailurePolicy {
    /// Block when middleware evaluation fails.
    FailClosed,
    /// Fail the enclosing operation when middleware evaluation fails.
    FailOperation,
    /// Continue when middleware evaluation fails, subject to caller policy.
    FailOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Versioned middleware policy bound to selected hook phases.
pub struct MiddlewareSpec {
    /// Stable middleware identifier.
    pub id: String,
    /// Positive middleware specification revision.
    pub version: u32,
    /// Ordering priority; lower values execute first.
    pub priority: u16,
    /// Hook phases where this policy is evaluated.
    pub phases: Vec<HookPhase>,
    /// Visibility class attached to middleware events.
    pub state_class: StateClass,
    /// Built-in non-executable policy action.
    pub policy: BuiltinPolicy,
    /// Declared interaction mode for this middleware.
    pub mode: HandlerMode,
    /// Failure behavior selected for this middleware.
    pub failure_policy: FailurePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Hashed ordered set of middleware policy specifications.
pub struct PipelineDefinition {
    /// Serialized contract version supported by this definition.
    pub schema_version: u32,
    /// Stable pipeline definition identifier.
    pub definition_id: String,
    /// Monotonic pipeline definition revision.
    pub revision: u64,
    /// Middleware policies evaluated for this pipeline.
    pub middleware: Vec<MiddlewareSpec>,
    /// Integrity hash of the canonical pipeline definition.
    pub contract_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Immutable definition and capability snapshot bound to one run.
pub struct PipelineRunSnapshot {
    /// Execution run bound to the pipeline snapshot.
    pub run_id: String,
    /// Stable pipeline definition identifier.
    pub definition_id: String,
    /// Pipeline definition revision captured for the run.
    pub definition_revision: u64,
    /// Integrity hash of the canonical pipeline definition.
    pub contract_hash: String,
    /// Hash of the effective policy snapshot.
    pub policy_hash: String,
    /// Hash of the capability set that must remain unchanged.
    pub capability_snapshot_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Idempotent request to evaluate middleware at one hook phase.
pub struct MiddlewareRequest {
    /// Execution run bound to the pipeline snapshot.
    pub run_id: String,
    /// Request correlation identifier.
    pub correlation_id: String,
    /// Stable key preventing duplicate policy evaluation.
    pub idempotency_key: String,
    /// Lifecycle phase being evaluated.
    pub phase: HookPhase,
    /// Hash of the immutable input being evaluated.
    pub input_hash: String,
    /// Hash of the capability set that must remain unchanged.
    pub capability_snapshot_hash: String,
    #[serde(default)]
    /// Nested intervention depth used to prevent reentrant loops.
    pub intervention_depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Auditable narrowed input produced by a middleware policy.
pub struct ImmutableOverride {
    /// Hash of the immutable input being evaluated.
    pub input_hash: String,
    /// Middleware policy that produced this override.
    pub source_middleware_id: String,
    /// Provenance label for the immutable override.
    pub provenance: String,
    /// Bounded explanation for the policy action.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Decision returned by middleware evaluation.
pub enum PipelineOutcome {
    /// No policy blocked or narrowed the operation.
    Allowed,
    /// Input was narrowed or redacted by a middleware policy.
    Overridden(ImmutableOverride),
    /// A middleware policy blocked the operation.
    Blocked {
        /// Policy reason that prevented the operation.
        reason: String,
    },
    /// Idempotency key was already evaluated.
    Duplicate,
    /// Run or capability snapshot no longer matches.
    StaleSnapshot,
    /// Evaluation was cancelled.
    Cancelled,
    /// A supported bound was exceeded.
    LimitExceeded,
    /// Middleware evaluation could not be completed.
    Unavailable,
    /// Outcome could not be determined.
    Unknown,
    /// Operation is waiting for an approval decision.
    PauseForApproval {
        /// Policy reason requiring an approval decision.
        reason: String,
    },
    /// A middleware policy aborted the operation.
    Aborted {
        /// Policy reason that terminated the operation.
        reason: String,
    },
    /// Maximum nested intervention depth was reached.
    ReentrantLimit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Ordered metadata-only event emitted for one evaluated middleware.
pub struct PipelineEvent {
    /// Stable identifier for this pipeline event.
    pub event_id: String,
    /// Execution run bound to the pipeline snapshot.
    pub run_id: String,
    /// Request correlation identifier.
    pub correlation_id: String,
    /// Monotonic event sequence within the run.
    pub sequence: u64,
    /// Lifecycle phase being evaluated.
    pub phase: HookPhase,
    /// Visibility class attached to middleware events.
    pub state_class: StateClass,
    /// Middleware decision recorded for this event.
    pub outcome: PipelineOutcome,
    /// Redaction state for the event payload.
    pub redaction_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Invalid contract, exceeded bound, capability expansion, or serialization failure.
pub enum PipelineError {
    /// A pipeline value or invariant is invalid.
    Invalid(&'static str),
    /// A middleware count, phase, or text bound was exceeded.
    Limit(&'static str),
    /// The serialized middleware contract version is unsupported.
    UnsupportedVersion(u32),
    /// Middleware attempted to expand the caller capability set.
    CapabilityExpansion,
    /// A serialized pipeline event exceeded its byte limit.
    EventTooLarge,
    /// A contract value could not be serialized.
    Serialization(String),
}
impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(v) => write!(f, "invalid middleware field: {v}"),
            Self::Limit(v) => write!(f, "middleware limit exceeded: {v}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported middleware version: {v}"),
            Self::CapabilityExpansion => write!(f, "middleware cannot expand capabilities"),
            Self::EventTooLarge => write!(f, "middleware event is too large"),
            Self::Serialization(error) => write!(f, "middleware serialization failed: {error}"),
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
fn hash<T: Serialize>(value: &T) -> Result<String, PipelineError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| PipelineError::Serialization(error.to_string()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

impl PipelineDefinition {
    /// Validates and hashes a versioned pipeline definition.
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
        value.contract_hash = value.compute_hash()?;
        value.validate()?;
        Ok(value)
    }
    /// Computes the canonical definition hash with the stored hash cleared.
    pub fn compute_hash(&self) -> Result<String, PipelineError> {
        let mut copy = self.clone();
        copy.contract_hash.clear();
        hash(&copy)
    }
    /// Validates schema version, middleware uniqueness, phase bounds, and integrity hash.
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
            if let BuiltinPolicy::PauseForApproval { reason } | BuiltinPolicy::Abort { reason } =
                &item.policy
            {
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
        if self.contract_hash != self.compute_hash()? {
            return Err(PipelineError::Invalid("contract_hash"));
        }
        Ok(())
    }
}

impl PipelineRunSnapshot {
    /// Checks the run snapshot matches the pipeline and current capability hash.
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
/// Per-run middleware evaluator with idempotency and stable ordering.
pub struct AgentMiddlewarePipelineService {
    definition: PipelineDefinition,
    snapshot: PipelineRunSnapshot,
    seen: BTreeSet<String>,
    next_sequence: u64,
}

impl AgentMiddlewarePipelineService {
    /// Validates and hashes a versioned pipeline definition.
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
    /// Returns the immutable contract hash captured for this run.
    pub fn contract_hash(&self) -> &str {
        &self.snapshot.contract_hash
    }
    /// Evaluates matching middleware in stable order with idempotency and event bounds.
    pub fn evaluate(
        &mut self,
        request: &MiddlewareRequest,
    ) -> Result<(PipelineOutcome, Vec<PipelineEvent>), PipelineError> {
        if request.run_id != self.snapshot.run_id
            || request.capability_snapshot_hash != self.snapshot.capability_snapshot_hash
        {
            return Err(PipelineError::Invalid("request snapshot"));
        }
        if request.intervention_depth > MAX_INTERVENTION_DEPTH {
            return Ok((PipelineOutcome::ReentrantLimit, Vec::new()));
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
                BuiltinPolicy::PauseForApproval { reason } => PipelineOutcome::PauseForApproval {
                    reason: reason.clone(),
                },
                BuiltinPolicy::Abort { reason } => PipelineOutcome::Aborted {
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
            if matches!(
                outcome,
                PipelineOutcome::Blocked { .. }
                    | PipelineOutcome::PauseForApproval { .. }
                    | PipelineOutcome::Aborted { .. }
            ) {
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
                mode: HandlerMode::Policy,
                failure_policy: FailurePolicy::FailClosed,
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
        assert_eq!(HookPhase::ALL.len(), 11);
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
            intervention_depth: 0,
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
            intervention_depth: 0,
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

    #[test]
    fn pause_abort_and_reentrancy_are_explicit() {
        let pipeline = definition(BuiltinPolicy::PauseForApproval {
            reason: "approve".into(),
        });
        let snap = PipelineRunSnapshot {
            run_id: "run".into(),
            definition_id: pipeline.definition_id.clone(),
            definition_revision: pipeline.revision,
            contract_hash: pipeline.contract_hash.clone(),
            policy_hash: "policy".into(),
            capability_snapshot_hash: "caps".into(),
        };
        let mut pipeline = AgentMiddlewarePipelineService::new(pipeline, snap, "caps").unwrap();
        let mut req = MiddlewareRequest {
            run_id: "run".into(),
            correlation_id: "c".into(),
            idempotency_key: "pause".into(),
            phase: HookPhase::BeforeTool,
            input_hash: "h".into(),
            capability_snapshot_hash: "caps".into(),
            intervention_depth: 0,
        };
        assert!(matches!(
            pipeline.evaluate(&req).unwrap().0,
            PipelineOutcome::PauseForApproval { .. }
        ));
        let abort_definition = definition(BuiltinPolicy::Abort {
            reason: "stop".into(),
        });
        let snap = PipelineRunSnapshot {
            run_id: "run".into(),
            definition_id: abort_definition.definition_id.clone(),
            definition_revision: abort_definition.revision,
            contract_hash: abort_definition.contract_hash.clone(),
            policy_hash: "policy".into(),
            capability_snapshot_hash: "caps".into(),
        };
        let mut abort =
            AgentMiddlewarePipelineService::new(abort_definition, snap, "caps").unwrap();
        req.idempotency_key = "abort".into();
        assert!(matches!(
            abort.evaluate(&req).unwrap().0,
            PipelineOutcome::Aborted { .. }
        ));
        req.intervention_depth = MAX_INTERVENTION_DEPTH + 1;
        assert_eq!(
            abort.evaluate(&req).unwrap().0,
            PipelineOutcome::ReentrantLimit
        );
    }
}
