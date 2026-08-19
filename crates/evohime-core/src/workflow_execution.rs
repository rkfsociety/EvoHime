//! Side-effectful runner over the deterministic workflow contract.
//!
//! `workflow_runner::plan_workflow` is pure: it decides topological order,
//! retry/timeout/cancellation/approval outcomes from a caller-supplied
//! snapshot, and performs no I/O. This module is the wiring layer: it walks
//! the plan in order and actually executes each node's work through an
//! injected `NodeExecutor`, applying the same retry/timeout/backoff rules
//! for real (via `tokio::time::timeout` and `tokio::time::sleep`) and
//! checking a live `ApprovalGate`/`CancellationSource` before letting a node
//! run, exactly the "wire the pure contract to real IO" pattern used by
//! `observability.rs` for `core.jsonl`.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::workflow_runner::{
    plan_workflow, ApprovalDecision, ExecutionPlan, NodeDecision, PlanGraph as WorkflowGraph,
    PlanNode as WorkflowNode, RunnerError,
};

pub type ExecFuture<'a> = Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;

/// Performs the real work for one node. Implementations dispatch to the
/// actual research pipeline / tool-runtime call named by the node.
pub trait NodeExecutor: Sync {
    fn execute<'a>(&'a self, node_id: &'a str) -> ExecFuture<'a>;
}

/// Live approval status for a node that requires approval. Implementations
/// may query a database, an IPC channel, or an in-memory store; this trait
/// is the real gate applied before execution, distinct from the
/// pre-baked `ApprovalDecision` the pure planner accepts as a snapshot.
pub trait ApprovalGate: Sync {
    fn decide(&self, node_id: &str) -> ApprovalDecision;
}

/// Live cancellation signal, checked before every node.
pub trait CancellationSource: Sync {
    fn is_cancelled(&self) -> bool;
}

pub struct AlwaysApproved;
impl ApprovalGate for AlwaysApproved {
    fn decide(&self, _node_id: &str) -> ApprovalDecision {
        ApprovalDecision::Approved
    }
}

pub struct NeverCancelled;
impl CancellationSource for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeOutcome {
    Succeeded {
        attempts: u32,
        output: Value,
    },
    Failed {
        attempts: u32,
        message: String,
    },
    TimedOut {
        attempts: u32,
    },
    Cancelled,
    Denied,
    AwaitApproval,
    /// Skipped because a dependency did not succeed.
    Blocked {
        blocking_dependency: String,
    },
}

impl NodeOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowRunOutcome {
    pub graph_id: String,
    pub version: u64,
    pub results: BTreeMap<String, NodeOutcome>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunError {
    Planning(RunnerError),
}

/// Executes `graph` node by node in the plan's topological order. For every
/// node:
/// 1. If cancellation is signalled, the node and everything after it are
///    marked `Cancelled` without being executed.
/// 2. If any dependency did not succeed, the node is `Blocked` and skipped.
/// 3. If the node requires approval, `approval_gate` is consulted for real;
///    `Denied` or a non-approved answer stops that branch without running
///    the node.
/// 4. Otherwise the node runs through `executor`, retried up to
///    `retry.max_attempts` times (sleeping `backoff_ms * attempt` between
///    attempts) and bounded by `timeout_ms` per attempt via a real
///    `tokio::time::timeout`. Only errors whose message contains one of the
///    node's `retryable_errors` substrings are retried; anything else fails
///    immediately.
pub async fn run_workflow(
    graph: &WorkflowGraph,
    executor: &dyn NodeExecutor,
    approval_gate: &dyn ApprovalGate,
    cancellation: &dyn CancellationSource,
) -> Result<WorkflowRunOutcome, RunError> {
    // The pure planner only needs a snapshot to compute ordering and
    // dependency edges; approval/retry/cancellation are re-decided live
    // below, so a neutral snapshot (attempt 1, no approval yet, not
    // cancelled) is enough to obtain the deterministic topological plan.
    let snapshot: BTreeMap<String, NodeDecision> = BTreeMap::new();
    let plan: ExecutionPlan = plan_workflow(graph, &snapshot).map_err(RunError::Planning)?;

    let nodes: BTreeMap<&str, &WorkflowNode> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();

    let mut results: BTreeMap<String, NodeOutcome> = BTreeMap::new();
    let mut run_cancelled = false;

    for step in &plan.steps {
        let node = nodes
            .get(step.node_id.as_str())
            .expect("planned step references a graph node");

        if run_cancelled || cancellation.is_cancelled() {
            run_cancelled = true;
            results.insert(step.node_id.clone(), NodeOutcome::Cancelled);
            continue;
        }

        if let Some(blocking) = step.dependencies.iter().find(
            |dependency| !matches!(results.get(*dependency), Some(outcome) if outcome.is_success()),
        ) {
            results.insert(
                step.node_id.clone(),
                NodeOutcome::Blocked {
                    blocking_dependency: blocking.clone(),
                },
            );
            continue;
        }

        if node.execution.approval.required {
            match approval_gate.decide(&step.node_id) {
                ApprovalDecision::Approved => {}
                ApprovalDecision::Denied => {
                    results.insert(step.node_id.clone(), NodeOutcome::Denied);
                    continue;
                }
                ApprovalDecision::NotRequested | ApprovalDecision::Pending => {
                    results.insert(step.node_id.clone(), NodeOutcome::AwaitApproval);
                    continue;
                }
            }
        }

        let max_attempts = node.execution.retry.max_attempts.max(1);
        let timeout = Duration::from_millis(node.execution.timeout_ms.max(1));
        let mut last_error: Option<String> = None;
        let mut timed_out = false;
        let mut succeeded: Option<Value> = None;
        let mut attempts_used = 0u32;

        for attempt in 1..=max_attempts {
            attempts_used = attempt;
            if cancellation.is_cancelled() {
                run_cancelled = true;
                break;
            }
            let started = Instant::now();
            match tokio::time::timeout(timeout, executor.execute(&step.node_id)).await {
                Ok(Ok(value)) => {
                    succeeded = Some(value);
                    break;
                }
                Ok(Err(message)) => {
                    let retryable = node
                        .execution
                        .retry
                        .retryable_errors
                        .iter()
                        .any(|pattern| message.contains(pattern.as_str()));
                    last_error = Some(message);
                    timed_out = false;
                    if !retryable || attempt == max_attempts {
                        break;
                    }
                    let elapsed = started.elapsed();
                    let backoff = Duration::from_millis(
                        node.execution
                            .retry
                            .backoff_ms
                            .saturating_mul(attempt as u64),
                    );
                    if backoff > elapsed {
                        tokio::time::sleep(backoff - elapsed).await;
                    }
                }
                Err(_) => {
                    timed_out = true;
                    last_error = None;
                    break;
                }
            }
        }

        if run_cancelled {
            results.insert(step.node_id.clone(), NodeOutcome::Cancelled);
            continue;
        }

        let outcome = if let Some(output) = succeeded {
            NodeOutcome::Succeeded {
                attempts: attempts_used,
                output,
            }
        } else if timed_out {
            NodeOutcome::TimedOut {
                attempts: attempts_used,
            }
        } else {
            NodeOutcome::Failed {
                attempts: attempts_used,
                message: last_error.unwrap_or_else(|| "node executor produced no result".into()),
            }
        };
        results.insert(step.node_id.clone(), outcome);
    }

    Ok(WorkflowRunOutcome {
        graph_id: plan.graph_id,
        version: plan.version,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{
        ApprovalPolicy, CancellationPolicy, ExecutionPolicy, NodeType, Port, PortType, RetryPolicy,
        WorkflowEdge,
    };
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    fn policy(
        max_attempts: u32,
        retryable: Vec<&str>,
        timeout_ms: u64,
        approval: bool,
    ) -> ExecutionPolicy {
        ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts,
                backoff_ms: 1,
                retryable_errors: retryable.into_iter().map(String::from).collect(),
            },
            timeout_ms,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: approval,
                reason: None,
            },
        }
    }

    fn node(id: &str, execution: ExecutionPolicy) -> WorkflowNode {
        WorkflowNode {
            id: id.into(),
            node_type: NodeType::Tool,
            inputs: vec![],
            outputs: vec![Port {
                name: "out".into(),
                value_type: PortType::Text,
                required: false,
            }],
            execution,
        }
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraph {
        WorkflowGraph {
            graph_id: "exec-test".into(),
            version: 1,
            entry_node: "a".into(),
            nodes,
            edges,
        }
    }

    struct ScriptedExecutor {
        calls: Mutex<Vec<String>>,
        attempt_counter: AtomicU32,
        fail_first_n: u32,
    }

    impl NodeExecutor for ScriptedExecutor {
        fn execute<'a>(&'a self, node_id: &'a str) -> ExecFuture<'a> {
            self.calls.lock().unwrap().push(node_id.to_string());
            let attempt = self.attempt_counter.fetch_add(1, Ordering::SeqCst) + 1;
            let fail_first_n = self.fail_first_n;
            Box::pin(async move {
                if attempt <= fail_first_n {
                    Err("transient failure".to_string())
                } else {
                    Ok(Value::String("ok".into()))
                }
            })
        }
    }

    struct SlowExecutor;
    impl NodeExecutor for SlowExecutor {
        fn execute<'a>(&'a self, _node_id: &'a str) -> ExecFuture<'a> {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(Value::Null)
            })
        }
    }

    struct DenyingGate;
    impl ApprovalGate for DenyingGate {
        fn decide(&self, _node_id: &str) -> ApprovalDecision {
            ApprovalDecision::Denied
        }
    }

    struct AlwaysCancelled;
    impl CancellationSource for AlwaysCancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn executes_success_path_in_dependency_order() {
        let a = node("a", policy(1, vec![], 1_000, false));
        let mut b = node("b", policy(1, vec![], 1_000, false));
        b.inputs.push(Port {
            name: "in".into(),
            value_type: PortType::Text,
            required: true,
        });
        let g = graph(
            vec![a, b],
            vec![WorkflowEdge {
                from_node: "a".into(),
                from_port: "out".into(),
                to_node: "b".into(),
                to_port: "in".into(),
            }],
        );
        let executor = ScriptedExecutor {
            calls: Mutex::new(vec![]),
            attempt_counter: AtomicU32::new(0),
            fail_first_n: 0,
        };
        let outcome = run_workflow(&g, &executor, &AlwaysApproved, &NeverCancelled)
            .await
            .expect("run succeeds");
        assert!(outcome.results["a"].is_success());
        assert!(outcome.results["b"].is_success());
        assert_eq!(*executor.calls.lock().unwrap(), vec!["a", "b"]);
    }

    #[tokio::test]
    async fn retries_transient_failures_up_to_the_limit_then_succeeds() {
        let n = node("a", policy(3, vec!["transient"], 1_000, false));
        let g = graph(vec![n], vec![]);
        let executor = ScriptedExecutor {
            calls: Mutex::new(vec![]),
            attempt_counter: AtomicU32::new(0),
            fail_first_n: 2,
        };
        let outcome = run_workflow(&g, &executor, &AlwaysApproved, &NeverCancelled)
            .await
            .unwrap();
        match &outcome.results["a"] {
            NodeOutcome::Succeeded { attempts, .. } => assert_eq!(*attempts, 3),
            other => panic!("expected success after retries, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn non_retryable_error_fails_immediately_without_retry() {
        let n = node("a", policy(5, vec!["only-this-is-retryable"], 1_000, false));
        let g = graph(vec![n], vec![]);
        let executor = ScriptedExecutor {
            calls: Mutex::new(vec![]),
            attempt_counter: AtomicU32::new(0),
            fail_first_n: 5,
        };
        let outcome = run_workflow(&g, &executor, &AlwaysApproved, &NeverCancelled)
            .await
            .unwrap();
        match &outcome.results["a"] {
            NodeOutcome::Failed { attempts, .. } => assert_eq!(*attempts, 1),
            other => panic!("expected immediate failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_timing_out_node_is_marked_timed_out_and_blocks_dependents() {
        let a = node("a", policy(1, vec![], 10, false));
        let mut b = node("b", policy(1, vec![], 1_000, false));
        b.inputs.push(Port {
            name: "in".into(),
            value_type: PortType::Text,
            required: true,
        });
        let g = graph(
            vec![a, b],
            vec![WorkflowEdge {
                from_node: "a".into(),
                from_port: "out".into(),
                to_node: "b".into(),
                to_port: "in".into(),
            }],
        );
        let outcome = run_workflow(&g, &SlowExecutor, &AlwaysApproved, &NeverCancelled)
            .await
            .unwrap();
        assert!(matches!(outcome.results["a"], NodeOutcome::TimedOut { .. }));
        assert!(matches!(outcome.results["b"], NodeOutcome::Blocked { .. }));
    }

    #[tokio::test]
    async fn denied_approval_stops_the_node_without_executing() {
        let n = node("a", policy(1, vec![], 1_000, true));
        let g = graph(vec![n], vec![]);
        let executor = ScriptedExecutor {
            calls: Mutex::new(vec![]),
            attempt_counter: AtomicU32::new(0),
            fail_first_n: 0,
        };
        let outcome = run_workflow(&g, &executor, &DenyingGate, &NeverCancelled)
            .await
            .unwrap();
        assert_eq!(outcome.results["a"], NodeOutcome::Denied);
        assert!(executor.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn live_cancellation_stops_the_run_without_executing_any_node() {
        let n = node("a", policy(1, vec![], 1_000, false));
        let g = graph(vec![n], vec![]);
        let executor = ScriptedExecutor {
            calls: Mutex::new(vec![]),
            attempt_counter: AtomicU32::new(0),
            fail_first_n: 0,
        };
        let outcome = run_workflow(&g, &executor, &AlwaysApproved, &AlwaysCancelled)
            .await
            .unwrap();
        assert_eq!(outcome.results["a"], NodeOutcome::Cancelled);
        assert!(executor.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn deterministic_replay_produces_identical_ordering_and_results() {
        let a = node("a", policy(1, vec![], 1_000, false));
        let mut b = node("b", policy(1, vec![], 1_000, false));
        let mut c = node("c", policy(1, vec![], 1_000, false));
        b.inputs.push(Port {
            name: "in".into(),
            value_type: PortType::Text,
            required: true,
        });
        c.inputs.push(Port {
            name: "in".into(),
            value_type: PortType::Text,
            required: true,
        });
        let g = graph(
            vec![c, b, a],
            vec![
                WorkflowEdge {
                    from_node: "a".into(),
                    from_port: "out".into(),
                    to_node: "b".into(),
                    to_port: "in".into(),
                },
                WorkflowEdge {
                    from_node: "a".into(),
                    from_port: "out".into(),
                    to_node: "c".into(),
                    to_port: "in".into(),
                },
            ],
        );
        let run = || async {
            let executor = ScriptedExecutor {
                calls: Mutex::new(vec![]),
                attempt_counter: AtomicU32::new(0),
                fail_first_n: 0,
            };
            let outcome = run_workflow(&g, &executor, &AlwaysApproved, &NeverCancelled)
                .await
                .unwrap();
            (executor.calls.into_inner().unwrap(), outcome)
        };
        let (calls_1, outcome_1) = run().await;
        let (calls_2, outcome_2) = run().await;
        assert_eq!(calls_1, calls_2);
        assert_eq!(outcome_1, outcome_2);
    }
}
