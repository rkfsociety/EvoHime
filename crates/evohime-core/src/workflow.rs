//! Статический typed workflow contract.
//!
//! Модуль экспортируется из crate и используется deterministic evals в
//! `crate::evals`, но пока не подключён к пользовательскому execution path.
//! Контракт не содержит операций редактирования: новая версия графа
//! создаётся целиком, а уже запущенный граф остаётся неизменяемым.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const MAX_GRAPH_NODES: usize = 256;
pub const MAX_GRAPH_EDGES: usize = 512;
pub const MAX_NODE_PORTS: usize = 64;
pub const MAX_TIMEOUT_MS: u64 = 300_000;
pub const MAX_RETRY_ATTEMPTS: u32 = 10;
pub const MAX_LOOP_ITERATIONS: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortType {
    Text,
    Integer,
    Number,
    Boolean,
    Json,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    pub value_type: PortType,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_ms: u64,
    #[serde(default)]
    pub retryable_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationPolicy {
    Cooperative,
    Immediate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    pub required: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    pub retry: RetryPolicy,
    pub timeout_ms: u64,
    pub cancellation: CancellationPolicy,
    pub approval: ApprovalPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeType {
    Research,
    Transform,
    Tool,
    Condition,
    Approval,
    Subgraph { graph_id: String },
    Loop { max_iterations: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub node_type: NodeType,
    #[serde(default)]
    pub inputs: Vec<Port>,
    #[serde(default)]
    pub outputs: Vec<Port>,
    pub execution: ExecutionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowGraph {
    pub graph_id: String,
    pub version: u64,
    pub entry_node: String,
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    EmptyGraphId,
    InvalidVersion,
    EmptyEntryNode,
    TooManyNodes {
        actual: usize,
        maximum: usize,
    },
    TooManyEdges {
        actual: usize,
        maximum: usize,
    },
    EmptyNodeId,
    DuplicateNodeId(String),
    UnknownEntryNode(String),
    UnknownNode(String),
    DuplicatePort {
        node_id: String,
        port: String,
    },
    TooManyPorts {
        node_id: String,
        actual: usize,
        maximum: usize,
    },
    EmptyPortName {
        node_id: String,
    },
    EmptySubgraphId {
        node_id: String,
    },
    InvalidLoopBound {
        node_id: String,
        actual: u32,
        maximum: u32,
    },
    InvalidRetryAttempts {
        node_id: String,
        actual: u32,
        maximum: u32,
    },
    InvalidTimeout {
        node_id: String,
        actual: u64,
        maximum: u64,
    },
    InvalidBackoff {
        node_id: String,
    },
    EmptyRetryableError {
        node_id: String,
    },
    UnknownSourcePort {
        node_id: String,
        port: String,
    },
    UnknownTargetPort {
        node_id: String,
        port: String,
    },
    TypeMismatch {
        edge: WorkflowEdge,
        from: PortType,
        to: PortType,
    },
    RequiredInputUnconnected {
        node_id: String,
        port: String,
    },
    DuplicateInputConnection {
        node_id: String,
        port: String,
    },
    SelfLoop(String),
    Cycle(Vec<String>),
    UnreachableNode(String),
}

impl WorkflowGraph {
    /// Проверяет полный граф в стабильном порядке и возвращает ошибки в том же порядке.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        let mut nodes = BTreeMap::new();

        if self.graph_id.trim().is_empty() {
            errors.push(ValidationError::EmptyGraphId);
        }
        if self.version == 0 {
            errors.push(ValidationError::InvalidVersion);
        }
        if self.entry_node.trim().is_empty() {
            errors.push(ValidationError::EmptyEntryNode);
        }
        if self.nodes.len() > MAX_GRAPH_NODES {
            errors.push(ValidationError::TooManyNodes {
                actual: self.nodes.len(),
                maximum: MAX_GRAPH_NODES,
            });
        }
        if self.edges.len() > MAX_GRAPH_EDGES {
            errors.push(ValidationError::TooManyEdges {
                actual: self.edges.len(),
                maximum: MAX_GRAPH_EDGES,
            });
        }

        for node in &self.nodes {
            if node.id.trim().is_empty() {
                errors.push(ValidationError::EmptyNodeId);
                continue;
            }
            if nodes.insert(node.id.clone(), node).is_some() {
                errors.push(ValidationError::DuplicateNodeId(node.id.clone()));
            }
        }

        if !self.entry_node.trim().is_empty() && !nodes.contains_key(&self.entry_node) {
            errors.push(ValidationError::UnknownEntryNode(self.entry_node.clone()));
        }

        for node in nodes.values() {
            validate_node(node, &mut errors);
        }

        let mut incoming = BTreeMap::<(String, String), usize>::new();
        let mut adjacency = BTreeMap::<String, BTreeSet<String>>::new();
        for edge in &self.edges {
            let Some(source) = nodes.get(&edge.from_node) else {
                errors.push(ValidationError::UnknownNode(edge.from_node.clone()));
                continue;
            };
            let Some(target) = nodes.get(&edge.to_node) else {
                errors.push(ValidationError::UnknownNode(edge.to_node.clone()));
                continue;
            };
            let Some(from) = source
                .outputs
                .iter()
                .find(|port| port.name == edge.from_port)
            else {
                errors.push(ValidationError::UnknownSourcePort {
                    node_id: edge.from_node.clone(),
                    port: edge.from_port.clone(),
                });
                continue;
            };
            let Some(to) = target.inputs.iter().find(|port| port.name == edge.to_port) else {
                errors.push(ValidationError::UnknownTargetPort {
                    node_id: edge.to_node.clone(),
                    port: edge.to_port.clone(),
                });
                continue;
            };
            if from.value_type != to.value_type {
                errors.push(ValidationError::TypeMismatch {
                    edge: edge.clone(),
                    from: from.value_type.clone(),
                    to: to.value_type.clone(),
                });
            }
            let key = (edge.to_node.clone(), edge.to_port.clone());
            let count = incoming.entry(key).or_default();
            *count += 1;
            if *count > 1 {
                errors.push(ValidationError::DuplicateInputConnection {
                    node_id: edge.to_node.clone(),
                    port: edge.to_port.clone(),
                });
            }
            if edge.from_node == edge.to_node {
                errors.push(ValidationError::SelfLoop(edge.from_node.clone()));
            }
            adjacency
                .entry(edge.from_node.clone())
                .or_default()
                .insert(edge.to_node.clone());
        }

        for node in nodes.values() {
            for port in node.inputs.iter().filter(|port| port.required) {
                if !incoming.contains_key(&(node.id.clone(), port.name.clone())) {
                    errors.push(ValidationError::RequiredInputUnconnected {
                        node_id: node.id.clone(),
                        port: port.name.clone(),
                    });
                }
            }
        }

        let cycle = find_cycle(&nodes, &adjacency);
        if let Some(cycle) = cycle {
            errors.push(ValidationError::Cycle(cycle));
        }
        for node_id in reachable_nodes(&self.entry_node, &adjacency) {
            if !nodes.contains_key(&node_id) {
                continue;
            }
        }
        let reachable = reachable_nodes(&self.entry_node, &adjacency);
        for node_id in nodes.keys() {
            if !reachable.contains(node_id) {
                errors.push(ValidationError::UnreachableNode(node_id.clone()));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn validate_node(node: &WorkflowNode, errors: &mut Vec<ValidationError>) {
    let port_count = node.inputs.len() + node.outputs.len();
    if port_count > MAX_NODE_PORTS {
        errors.push(ValidationError::TooManyPorts {
            node_id: node.id.clone(),
            actual: port_count,
            maximum: MAX_NODE_PORTS,
        });
    }
    let mut names = BTreeSet::new();
    for port in node.inputs.iter().chain(node.outputs.iter()) {
        if port.name.trim().is_empty() {
            errors.push(ValidationError::EmptyPortName {
                node_id: node.id.clone(),
            });
        } else if !names.insert(port.name.clone()) {
            errors.push(ValidationError::DuplicatePort {
                node_id: node.id.clone(),
                port: port.name.clone(),
            });
        }
    }
    if let NodeType::Subgraph { graph_id } = &node.node_type {
        if graph_id.trim().is_empty() {
            errors.push(ValidationError::EmptySubgraphId {
                node_id: node.id.clone(),
            });
        }
    }
    if let NodeType::Loop { max_iterations } = node.node_type {
        if max_iterations == 0 || max_iterations > MAX_LOOP_ITERATIONS {
            errors.push(ValidationError::InvalidLoopBound {
                node_id: node.id.clone(),
                actual: max_iterations,
                maximum: MAX_LOOP_ITERATIONS,
            });
        }
    }
    let retry = &node.execution.retry;
    if retry.max_attempts == 0 || retry.max_attempts > MAX_RETRY_ATTEMPTS {
        errors.push(ValidationError::InvalidRetryAttempts {
            node_id: node.id.clone(),
            actual: retry.max_attempts,
            maximum: MAX_RETRY_ATTEMPTS,
        });
    }
    if retry.max_attempts > 1 && retry.backoff_ms == 0 {
        errors.push(ValidationError::InvalidBackoff {
            node_id: node.id.clone(),
        });
    }
    for error in &retry.retryable_errors {
        if error.trim().is_empty() {
            errors.push(ValidationError::EmptyRetryableError {
                node_id: node.id.clone(),
            });
        }
    }
    if node.execution.timeout_ms == 0 || node.execution.timeout_ms > MAX_TIMEOUT_MS {
        errors.push(ValidationError::InvalidTimeout {
            node_id: node.id.clone(),
            actual: node.execution.timeout_ms,
            maximum: MAX_TIMEOUT_MS,
        });
    }
}

fn reachable_nodes(
    entry: &str,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([entry.to_string()]);
    while let Some(node) = queue.pop_front() {
        if !seen.insert(node.clone()) {
            continue;
        }
        if let Some(next) = adjacency.get(&node) {
            queue.extend(next.iter().cloned());
        }
    }
    seen
}

fn find_cycle(
    nodes: &BTreeMap<String, &WorkflowNode>,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> Option<Vec<String>> {
    fn visit(
        node: &str,
        adjacency: &BTreeMap<String, BTreeSet<String>>,
        colors: &mut BTreeMap<String, u8>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        colors.insert(node.to_string(), 1);
        stack.push(node.to_string());
        for next in adjacency.get(node).into_iter().flatten() {
            match colors.get(next).copied().unwrap_or(0) {
                0 => {
                    if let Some(cycle) = visit(next, adjacency, colors, stack) {
                        return Some(cycle);
                    }
                }
                1 => {
                    let start = stack.iter().position(|item| item == next).unwrap_or(0);
                    let mut cycle = stack[start..].to_vec();
                    cycle.push(next.clone());
                    return Some(cycle);
                }
                _ => {}
            }
        }
        stack.pop();
        colors.insert(node.to_string(), 2);
        None
    }
    let mut colors = BTreeMap::new();
    for node in nodes.keys() {
        if colors.get(node).copied().unwrap_or(0) == 0 {
            if let Some(cycle) = visit(node, adjacency, &mut colors, &mut Vec::new()) {
                return Some(cycle);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ExecutionPolicy {
        ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts: 2,
                backoff_ms: 10,
                retryable_errors: vec!["transient".into()],
            },
            timeout_ms: 1_000,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: false,
                reason: None,
            },
        }
    }

    fn node(
        id: &str,
        input: Option<(&str, PortType)>,
        output: Option<(&str, PortType)>,
    ) -> WorkflowNode {
        WorkflowNode {
            id: id.into(),
            node_type: NodeType::Transform,
            inputs: input
                .into_iter()
                .map(|(name, value_type)| Port {
                    name: name.into(),
                    value_type,
                    required: true,
                })
                .collect(),
            outputs: output
                .into_iter()
                .map(|(name, value_type)| Port {
                    name: name.into(),
                    value_type,
                    required: false,
                })
                .collect(),
            execution: policy(),
        }
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraph {
        WorkflowGraph {
            graph_id: "graph-1".into(),
            version: 1,
            entry_node: "source".into(),
            nodes,
            edges,
        }
    }

    #[test]
    fn accepts_a_typed_bounded_static_graph() {
        let result = graph(
            vec![
                node("source", None, Some(("text", PortType::Text))),
                node("sink", Some(("text", PortType::Text)), None),
            ],
            vec![WorkflowEdge {
                from_node: "source".into(),
                from_port: "text".into(),
                to_node: "sink".into(),
                to_port: "text".into(),
            }],
        )
        .validate();
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn rejects_type_mismatch_and_missing_required_input() {
        let errors = graph(
            vec![
                node("source", None, Some(("value", PortType::Integer))),
                node("sink", Some(("text", PortType::Text)), None),
            ],
            vec![WorkflowEdge {
                from_node: "source".into(),
                from_port: "value".into(),
                to_node: "sink".into(),
                to_port: "text".into(),
            }],
        )
        .validate()
        .expect_err("invalid types");
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::TypeMismatch { .. })));
        assert!(!errors
            .iter()
            .any(|error| matches!(error, ValidationError::RequiredInputUnconnected { .. })));
    }

    #[test]
    fn rejects_cycles_and_unreachable_nodes_deterministically() {
        let errors = graph(
            vec![
                node(
                    "source",
                    Some(("in", PortType::Text)),
                    Some(("out", PortType::Text)),
                ),
                node("orphan", None, None),
            ],
            vec![WorkflowEdge {
                from_node: "source".into(),
                from_port: "out".into(),
                to_node: "source".into(),
                to_port: "in".into(),
            }],
        )
        .validate()
        .expect_err("cycle");
        assert!(matches!(errors[0], ValidationError::SelfLoop(_)));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::Cycle(_))));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::UnreachableNode(id) if id == "orphan")));
    }

    #[test]
    fn rejects_unbounded_execution_controls() {
        let mut invalid = node("source", None, None);
        invalid.execution.retry.max_attempts = MAX_RETRY_ATTEMPTS + 1;
        invalid.execution.timeout_ms = MAX_TIMEOUT_MS + 1;
        invalid.node_type = NodeType::Loop {
            max_iterations: MAX_LOOP_ITERATIONS + 1,
        };
        let errors = graph(vec![invalid], vec![]).validate().expect_err("bounds");
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::InvalidRetryAttempts { .. })));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::InvalidTimeout { .. })));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::InvalidLoopBound { .. })));
    }

    #[test]
    fn serializes_typed_nodes_with_execution_contract() {
        let workflow = graph(
            vec![WorkflowNode {
                id: "approval".into(),
                node_type: NodeType::Approval,
                inputs: vec![],
                outputs: vec![Port {
                    name: "approved".into(),
                    value_type: PortType::Boolean,
                    required: false,
                }],
                execution: ExecutionPolicy {
                    approval: ApprovalPolicy {
                        required: true,
                        reason: Some("external mutation".into()),
                    },
                    ..policy()
                },
            }],
            vec![],
        );
        let json = serde_json::to_value(&workflow).expect("serializes");
        assert_eq!(json["nodes"][0]["node_type"]["type"], "approval");
        assert_eq!(json["nodes"][0]["execution"]["timeout_ms"], 1000);
        let round_trip: WorkflowGraph = serde_json::from_value(json).expect("deserializes");
        assert_eq!(round_trip, workflow);
    }
}
