//! Core-owned purpose-to-model-profile routing policy.
//!
//! This contract selects a primary profile hint for a model-call purpose.
//! Retry and fallback remain owned by `model_resilience_policy` and the model
//! gateway. The policy contains metadata only; credentials and prompts never
//! enter it.
use evohime_model_gateway::provider_contract::PrivacyClass;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Schema version for purpose-routing policies.
pub const CONTRACT_VERSION: u32 = 1;
/// Stable identifier included in routing policy hashes.
pub const CONTRACT_ID: &str = "model-purpose-routing-v1";
/// Maximum number of distinct purposes a policy may route.
pub const MAX_PURPOSES: usize = 32;
/// Maximum length of a profile reference or capability name.
pub const MAX_PROFILE_REF: usize = 128;
/// Maximum number of capabilities required by one purpose.
pub const MAX_CAPABILITIES: usize = 16;

/// Category of model call used to select a profile and invocation requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCallPurpose {
    /// Main user-facing reasoning call.
    PrimaryReasoning,
    /// Producing or modifying source code.
    CodeEditing,
    /// Reasoning about system structure and component boundaries.
    ArchitectureReasoning,
    /// Selecting an appropriate tool for a task.
    ToolSelection,
    /// Selecting collaborators for a task.
    TeamSelection,
    /// Selecting relevant context for a model call.
    ContextSelection,
    /// Condensing information into a shorter representation.
    Summarization,
    /// Compressing accumulated conversation or task state.
    Compaction,
    /// Producing a commit message from a change set.
    CommitMessage,
    /// Reviewing code, plans, or other artifacts.
    Review,
    /// Judging or ranking candidate outputs.
    Judge,
    /// Refining an existing candidate output.
    Refinement,
    /// Simulating a proposed action or outcome.
    Simulation,
}

impl ModelCallPurpose {
    /// Returns the stable snake-case identifier used in serialized contracts.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrimaryReasoning => "primary_reasoning",
            Self::CodeEditing => "code_editing",
            Self::ArchitectureReasoning => "architecture_reasoning",
            Self::ToolSelection => "tool_selection",
            Self::TeamSelection => "team_selection",
            Self::ContextSelection => "context_selection",
            Self::Summarization => "summarization",
            Self::Compaction => "compaction",
            Self::CommitMessage => "commit_message",
            Self::Review => "review",
            Self::Judge => "judge",
            Self::Refinement => "refinement",
            Self::Simulation => "simulation",
        }
    }
}

/// Maximum tool authority permitted for a purpose route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCeiling {
    /// Model calls must not receive tools.
    NoTools,
    /// Only read-only tools may be exposed.
    ReadOnly,
    /// Workspace-safe tools may be exposed.
    WorkspaceSafe,
    /// Tools granted by the active authorization may be exposed.
    Granted,
}

/// Amount of task context a purpose route may receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPolicy {
    /// Include only the minimum routing and request context.
    Minimal,
    /// Include context selected for the current task.
    Task,
    /// Permit the full available context set.
    Full,
}

/// Privacy, capability, tool, and context requirements for one model purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurposeRequirements {
    /// Capabilities the chosen profile must support.
    pub capabilities: BTreeSet<String>,
    /// Minimum privacy classification required of the provider.
    pub required_privacy: PrivacyClass,
    /// Maximum tool authority allowed for the invocation.
    pub tool_ceiling: ToolCeiling,
    /// Context inclusion policy for the invocation.
    pub context_policy: ContextPolicy,
}

/// Selected profile reference and constraints for a model-call purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurposeRoute {
    /// Identifier of the model profile to resolve for this purpose.
    pub profile_ref: String,
    /// Requirements that must accompany this route.
    pub requirements: PurposeRequirements,
}

/// Versioned mapping from model-call purposes to profile requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPurposeRoutingPolicy {
    /// Serialized schema version.
    pub schema_version: u32,
    /// Contract identifier expected by this implementation.
    pub policy_id: String,
    /// Monotonic policy revision; zero is invalid.
    pub version: u64,
    /// Route for each supported model-call purpose.
    pub routes: BTreeMap<ModelCallPurpose, PurposeRoute>,
}

/// Policy validation or purpose lookup failure.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RoutingError {
    /// The serialized policy version is unsupported.
    #[error("unsupported model purpose routing schema")]
    UnsupportedVersion,
    /// A policy field or route violates the contract.
    #[error("invalid model purpose routing policy: {0}")]
    Invalid(&'static str),
    /// No route was configured for the requested purpose.
    #[error("purpose route is not configured")]
    MissingPurpose,
}

impl ModelPurposeRoutingPolicy {
    /// Checks schema identity, bounds, references, and compatible requirement pairs.
    pub fn validate(&self) -> Result<(), RoutingError> {
        if self.schema_version != CONTRACT_VERSION {
            return Err(RoutingError::UnsupportedVersion);
        }
        if self.policy_id != CONTRACT_ID
            || self.version == 0
            || self.routes.is_empty()
            || self.routes.len() > MAX_PURPOSES
        {
            return Err(RoutingError::Invalid("identity or bounds"));
        }
        for route in self.routes.values() {
            if route.profile_ref.trim().is_empty()
                || route.profile_ref.len() > MAX_PROFILE_REF
                || route.profile_ref.bytes().any(|b| b.is_ascii_control())
            {
                return Err(RoutingError::Invalid("profile_ref"));
            }
            if route.requirements.capabilities.len() > MAX_CAPABILITIES
                || route.requirements.capabilities.iter().any(|v| {
                    v.is_empty()
                        || v.len() > MAX_PROFILE_REF
                        || v.bytes().any(|b| b.is_ascii_control())
                })
            {
                return Err(RoutingError::Invalid("capabilities"));
            }
            if route.requirements.tool_ceiling == ToolCeiling::NoTools
                && route.requirements.context_policy == ContextPolicy::Full
            {
                return Err(RoutingError::Invalid("no-tools context"));
            }
        }
        Ok(())
    }

    /// Validates the policy and hashes its stable contract identifier and JSON.
    pub fn canonical_hash(&self) -> Result<String, RoutingError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| RoutingError::Invalid("serialization"))?;
        let mut hash = Sha256::new();
        hash.update(CONTRACT_ID.as_bytes());
        hash.update([0]);
        hash.update(bytes);
        Ok(hex::encode(hash.finalize()))
    }

    /// Validates the policy and returns the route configured for `purpose`.
    pub fn route(&self, purpose: ModelCallPurpose) -> Result<&PurposeRoute, RoutingError> {
        self.validate()?;
        self.routes
            .get(&purpose)
            .ok_or(RoutingError::MissingPurpose)
    }
}

/// Builds the built-in routing table with a conservative task-context default.
pub fn builtin_policy() -> ModelPurposeRoutingPolicy {
    let requirements = PurposeRequirements {
        capabilities: ["chat".into()].into_iter().collect(),
        required_privacy: PrivacyClass::Internal,
        tool_ceiling: ToolCeiling::Granted,
        context_policy: ContextPolicy::Task,
    };
    let mut routes = BTreeMap::new();
    for purpose in [
        ModelCallPurpose::PrimaryReasoning,
        ModelCallPurpose::CodeEditing,
        ModelCallPurpose::ArchitectureReasoning,
        ModelCallPurpose::ToolSelection,
        ModelCallPurpose::TeamSelection,
        ModelCallPurpose::ContextSelection,
        ModelCallPurpose::Summarization,
        ModelCallPurpose::Compaction,
        ModelCallPurpose::CommitMessage,
        ModelCallPurpose::Review,
        ModelCallPurpose::Judge,
        ModelCallPurpose::Refinement,
        ModelCallPurpose::Simulation,
    ] {
        routes.insert(
            purpose,
            PurposeRoute {
                profile_ref: "default".into(),
                requirements: requirements.clone(),
            },
        );
    }
    ModelPurposeRoutingPolicy {
        schema_version: CONTRACT_VERSION,
        policy_id: CONTRACT_ID.into(),
        version: 1,
        routes,
    }
}

/// Maps a known task-class label to its model purpose, defaulting to reasoning.
pub fn purpose_for_task_class(task_class: Option<&str>) -> ModelCallPurpose {
    match task_class.unwrap_or_default() {
        "code_editing" | "editing" => ModelCallPurpose::CodeEditing,
        "architecture" | "architecture_reasoning" => ModelCallPurpose::ArchitectureReasoning,
        "tool_selection" => ModelCallPurpose::ToolSelection,
        "review" => ModelCallPurpose::Review,
        "summarization" => ModelCallPurpose::Summarization,
        "compaction" => ModelCallPurpose::Compaction,
        "simulation" => ModelCallPurpose::Simulation,
        _ => ModelCallPurpose::PrimaryReasoning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_policy_covers_all_purposes_and_hashes() {
        let p = builtin_policy();
        assert_eq!(p.routes.len(), 13);
        assert_eq!(p.canonical_hash().unwrap().len(), 64);
    }
    #[test]
    fn no_tools_cannot_claim_full_context() {
        let mut p = builtin_policy();
        let route = p.routes.get_mut(&ModelCallPurpose::Review).unwrap();
        route.requirements.tool_ceiling = ToolCeiling::NoTools;
        route.requirements.context_policy = ContextPolicy::Full;
        assert_eq!(p.validate(), Err(RoutingError::Invalid("no-tools context")));
    }
    #[test]
    fn task_class_mapping_is_stable() {
        assert_eq!(
            purpose_for_task_class(Some("code_editing")),
            ModelCallPurpose::CodeEditing
        );
        assert_eq!(
            purpose_for_task_class(None),
            ModelCallPurpose::PrimaryReasoning
        );
    }
}
