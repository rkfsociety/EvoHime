use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current serialized schema version for runtime service graphs.
pub const SCHEMA_VERSION: u32 = 1;
const MAX_ID: usize = 128;
const MAX_NODES: usize = 64;
const MAX_EDGES: usize = 128;
const MAX_SCOPE: usize = 256;

/// Lifecycle state of a runtime service graph revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Graph is being assembled and cannot be pinned.
    Draft,
    /// Graph has passed validation and can be pinned to a run.
    Active,
    /// Graph revision has been replaced by a newer one.
    Superseded,
    /// Graph is invalid and cannot be used.
    Invalid,
}

/// One bounded service or capability owner in the graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceNode {
    /// Stable node identifier, unique within the graph.
    pub id: String,
    /// Service category represented by the node.
    pub kind: String,
    /// Scope or component that owns the service.
    pub owner: String,
    /// Capability associated with this service node.
    pub capability: String,
}

/// Directed relationship between two service nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceEdge {
    /// Identifier of the relationship's source node.
    pub from: String,
    /// Identifier of the relationship's destination node.
    pub to: String,
    /// Relationship kind, such as a dependency.
    pub relation: String,
}

/// Content-addressed graph describing bounded Core runtime services.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeServiceGraph {
    /// Serialized schema version.
    pub schema_version: u32,
    /// Stable graph identifier.
    pub id: String,
    /// Monotonically increasing graph revision.
    pub revision: u64,
    /// Lifecycle state controlling whether the graph can be pinned.
    pub lifecycle: Lifecycle,
    /// Workspace or runtime scope represented by this graph.
    pub scope: String,
    /// Service nodes included in the graph.
    pub nodes: Vec<ServiceNode>,
    /// Directed relationships between graph nodes.
    pub edges: Vec<ServiceEdge>,
    /// SHA-256 digest of the canonical graph with this field cleared.
    pub content_hash: String,
}

/// Reference that binds a run to one immutable graph revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PinnedRevision {
    /// Identifier of the pinned graph.
    pub graph_id: String,
    /// Revision selected for the run.
    pub revision: u64,
    /// Digest of the selected graph content.
    pub content_hash: String,
    /// Run that owns this graph pin.
    pub run_id: String,
}

/// Validation failure for a runtime service graph or pin request.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GraphError {
    /// The graph violated schema, bounds, references, lifecycle, or digest requirements.
    #[error("invalid runtime service graph: {0}")]
    Invalid(String),
}

fn bounded(value: &str, limit: usize, name: &str) -> Result<(), GraphError> {
    if value.trim().is_empty() || value.len() > limit {
        return Err(GraphError::Invalid(format!("{name}_out_of_bounds")));
    }
    Ok(())
}

/// Computes the canonical SHA-256 digest with `content_hash` cleared.
pub fn canonical_hash(graph: &RuntimeServiceGraph) -> Result<String, GraphError> {
    let mut normalized = graph.clone();
    normalized.content_hash.clear();
    let bytes = serde_json::to_vec(&normalized)
        .map_err(|_| GraphError::Invalid("graph_not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Validates schema, bounds, node uniqueness, edge references, and content digest.
pub fn validate(graph: &RuntimeServiceGraph) -> Result<(), GraphError> {
    if graph.schema_version != SCHEMA_VERSION {
        return Err(GraphError::Invalid("unsupported_schema_version".into()));
    }
    bounded(&graph.id, MAX_ID, "graph_id")?;
    bounded(&graph.scope, MAX_SCOPE, "scope")?;
    if graph.revision == 0 || graph.nodes.is_empty() || graph.nodes.len() > MAX_NODES {
        return Err(GraphError::Invalid("graph_size_or_revision_invalid".into()));
    }
    if graph.edges.len() > MAX_EDGES {
        return Err(GraphError::Invalid("edge_count_out_of_bounds".into()));
    }
    let ids: std::collections::BTreeSet<_> =
        graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if ids.len() != graph.nodes.len() {
        return Err(GraphError::Invalid("duplicate_node_id".into()));
    }
    for node in &graph.nodes {
        bounded(&node.id, MAX_ID, "node_id")?;
        bounded(&node.kind, MAX_ID, "node_kind")?;
        bounded(&node.owner, MAX_ID, "node_owner")?;
        bounded(&node.capability, MAX_ID, "node_capability")?;
    }
    for edge in &graph.edges {
        bounded(&edge.from, MAX_ID, "edge_from")?;
        bounded(&edge.to, MAX_ID, "edge_to")?;
        bounded(&edge.relation, MAX_ID, "edge_relation")?;
        if !ids.contains(edge.from.as_str()) || !ids.contains(edge.to.as_str()) {
            return Err(GraphError::Invalid("edge_references_unknown_node".into()));
        }
    }
    if canonical_hash(graph)? != graph.content_hash {
        return Err(GraphError::Invalid("content_hash_mismatch".into()));
    }
    Ok(())
}

/// Pins a valid active graph revision to a run.
///
/// # Example
///
/// ```
/// use evohime_core::runtime_service_graph::{
///     canonical_hash, pin, Lifecycle, RuntimeServiceGraph, ServiceNode, SCHEMA_VERSION,
/// };
///
/// let mut graph = RuntimeServiceGraph {
///     schema_version: SCHEMA_VERSION,
///     id: "runtime".into(),
///     revision: 1,
///     lifecycle: Lifecycle::Active,
///     scope: "workspace".into(),
///     nodes: vec![ServiceNode {
///         id: "core".into(),
///         kind: "service".into(),
///         owner: "evohime-core".into(),
///         capability: "runtime.execute".into(),
///     }],
///     edges: Vec::new(),
///     content_hash: String::new(),
/// };
/// graph.content_hash = canonical_hash(&graph)?;
/// let pinned = pin(&graph, "run-1")?;
/// assert_eq!(pinned.revision, 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn pin(graph: &RuntimeServiceGraph, run_id: &str) -> Result<PinnedRevision, GraphError> {
    bounded(run_id, MAX_ID, "run_id")?;
    validate(graph)?;
    if graph.lifecycle != Lifecycle::Active {
        return Err(GraphError::Invalid("graph_is_not_active".into()));
    }
    Ok(PinnedRevision {
        graph_id: graph.id.clone(),
        revision: graph.revision,
        content_hash: graph.content_hash.clone(),
        run_id: run_id.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(lifecycle: Lifecycle) -> RuntimeServiceGraph {
        let mut value = RuntimeServiceGraph {
            schema_version: SCHEMA_VERSION,
            id: "graph".into(),
            revision: 1,
            lifecycle,
            scope: "workspace".into(),
            nodes: vec![ServiceNode {
                id: "core".into(),
                kind: "owner".into(),
                owner: "core-policy".into(),
                capability: "runtime.observe".into(),
            }],
            edges: Vec::new(),
            content_hash: String::new(),
        };
        value.content_hash = canonical_hash(&value).expect("hash");
        value
    }

    #[test]
    fn validates_hash_and_pins_only_active_revision() {
        let active = graph(Lifecycle::Active);
        assert!(validate(&active).is_ok());
        assert_eq!(pin(&active, "run-1").expect("pin").revision, 1);
        assert_eq!(
            pin(&graph(Lifecycle::Draft), "run-1").unwrap_err(),
            GraphError::Invalid("graph_is_not_active".into())
        );
    }

    #[test]
    fn rejects_unknown_edge_and_hash_mutation() {
        let mut invalid = graph(Lifecycle::Active);
        invalid.edges.push(ServiceEdge {
            from: "core".into(),
            to: "missing".into(),
            relation: "depends_on".into(),
        });
        assert!(validate(&invalid).is_err());
        let mut tampered = graph(Lifecycle::Active);
        tampered.scope = "other".into();
        assert_eq!(
            validate(&tampered).unwrap_err(),
            GraphError::Invalid("content_hash_mismatch".into())
        );
    }
}
