//! Детерминированный планировщик статического workflow.
//!
//! Runner намеренно не подключён к crate до отдельного этапа интеграции. Он
//! строит только план: никаких вызовов инструментов, файловых изменений,
//! сетевых запросов или иных side effects здесь нет.

#[cfg(test)]
#[path = "workflow.rs"]
mod workflow;

#[cfg(not(test))]
use crate::workflow::*;
#[cfg(test)]
use workflow::*;

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    NotRequested,
    Pending,
    Approved,
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationDecision {
    NotCancelled,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDecision {
    /// Номер попытки, которую планируется выполнить: первая попытка равна 1.
    pub retry_attempt: u32,
    /// Уже затраченное время, используемое только для детерминированной проверки deadline.
    pub elapsed_ms: u64,
    pub cancellation: CancellationDecision,
    pub approval: ApprovalDecision,
}

impl Default for NodeDecision {
    fn default() -> Self {
        Self {
            retry_attempt: 1,
            elapsed_ms: 0,
            cancellation: CancellationDecision::NotCancelled,
            approval: ApprovalDecision::NotRequested,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepDecision {
    Execute,
    Retry { attempt: u32 },
    AwaitApproval,
    Denied,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedStep {
    pub ordinal: usize,
    pub node_id: String,
    pub dependencies: Vec<String>,
    pub execution: ExecutionPolicy,
    pub decision: StepDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub graph_id: String,
    pub version: u64,
    pub steps: Vec<PlannedStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerError {
    InvalidGraph(Vec<ValidationError>),
    UnknownDecision(String),
    InvalidRetryAttempt {
        node_id: String,
        actual: u32,
        maximum: u32,
    },
}

/// Строит стабильный topological plan. Все узлы выбираются лексикографически
/// при одинаковой степени готовности, поэтому порядок не зависит от входного
/// порядка вектора узлов или рёбер.
pub fn plan_workflow(
    graph: &WorkflowGraph,
    decisions: &BTreeMap<String, NodeDecision>,
) -> Result<ExecutionPlan, RunnerError> {
    graph.validate().map_err(RunnerError::InvalidGraph)?;

    let node_ids: BTreeSet<String> = graph.nodes.iter().map(|node| node.id.clone()).collect();
    if let Some(unknown) = decisions.keys().find(|id| !node_ids.contains(*id)) {
        return Err(RunnerError::UnknownDecision(unknown.clone()));
    }

    let mut nodes = BTreeMap::new();
    for node in &graph.nodes {
        nodes.insert(node.id.clone(), node);
    }

    let mut dependencies = BTreeMap::<String, BTreeSet<String>>::new();
    let mut outgoing = BTreeMap::<String, BTreeSet<String>>::new();
    let mut indegree = BTreeMap::<String, usize>::new();
    for node_id in &node_ids {
        dependencies.insert(node_id.clone(), BTreeSet::new());
        outgoing.insert(node_id.clone(), BTreeSet::new());
        indegree.insert(node_id.clone(), 0);
    }
    for edge in &graph.edges {
        let inserted = dependencies
            .get_mut(&edge.to_node)
            .expect("validated target node")
            .insert(edge.from_node.clone());
        if inserted {
            outgoing
                .get_mut(&edge.from_node)
                .expect("validated source node")
                .insert(edge.to_node.clone());
            *indegree
                .get_mut(&edge.to_node)
                .expect("validated target node") += 1;
        }
    }

    let mut ready = BTreeSet::new();
    for (node_id, degree) in &indegree {
        if *degree == 0 {
            ready.insert(node_id.clone());
        }
    }

    let mut ordered = Vec::with_capacity(graph.nodes.len());
    while let Some(node_id) = ready.pop_first() {
        ordered.push(node_id.clone());
        for next in outgoing.get(&node_id).into_iter().flatten() {
            let degree = indegree.get_mut(next).expect("validated target node");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(next.clone());
            }
        }
    }

    let mut steps = Vec::with_capacity(ordered.len());
    for (ordinal, node_id) in ordered.into_iter().enumerate() {
        let node = nodes.get(&node_id).expect("validated node");
        let decision = decisions.get(&node_id).cloned().unwrap_or_default();
        let step_decision = decide_step(node, &decision)?;
        let dependencies = dependencies
            .get(&node_id)
            .expect("initialized node")
            .iter()
            .cloned()
            .collect();
        steps.push(PlannedStep {
            ordinal,
            node_id,
            dependencies,
            execution: node.execution.clone(),
            decision: step_decision,
        });
    }

    Ok(ExecutionPlan {
        graph_id: graph.graph_id.clone(),
        version: graph.version,
        steps,
    })
}

fn decide_step(node: &WorkflowNode, decision: &NodeDecision) -> Result<StepDecision, RunnerError> {
    let maximum = node.execution.retry.max_attempts;
    if decision.retry_attempt == 0 || decision.retry_attempt > maximum {
        return Err(RunnerError::InvalidRetryAttempt {
            node_id: node.id.clone(),
            actual: decision.retry_attempt,
            maximum,
        });
    }
    if decision.cancellation == CancellationDecision::Cancelled {
        return Ok(StepDecision::Cancelled);
    }
    if decision.elapsed_ms >= node.execution.timeout_ms {
        return Ok(StepDecision::TimedOut);
    }
    if matches!(decision.approval, ApprovalDecision::Denied) {
        return Ok(StepDecision::Denied);
    }
    if node.execution.approval.required {
        match decision.approval {
            ApprovalDecision::Approved => {}
            ApprovalDecision::NotRequested | ApprovalDecision::Pending => {
                return Ok(StepDecision::AwaitApproval)
            }
            ApprovalDecision::Denied => return Ok(StepDecision::Denied),
        }
    }
    if decision.retry_attempt > 1 {
        Ok(StepDecision::Retry {
            attempt: decision.retry_attempt,
        })
    } else {
        Ok(StepDecision::Execute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(required_approval: bool) -> ExecutionPolicy {
        ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts: 3,
                backoff_ms: 10,
                retryable_errors: vec!["transient".into()],
            },
            timeout_ms: 1_000,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: required_approval,
                reason: Some("bounded test".into()),
            },
        }
    }

    fn node(id: &str, required_approval: bool) -> WorkflowNode {
        WorkflowNode {
            id: id.into(),
            node_type: NodeType::Transform,
            inputs: vec![],
            outputs: vec![],
            execution: policy(required_approval),
        }
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraph {
        WorkflowGraph {
            graph_id: "runner-test".into(),
            version: 7,
            entry_node: "a".into(),
            nodes,
            edges,
        }
    }

    fn edge(from: &str, to: &str) -> WorkflowEdge {
        WorkflowEdge {
            from_node: from.into(),
            from_port: "out".into(),
            to_node: to.into(),
            to_port: "in".into(),
        }
    }

    #[test]
    fn topological_order_is_stable_even_when_input_is_reversed() {
        let mut a = node("a", false);
        a.outputs.push(Port {
            name: "out".into(),
            value_type: PortType::Text,
            required: false,
        });
        let mut b = node("b", false);
        b.inputs.push(Port {
            name: "in".into(),
            value_type: PortType::Text,
            required: true,
        });
        let mut c = node("c", false);
        c.inputs.push(Port {
            name: "in".into(),
            value_type: PortType::Text,
            required: true,
        });
        let plan = plan_workflow(
            &graph(vec![c, b, a], vec![edge("a", "b"), edge("a", "c")]),
            &BTreeMap::new(),
        )
        .expect("valid plan");
        assert_eq!(
            plan.steps
                .iter()
                .map(|step| step.node_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
        assert_eq!(plan.steps[1].dependencies, vec!["a"]);
    }

    #[test]
    fn retry_and_approval_decisions_are_planned_without_execution() {
        let mut source = node("a", false);
        source.outputs.push(Port {
            name: "out".into(),
            value_type: PortType::Text,
            required: false,
        });
        let mut approval = node("b", true);
        approval.inputs.push(Port {
            name: "in".into(),
            value_type: PortType::Text,
            required: true,
        });
        let plan = plan_workflow(
            &graph(vec![source, approval], vec![edge("a", "b")]),
            &BTreeMap::from([
                (
                    "a".into(),
                    NodeDecision {
                        retry_attempt: 2,
                        ..Default::default()
                    },
                ),
                (
                    "b".into(),
                    NodeDecision {
                        approval: ApprovalDecision::Pending,
                        ..Default::default()
                    },
                ),
            ]),
        )
        .expect("valid plan");
        assert_eq!(plan.steps[0].decision, StepDecision::Retry { attempt: 2 });
        assert_eq!(plan.steps[1].decision, StepDecision::AwaitApproval);
    }

    #[test]
    fn cancellation_and_timeout_are_safe_terminal_decisions() {
        let mut source = node("a", false);
        source.outputs.push(Port {
            name: "out".into(),
            value_type: PortType::Text,
            required: false,
        });
        let mut timed = node("b", false);
        timed.inputs.push(Port {
            name: "in".into(),
            value_type: PortType::Text,
            required: true,
        });
        let plan = plan_workflow(
            &graph(vec![source, timed], vec![edge("a", "b")]),
            &BTreeMap::from([
                (
                    "a".into(),
                    NodeDecision {
                        cancellation: CancellationDecision::Cancelled,
                        ..Default::default()
                    },
                ),
                (
                    "b".into(),
                    NodeDecision {
                        elapsed_ms: 1_000,
                        ..Default::default()
                    },
                ),
            ]),
        )
        .expect("valid plan");
        assert_eq!(plan.steps[0].decision, StepDecision::Cancelled);
        assert_eq!(plan.steps[1].decision, StepDecision::TimedOut);
    }

    #[test]
    fn unknown_or_out_of_bound_decisions_are_rejected() {
        let unknown = plan_workflow(
            &graph(vec![node("a", false)], vec![]),
            &BTreeMap::from([("missing".into(), NodeDecision::default())]),
        )
        .expect_err("unknown decision");
        assert_eq!(unknown, RunnerError::UnknownDecision("missing".into()));

        let invalid = plan_workflow(
            &graph(vec![node("a", false)], vec![]),
            &BTreeMap::from([(
                "a".into(),
                NodeDecision {
                    retry_attempt: 4,
                    ..Default::default()
                },
            )]),
        )
        .expect_err("attempt bound");
        assert!(matches!(invalid, RunnerError::InvalidRetryAttempt { .. }));
    }

    #[test]
    fn invalid_graph_is_returned_without_partial_plan() {
        let invalid = graph(vec![node("b", false)], vec![]);
        let error = plan_workflow(&invalid, &BTreeMap::new()).expect_err("entry missing");
        assert!(matches!(error, RunnerError::InvalidGraph(_)));
    }
}
