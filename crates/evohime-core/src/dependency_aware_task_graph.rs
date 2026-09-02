//! Core-owned dependency-aware execution graph (plan 73).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_TASKS: usize = 1024;
pub const MAX_EDGES: usize = 4096;
pub const MAX_ID: usize = 128;
pub const MAX_TEXT: usize = 16 * 1024;
pub const MAX_REFS: usize = 256;
pub const MAX_PARALLELISM: usize = 16;
pub const MAX_PATCH_OPS: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskStatus {
    Pending,
    Ready,
    Running,
    Waiting,
    Completed,
    Failed,
    Blocked,
    NeedsRevision,
    Invalidated,
    Skipped,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDependency {
    pub from_id: String,
    pub to_id: String,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskEvidence {
    pub reference: String,
    pub content_hash: String,
    pub revision: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionTask {
    pub id: String,
    pub title: String,
    pub instruction: String,
    pub status: TaskStatus,
    pub revision: u64,
    pub contract_hash: String,
    pub role: String,
    pub grants: Vec<String>,
    pub dependencies: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub evidence: Vec<TaskEvidence>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskGraph {
    pub schema_version: u32,
    pub graph_id: String,
    pub revision: u64,
    pub tasks: BTreeMap<String, ExecutionTask>,
    pub edges: Vec<TaskDependency>,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatchOp {
    AddTask(ExecutionTask),
    ReviseTask(ExecutionTask),
    RemovePendingTask(String),
    AddDependency(TaskDependency),
    RemoveDependency(TaskDependency),
    ReassignTask {
        task_id: String,
        role: String,
        grants: Vec<String>,
    },
    ChangeAcceptanceCriteria {
        task_id: String,
        criteria: Vec<String>,
    },
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GraphError {
    #[error("invalid graph: {0}")]
    Invalid(String),
    #[error("unknown task reference: {0}")]
    UnknownTask(String),
    #[error("dependency cycle")]
    Cycle,
    #[error("stale graph revision")]
    StaleRevision,
    #[error("completed task is immutable")]
    CompletedImmutable,
    #[error("stale evidence")]
    StaleEvidence,
    #[error("grant exceeds caller ceiling")]
    GrantsCeiling,
    #[error("patch is not atomic")]
    PatchRejected,
}
fn bounded(s: &str, limit: usize) -> Result<(), GraphError> {
    if s.is_empty() || s.len() > limit || s.chars().any(char::is_control) {
        Err(GraphError::Invalid("bounded text".into()))
    } else {
        Ok(())
    }
}
pub fn hash<T: Serialize>(v: &T) -> String {
    let bytes = serde_json::to_vec(v).unwrap_or_default();
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}
pub fn validate(graph: &TaskGraph, grants_ceiling: &[String]) -> Result<(), GraphError> {
    if graph.schema_version != SCHEMA_VERSION
        || graph.tasks.len() > MAX_TASKS
        || graph.edges.len() > MAX_EDGES
    {
        return Err(GraphError::Invalid("schema or graph bounds".into()));
    }
    bounded(&graph.graph_id, MAX_ID)?;
    for (id, task) in &graph.tasks {
        bounded(id, MAX_ID)?;
        bounded(&task.title, MAX_TEXT)?;
        bounded(&task.instruction, MAX_TEXT)?;
        bounded(&task.role, MAX_ID)?;
        if task.acceptance_criteria.len() > MAX_REFS || task.evidence.len() > MAX_REFS {
            return Err(GraphError::Invalid("task references bound".into()));
        }
        if task
            .grants
            .iter()
            .any(|g| !grants_ceiling.iter().any(|x| x == g))
        {
            return Err(GraphError::GrantsCeiling);
        }
        if task.id != *id {
            return Err(GraphError::Invalid("task id mismatch".into()));
        }
        for evidence in &task.evidence {
            bounded(&evidence.reference, MAX_ID)?;
            if !evidence.content_hash.starts_with("sha256:")
                || evidence.content_hash.len() != 71
                || evidence.revision == 0
            {
                return Err(GraphError::StaleEvidence);
            }
        }
    }
    for edge in &graph.edges {
        if !graph.tasks.contains_key(&edge.from_id) {
            return Err(GraphError::UnknownTask(edge.from_id.clone()));
        }
        if !graph.tasks.contains_key(&edge.to_id) {
            return Err(GraphError::UnknownTask(edge.to_id.clone()));
        }
    }
    let mut incoming: BTreeMap<&str, usize> = graph.tasks.keys().map(|k| (k.as_str(), 0)).collect();
    for e in &graph.edges {
        *incoming.get_mut(e.to_id.as_str()).unwrap() += 1;
    }
    let mut q: VecDeque<&str> = incoming
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut seen = 0;
    while let Some(id) = q.pop_front() {
        seen += 1;
        for e in graph.edges.iter().filter(|e| e.from_id == id) {
            let n = incoming.get_mut(e.to_id.as_str()).unwrap();
            *n -= 1;
            if *n == 0 {
                q.push_back(e.to_id.as_str());
            }
        }
    }
    if seen != graph.tasks.len() {
        return Err(GraphError::Cycle);
    }
    Ok(())
}
pub fn ready_set(graph: &TaskGraph) -> Vec<String> {
    let done: BTreeSet<_> = graph
        .tasks
        .values()
        .filter(|t| t.status == TaskStatus::Completed)
        .map(|t| t.id.as_str())
        .collect();
    graph
        .tasks
        .values()
        .filter(|t| {
            matches!(t.status, TaskStatus::Pending | TaskStatus::Ready)
                && graph
                    .edges
                    .iter()
                    .filter(|e| e.to_id == t.id)
                    .all(|e| done.contains(e.from_id.as_str()))
        })
        .map(|t| t.id.clone())
        .collect()
}
pub fn apply_patch(
    mut graph: TaskGraph,
    ops: &[PatchOp],
    expected_revision: u64,
    grants: &[String],
) -> Result<TaskGraph, GraphError> {
    if graph.revision != expected_revision || ops.len() > MAX_PATCH_OPS {
        return Err(GraphError::StaleRevision);
    }
    let old = graph.clone();
    for op in ops {
        match op {
            PatchOp::AddTask(t) => {
                if graph.tasks.contains_key(&t.id) {
                    return Err(GraphError::PatchRejected);
                }
                graph.tasks.insert(t.id.clone(), t.clone());
            }
            PatchOp::ReviseTask(t) => {
                let old_t = graph
                    .tasks
                    .get(&t.id)
                    .ok_or_else(|| GraphError::UnknownTask(t.id.clone()))?;
                if old_t.status == TaskStatus::Completed {
                    return Err(GraphError::CompletedImmutable);
                }
                graph.tasks.insert(t.id.clone(), t.clone());
            }
            PatchOp::RemovePendingTask(id) => {
                if graph
                    .tasks
                    .get(id)
                    .map(|t| t.status != TaskStatus::Pending)
                    .unwrap_or(true)
                {
                    return Err(GraphError::PatchRejected);
                }
                graph.tasks.remove(id);
                graph.edges.retain(|e| &e.from_id != id && &e.to_id != id);
            }
            PatchOp::AddDependency(e) => {
                graph.edges.push(e.clone());
            }
            PatchOp::RemoveDependency(e) => graph.edges.retain(|x| x != e),
            PatchOp::ReassignTask {
                task_id,
                role,
                grants: gs,
            } => {
                let t = graph
                    .tasks
                    .get_mut(task_id)
                    .ok_or_else(|| GraphError::UnknownTask(task_id.clone()))?;
                if t.status == TaskStatus::Completed {
                    return Err(GraphError::CompletedImmutable);
                }
                t.role = role.clone();
                t.grants = gs.clone();
                t.revision += 1;
            }
            PatchOp::ChangeAcceptanceCriteria { task_id, criteria } => {
                let t = graph
                    .tasks
                    .get_mut(task_id)
                    .ok_or_else(|| GraphError::UnknownTask(task_id.clone()))?;
                if t.status == TaskStatus::Completed {
                    return Err(GraphError::CompletedImmutable);
                }
                t.acceptance_criteria = criteria.clone();
                t.revision += 1;
            }
        }
    }
    validate(&graph, grants)?;
    let changed: BTreeSet<String> = ops
        .iter()
        .filter_map(|op| match op {
            PatchOp::ReviseTask(t) => Some(t.id.clone()),
            PatchOp::ReassignTask { task_id, .. }
            | PatchOp::ChangeAcceptanceCriteria { task_id, .. } => Some(task_id.clone()),
            _ => None,
        })
        .collect();
    let mut queue: VecDeque<String> = changed.into_iter().collect();
    let mut invalidated = BTreeSet::new();
    while let Some(id) = queue.pop_front() {
        for e in graph.edges.iter().filter(|e| e.from_id == id) {
            if invalidated.insert(e.to_id.clone()) {
                if let Some(t) = graph.tasks.get_mut(&e.to_id) {
                    if t.status != TaskStatus::Completed {
                        t.status = TaskStatus::Invalidated;
                    }
                }
                queue.push_back(e.to_id.clone());
            }
        }
    }
    graph.revision += 1;
    graph.content_hash = hash(&graph);
    if old
        .tasks
        .iter()
        .filter(|(_, t)| t.status == TaskStatus::Completed)
        .any(|(id, t)| {
            graph
                .tasks
                .get(id)
                .map(|n| n.contract_hash != t.contract_hash)
                .unwrap_or(true)
        })
    {
        return Err(GraphError::CompletedImmutable);
    }
    Ok(graph)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn graph() -> TaskGraph {
        let t = |id: &str| ExecutionTask {
            id: id.into(),
            title: id.into(),
            instruction: "do".into(),
            status: TaskStatus::Pending,
            revision: 1,
            contract_hash: "sha256:x".into(),
            role: "worker".into(),
            grants: vec![],
            dependencies: vec![],
            acceptance_criteria: vec!["ok".into()],
            evidence: vec![],
        };
        let mut tasks = BTreeMap::new();
        tasks.insert("a".into(), t("a"));
        tasks.insert("b".into(), t("b"));
        TaskGraph {
            schema_version: 1,
            graph_id: "g".into(),
            revision: 1,
            tasks,
            edges: vec![TaskDependency {
                from_id: "a".into(),
                to_id: "b".into(),
                kind: "blocks".into(),
            }],
            content_hash: String::new(),
        }
    }
    #[test]
    fn cycle_and_ready_are_deterministic() {
        let mut g = graph();
        assert_eq!(ready_set(&g), vec!["a"]);
        g.edges.push(TaskDependency {
            from_id: "b".into(),
            to_id: "a".into(),
            kind: "blocks".into(),
        });
        assert_eq!(validate(&g, &[]), Err(GraphError::Cycle));
    }
    #[test]
    fn downstream_only_invalidation() {
        let g = graph();
        let n = apply_patch(
            g,
            &[PatchOp::ChangeAcceptanceCriteria {
                task_id: "a".into(),
                criteria: vec!["new".into()],
            }],
            1,
            &[],
        )
        .unwrap();
        assert_eq!(n.tasks["b"].status, TaskStatus::Invalidated);
    }
}
