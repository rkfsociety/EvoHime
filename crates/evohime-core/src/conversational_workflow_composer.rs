//! Core-owned natural-language workflow proposal contract.
//!
//! Model output is parsed into a closed proposal envelope and is never treated
//! as an executable workflow. Only the validated Builder definition may cross
//! the authoring boundary.

use crate::visual_workflow_builder::{DraftCommand, VisualWorkflowBuilderDefinition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const REQUEST_VERSION: &str = "composer-request/v1";
pub const PROPOSAL_VERSION: &str = "composer-proposal/v1";
pub const PROVENANCE_VERSION: &str = "composer-provenance/v1";
pub const MAX_REQUEST_BYTES: usize = 16 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 512 * 1024;
pub const MAX_ASSUMPTIONS: usize = 32;
pub const MAX_ASSUMPTION_BYTES: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalEnvelope {
    pub schema_version: String,
    pub proposal_id: String,
    pub definition: VisualWorkflowBuilderDefinition,
    #[serde(default)]
    pub assumptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposerProvenance {
    pub schema_version: String,
    pub request_hash: String,
    pub proposal_hash: String,
    pub catalog_hash: String,
    pub model_route: String,
    pub model_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComposerError {
    #[error("request exceeds composer limit")]
    RequestTooLarge,
    #[error("model response exceeds composer limit")]
    ResponseTooLarge,
    #[error("unsupported composer proposal version")]
    UnsupportedVersion,
    #[error("malformed composer proposal")]
    MalformedProposal,
    #[error("composer proposal exceeds bounds")]
    Limit,
    #[error("composer proposal is not a valid workflow")]
    InvalidDefinition,
}

pub fn request_hash(request: &[u8]) -> Result<String, ComposerError> {
    if request.is_empty() || request.len() > MAX_REQUEST_BYTES {
        return Err(ComposerError::RequestTooLarge);
    }
    Ok(hex::encode(Sha256::digest(request)))
}

pub fn parse_proposal(response: &[u8]) -> Result<ProposalEnvelope, ComposerError> {
    if response.len() > MAX_RESPONSE_BYTES {
        return Err(ComposerError::ResponseTooLarge);
    }
    let proposal: ProposalEnvelope =
        serde_json::from_slice(response).map_err(|_| ComposerError::MalformedProposal)?;
    if proposal.schema_version != PROPOSAL_VERSION
        || proposal.proposal_id.trim().is_empty()
        || proposal.assumptions.len() > MAX_ASSUMPTIONS
        || proposal
            .assumptions
            .iter()
            .any(|value| value.is_empty() || value.len() > MAX_ASSUMPTION_BYTES)
    {
        return Err(if proposal.schema_version != PROPOSAL_VERSION {
            ComposerError::UnsupportedVersion
        } else {
            ComposerError::Limit
        });
    }
    proposal
        .definition
        .validate()
        .map_err(|_| ComposerError::InvalidDefinition)?;
    Ok(proposal)
}

pub fn canonical_proposal(proposal: &ProposalEnvelope) -> Result<Vec<u8>, ComposerError> {
    serde_json::to_vec(proposal).map_err(|_| ComposerError::MalformedProposal)
}

pub fn apply_edit(
    definition: &mut VisualWorkflowBuilderDefinition,
    command: &DraftCommand,
) -> Result<(), ComposerError> {
    command
        .apply(definition)
        .map_err(|_| ComposerError::InvalidDefinition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual_workflow_builder::{WorkflowLayout, BUILDER_CONTRACT_VERSION};
    use crate::workflow::{
        ApprovalPolicy, CancellationPolicy, ExecutionPolicy, NodeType, RetryPolicy, WorkflowGraph,
        WorkflowNode,
    };

    fn proposal() -> ProposalEnvelope {
        ProposalEnvelope {
            schema_version: PROPOSAL_VERSION.into(),
            proposal_id: "p1".into(),
            definition: VisualWorkflowBuilderDefinition {
                contract_version: BUILDER_CONTRACT_VERSION.into(),
                graph: WorkflowGraph {
                    contract: crate::workflow::WORKFLOW_CONTRACT_VERSION.into(),
                    graph_id: "g".into(),
                    version: 1,
                    entry_node: "start".into(),
                    nodes: vec![WorkflowNode::new(
                        "start",
                        NodeType::Transform,
                        ExecutionPolicy {
                            retry: RetryPolicy {
                                max_attempts: 1,
                                backoff_ms: 0,
                                retryable_errors: vec![],
                            },
                            timeout_ms: 1000,
                            cancellation: CancellationPolicy::Cooperative,
                            approval: ApprovalPolicy {
                                required: false,
                                reason: None,
                            },
                        },
                    )],
                    edges: vec![],
                    budget: Default::default(),
                },
                layout: WorkflowLayout::default(),
            },
            assumptions: vec!["user will review before save".into()],
        }
    }

    #[test]
    fn proposal_is_closed_and_validated() {
        let bytes = serde_json::to_vec(&proposal()).unwrap();
        assert_eq!(parse_proposal(&bytes).unwrap().proposal_id, "p1");
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(matches!(
            parse_proposal(&serde_json::to_vec(&value).unwrap()),
            Err(ComposerError::MalformedProposal)
        ));
    }

    #[test]
    fn hashes_are_bounded_and_stable() {
        let bytes = b"request";
        assert_eq!(request_hash(bytes), request_hash(bytes));
        assert_eq!(request_hash(&[]), Err(ComposerError::RequestTooLarge));
    }
}
