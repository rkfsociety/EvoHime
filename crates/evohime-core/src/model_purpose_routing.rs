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

pub const CONTRACT_VERSION: u32 = 1;
pub const CONTRACT_ID: &str = "model-purpose-routing-v1";
pub const MAX_PURPOSES: usize = 32;
pub const MAX_PROFILE_REF: usize = 128;
pub const MAX_CAPABILITIES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCallPurpose {
    PrimaryReasoning,
    CodeEditing,
    ArchitectureReasoning,
    ToolSelection,
    TeamSelection,
    ContextSelection,
    Summarization,
    Compaction,
    CommitMessage,
    Review,
    Judge,
    Refinement,
    Simulation,
}

impl ModelCallPurpose {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCeiling {
    NoTools,
    ReadOnly,
    WorkspaceSafe,
    Granted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPolicy {
    Minimal,
    Task,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurposeRequirements {
    pub capabilities: BTreeSet<String>,
    pub required_privacy: PrivacyClass,
    pub tool_ceiling: ToolCeiling,
    pub context_policy: ContextPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurposeRoute {
    pub profile_ref: String,
    pub requirements: PurposeRequirements,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPurposeRoutingPolicy {
    pub schema_version: u32,
    pub policy_id: String,
    pub version: u64,
    pub routes: BTreeMap<ModelCallPurpose, PurposeRoute>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RoutingError {
    #[error("unsupported model purpose routing schema")]
    UnsupportedVersion,
    #[error("invalid model purpose routing policy: {0}")]
    Invalid(&'static str),
    #[error("purpose route is not configured")]
    MissingPurpose,
}

impl ModelPurposeRoutingPolicy {
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

    pub fn canonical_hash(&self) -> Result<String, RoutingError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| RoutingError::Invalid("serialization"))?;
        let mut hash = Sha256::new();
        hash.update(CONTRACT_ID.as_bytes());
        hash.update([0]);
        hash.update(bytes);
        Ok(hex::encode(hash.finalize()))
    }

    pub fn route(&self, purpose: ModelCallPurpose) -> Result<&PurposeRoute, RoutingError> {
        self.validate()?;
        self.routes
            .get(&purpose)
            .ok_or(RoutingError::MissingPurpose)
    }
}

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
