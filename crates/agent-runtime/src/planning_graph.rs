use std::collections::{HashMap, HashSet, BTreeSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("cycle detected in task graph")]
    CycleDetected,
    #[error("missing dependency: {0}")]
    MissingDependency(String),
}

pub struct TaskDependencyGraph {
    pub steps: HashMap<String, Vec<String>>, // task_id -> list of dependency IDs this task depends on
    pub topological_order: Vec<String>,       // execution order (respecting dependencies)
}

/// Validate graph (no cycles, all deps exist) and compute topological sort
///
/// Kahn's algorithm: O(V+E) complexity with deterministic ordering
/// - in_degree[task] = count of dependencies task has (tasks that must finish before this one)
/// - Initial queue = tasks with in_degree=0 (no deps), stored in BTreeSet for O(log V) operations
/// - Process queue, decrement in_degree of dependents, add to queue when ready
/// - Determinism: BTreeSet maintains lexicographic order, no manual sort needed
pub fn validate_and_sort_graph(
    steps: &HashMap<String, Vec<String>>,
) -> Result<TaskDependencyGraph, GraphError> {
    // Validate all dependencies exist
    let all_ids: HashSet<_> = steps.keys().cloned().collect();
    for (task_id, deps) in steps {
        for dep in deps {
            if !all_ids.contains(dep) {
                return Err(GraphError::MissingDependency(format!(
                    "task {} depends on non-existent task {}",
                    task_id, dep
                )));
            }
        }
    }

    // Build reverse index: dependents[X] = list of tasks that depend on X
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    for id in &all_ids {
        dependents.insert(id.clone(), Vec::new());
    }
    for (task_id, deps) in steps {
        for dep in deps {
            dependents.get_mut(dep).unwrap().push(task_id.clone());
        }
    }

    // Kahn's algorithm: in_degree = # of dependencies each task has
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    for id in &all_ids {
        in_degree.insert(id.clone(), steps[id].len());
    }

    // Initial queue: all tasks with no dependencies (in_degree=0)
    // BTreeSet maintains deterministic lexicographic order: O(log V) per insert/remove
    let mut queue: BTreeSet<String> = in_degree
        .iter()
        .filter(|(_, &degree)| degree == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut sorted = Vec::new();
    while let Some(current) = queue.pop_first() {
        sorted.push(current.clone());

        // For each task that depends on current, decrement in_degree
        for dependent in &dependents[&current] {
            *in_degree.get_mut(dependent).unwrap() -= 1;
            if in_degree[dependent] == 0 {
                queue.insert(dependent.clone()); // O(log V)
            }
        }
    }

    // If not all tasks processed, there's a cycle
    if sorted.len() != all_ids.len() {
        return Err(GraphError::CycleDetected);
    }

    Ok(TaskDependencyGraph {
        steps: steps.clone(),
        topological_order: sorted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_cycle_in_graph() {
        let mut steps = HashMap::new();
        steps.insert("task-a".to_string(), vec!["task-b".to_string()]);
        steps.insert("task-b".to_string(), vec!["task-a".to_string()]);

        let result = validate_and_sort_graph(&steps);
        assert!(result.is_err());
        assert!(matches!(result, Err(GraphError::CycleDetected)));
    }

    #[test]
    fn test_missing_dependency() {
        let mut steps = HashMap::new();
        steps.insert("task-a".to_string(), vec!["task-b".to_string()]);

        let result = validate_and_sort_graph(&steps);
        assert!(result.is_err());
        assert!(matches!(result, Err(GraphError::MissingDependency(_))));
    }

    #[test]
    fn test_topological_sort_respects_dependencies() {
        let mut steps = HashMap::new();
        steps.insert("task-c".to_string(), vec![]);
        steps.insert("task-b".to_string(), vec!["task-c".to_string()]);
        steps.insert("task-a".to_string(), vec!["task-b".to_string()]);

        let result = validate_and_sort_graph(&steps);
        assert!(result.is_ok());
        let order = result.unwrap().topological_order;
        assert_eq!(order, vec!["task-c", "task-b", "task-a"]);
    }

    #[test]
    fn test_parallel_tasks_no_dependencies() {
        let mut steps = HashMap::new();
        steps.insert("task-a".to_string(), vec![]);
        steps.insert("task-b".to_string(), vec![]);
        steps.insert("task-c".to_string(), vec![]);

        let result = validate_and_sort_graph(&steps);
        assert!(result.is_ok());
        let order = result.unwrap().topological_order;
        assert_eq!(order.len(), 3);
        // All can be executed in any order since no dependencies, but order is deterministic (lexicographic)
        assert_eq!(order, vec!["task-a", "task-b", "task-c"]);
    }

    #[test]
    fn test_diamond_dependency_pattern() {
        // A
        // ├─ B
        // └─ C
        //    └─ D
        let mut steps = HashMap::new();
        steps.insert("a".to_string(), vec![]);
        steps.insert("b".to_string(), vec!["a".to_string()]);
        steps.insert("c".to_string(), vec!["a".to_string()]);
        steps.insert("d".to_string(), vec!["b".to_string(), "c".to_string()]);

        let result = validate_and_sort_graph(&steps);
        assert!(result.is_ok());
        let order = result.unwrap().topological_order;

        // Verify ordering constraints: a before b, a before c, b before d, c before d
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        let pos_c = order.iter().position(|x| x == "c").unwrap();
        let pos_d = order.iter().position(|x| x == "d").unwrap();

        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn test_deterministic_order_with_independent_nodes() {
        let mut steps = HashMap::new();
        steps.insert("x".to_string(), vec![]);
        steps.insert("z".to_string(), vec![]);
        steps.insert("y".to_string(), vec![]);

        let result = validate_and_sort_graph(&steps);
        assert!(result.is_ok());
        // BTreeSet ensures deterministic lexicographic order
        let order = result.unwrap().topological_order;
        assert_eq!(order, vec!["x", "y", "z"]);
    }

    #[test]
    fn test_single_node() {
        let mut steps = HashMap::new();
        steps.insert("only".to_string(), vec![]);

        let result = validate_and_sort_graph(&steps);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().topological_order, vec!["only"]);
    }

    #[test]
    fn test_empty_graph() {
        let steps = HashMap::new();
        let result = validate_and_sort_graph(&steps);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().topological_order.len(), 0);
    }
}
