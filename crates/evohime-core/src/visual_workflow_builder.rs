//! Core-owned contract for editing a typed `workflow/v1` graph.
//!
//! Layout is deliberately kept outside the execution graph.  The builder can
//! therefore move nodes without changing the immutable runtime hash.

use crate::workflow::{WorkflowEdge, WorkflowGraph, WorkflowNode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const BUILDER_CONTRACT_VERSION: &str = "visual-workflow-builder/v1";
pub const MAX_DRAFT_BYTES: usize = 512 * 1024;
pub const MAX_LAYOUT_NODES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DraftCommand {
    AddNode {
        node: Box<WorkflowNode>,
        x: i32,
        y: i32,
    },
    RemoveNode {
        node_id: String,
    },
    MoveNode {
        node_id: String,
        x: i32,
        y: i32,
    },
    Connect {
        edge: WorkflowEdge,
    },
}

impl DraftCommand {
    pub fn apply(
        &self,
        definition: &mut VisualWorkflowBuilderDefinition,
    ) -> Result<(), BuilderError> {
        match self {
            Self::AddNode { node, x, y } => {
                if definition
                    .graph
                    .nodes
                    .iter()
                    .any(|existing| existing.id == node.id)
                {
                    return Err(BuilderError::DuplicateNode);
                }
                definition.graph.nodes.push((**node).clone());
                definition.layout.nodes.push(LayoutNode {
                    node_id: node.id.clone(),
                    x: *x,
                    y: *y,
                });
            }
            Self::RemoveNode { node_id } => {
                definition.graph.nodes.retain(|node| node.id != *node_id);
                definition
                    .graph
                    .edges
                    .retain(|edge| edge.from_node != *node_id && edge.to_node != *node_id);
                definition
                    .layout
                    .nodes
                    .retain(|node| node.node_id != *node_id);
            }
            Self::MoveNode { node_id, x, y } => {
                let layout = definition
                    .layout
                    .nodes
                    .iter_mut()
                    .find(|node| node.node_id == *node_id)
                    .ok_or(BuilderError::UnknownLayoutNode)?;
                layout.x = *x;
                layout.y = *y;
            }
            Self::Connect { edge } => definition.graph.edges.push(edge.clone()),
        }
        definition.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutNode {
    pub node_id: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkflowLayout {
    pub nodes: Vec<LayoutNode>,
}

impl WorkflowLayout {
    pub fn canonical_hash(&self) -> String {
        let mut nodes = self.nodes.clone();
        nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        let bytes = serde_json::to_vec(&nodes).expect("layout is serializable");
        hex::encode(Sha256::digest(bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualWorkflowBuilderDefinition {
    pub contract_version: String,
    pub graph: WorkflowGraph,
    pub layout: WorkflowLayout,
}

impl VisualWorkflowBuilderDefinition {
    pub fn validate(&self) -> Result<(), BuilderError> {
        if self.contract_version != BUILDER_CONTRACT_VERSION {
            return Err(BuilderError::UnsupportedVersion);
        }
        if self.layout.nodes.len() > MAX_LAYOUT_NODES {
            return Err(BuilderError::Limit("layout.nodes"));
        }
        self.graph
            .validate()
            .map_err(|_| BuilderError::InvalidGraph)?;
        let graph_ids = self
            .graph
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if self
            .layout
            .nodes
            .iter()
            .any(|node| !graph_ids.contains(node.node_id.as_str()))
        {
            return Err(BuilderError::UnknownLayoutNode);
        }
        let encoded = serde_json::to_vec(&self.graph).expect("graph is serializable");
        if encoded.len() > MAX_DRAFT_BYTES {
            return Err(BuilderError::Limit("graph"));
        }
        Ok(())
    }

    pub fn execution_hash(&self) -> String {
        self.graph.canonical_hash()
    }
    pub fn layout_hash(&self) -> String {
        self.layout.canonical_hash()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuilderError {
    #[error("unsupported builder contract version")]
    UnsupportedVersion,
    #[error("invalid workflow graph")]
    InvalidGraph,
    #[error("bounded field exceeds limit: {0}")]
    Limit(&'static str),
    #[error("layout references an unknown node")]
    UnknownLayoutNode,
    #[error("node already exists")]
    DuplicateNode,
    #[error("workflow block or capability is not registered")]
    RegistryRejected,
    #[error("draft revision is stale")]
    StaleRevision,
    #[error("handoff is stale or revoked")]
    InvalidHandoff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuilderHandoff {
    pub handle: String,
    pub contract_version: String,
    pub owner_scope: String,
    pub draft_revision: u64,
    pub draft_hash: String,
    pub save_precondition: String,
    pub single_use: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftRecord {
    pub owner_scope: String,
    pub revision: u64,
    pub definition: VisualWorkflowBuilderDefinition,
    pub handoff: Option<BuilderHandoff>,
}

/// Small Core-side state machine used by the IPC service and deterministic tests.
#[derive(Debug, Default)]
pub struct BuilderDraftStore {
    drafts: std::collections::BTreeMap<String, DraftRecord>,
    consumed: std::collections::BTreeSet<String>,
}

impl BuilderDraftStore {
    pub fn put(&mut self, draft_id: impl Into<String>, record: DraftRecord) {
        self.drafts.insert(draft_id.into(), record);
    }

    pub fn issue_handoff(
        &mut self,
        draft_id: &str,
        owner_scope: &str,
    ) -> Result<BuilderHandoff, BuilderError> {
        let draft = self
            .drafts
            .get_mut(draft_id)
            .ok_or(BuilderError::InvalidHandoff)?;
        if draft.owner_scope != owner_scope {
            return Err(BuilderError::InvalidHandoff);
        }
        draft.definition.validate()?;
        let draft_hash = draft.definition.execution_hash();
        let handle = format!("builder-handoff:{draft_id}:{}", draft.revision);
        let handoff = BuilderHandoff {
            handle: handle.clone(),
            contract_version: BUILDER_CONTRACT_VERSION.into(),
            owner_scope: owner_scope.into(),
            draft_revision: draft.revision,
            draft_hash: draft_hash.clone(),
            save_precondition: format!("{}:{}", draft.revision, draft_hash),
            single_use: true,
        };
        draft.handoff = Some(handoff.clone());
        Ok(handoff)
    }

    pub fn consume_handoff(
        &mut self,
        draft_id: &str,
        owner_scope: &str,
        handle: &str,
    ) -> Result<DraftRecord, BuilderError> {
        let draft = self
            .drafts
            .get(draft_id)
            .ok_or(BuilderError::InvalidHandoff)?
            .clone();
        let handoff = draft.handoff.as_ref().ok_or(BuilderError::InvalidHandoff)?;
        if draft.owner_scope != owner_scope
            || handoff.handle != handle
            || !handoff.single_use
            || self.consumed.contains(handle)
            || handoff.draft_revision != draft.revision
            || handoff.draft_hash != draft.definition.execution_hash()
        {
            return Err(BuilderError::InvalidHandoff);
        }
        self.consumed.insert(handle.into());
        Ok(draft)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{
        ApprovalPolicy, CancellationPolicy, ExecutionPolicy, NodeType, RetryPolicy, WorkflowNode,
    };

    fn definition() -> VisualWorkflowBuilderDefinition {
        let execution = ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts: 1,
                backoff_ms: 0,
                retryable_errors: vec![],
            },
            timeout_ms: 1_000,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: false,
                reason: None,
            },
        };
        VisualWorkflowBuilderDefinition {
            contract_version: BUILDER_CONTRACT_VERSION.into(),
            graph: WorkflowGraph {
                contract: crate::workflow::WORKFLOW_CONTRACT_VERSION.into(),
                graph_id: "g".into(),
                version: 1,
                entry_node: "start".into(),
                nodes: vec![WorkflowNode::new("start", NodeType::Transform, execution)],
                edges: vec![],
                budget: Default::default(),
            },
            layout: WorkflowLayout {
                nodes: vec![LayoutNode {
                    node_id: "start".into(),
                    x: 1,
                    y: 2,
                }],
            },
        }
    }

    #[test]
    fn layout_does_not_change_execution_hash() {
        let mut left = definition();
        let right_hash = left.execution_hash();
        left.layout.nodes[0].x = 900;
        assert_eq!(left.execution_hash(), right_hash);
        assert_ne!(left.layout_hash(), definition().layout_hash());
    }

    #[test]
    fn handoff_is_owner_scoped_and_single_use() {
        let mut store = BuilderDraftStore::default();
        store.put(
            "d",
            DraftRecord {
                owner_scope: "workspace:a".into(),
                revision: 1,
                definition: definition(),
                handoff: None,
            },
        );
        let handoff = store.issue_handoff("d", "workspace:a").unwrap();
        assert!(store
            .consume_handoff("d", "workspace:b", &handoff.handle)
            .is_err());
        assert!(store
            .consume_handoff("d", "workspace:a", &handoff.handle)
            .is_ok());
        assert!(store
            .consume_handoff("d", "workspace:a", &handoff.handle)
            .is_err());
    }
}
