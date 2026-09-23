use serde::Serialize;

/// Bounded, read-only plan and specification prepared from a task description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskPlanSpec {
    /// Generated task plan text.
    pub plan: String,
    /// Generated specification containing the task's criteria and context.
    pub spec: String,
    /// Whether generating this result avoided writing files.
    pub read_only: bool,
}

/// Builds a read-only plan and specification, truncating each to `max_chars`.
///
/// The supplied acceptance criteria, non-goals, and context are included in
/// the specification so downstream planning can preserve the task boundary.
pub fn build_task_plan_spec(
    title: &str,
    description: &str,
    acceptance_criteria: &str,
    non_goals: &str,
    context: &str,
    max_chars: usize,
) -> TaskPlanSpec {
    let plan = bounded(
        &format!(
            "1. Изучить контекст задачи «{title}».\n2. Проверить локальные references.\n3. Сопоставить результат с acceptance criteria.\n4. Подготовить bounded Build scope без записи файлов."
        ),
        max_chars,
    );
    let spec = bounded(
        &format!(
            "Title: {title}\n\nDescription:\n{description}\n\nAcceptance criteria:\n{acceptance_criteria}\n\nNon-goals:\n{non_goals}\n\nRead-only context:\n{context}"
        ),
        max_chars,
    );
    TaskPlanSpec {
        plan,
        spec,
        read_only: true,
    }
}

fn bounded(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::build_task_plan_spec;

    #[test]
    fn plan_and_spec_are_read_only_and_bounded() {
        let result = build_task_plan_spec(
            "Task",
            "Description",
            "Tests pass",
            "No network",
            "## Task\nref",
            40,
        );
        assert!(result.read_only);
        assert!(result.plan.chars().count() <= 40);
        assert!(result.spec.chars().count() <= 40);
    }
}
