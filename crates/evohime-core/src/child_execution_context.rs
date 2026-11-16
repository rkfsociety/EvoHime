//! Trust marker for a Core-owned child execution.
//!
//! The `parent_is_child` field in a child request is a claim supplied by the
//! caller. It is not evidence that the caller really runs inside a child
//! worker. This module owns the only writable marker that can back such a
//! claim: a task-local context installed by
//! [`crate::child_executor::RegisteredChildExecutor`] for the duration of one
//! worker run.
//!
//! The marker is deliberately not serializable and not reachable from the IPC
//! or legacy command handlers. Those handlers run in their own tasks and build
//! requests from wire fields, so they can never install or forge the context.
//! Validation in [`crate::child_runtime::ChildTaskRequest::validate`] and
//! [`crate::child_contracts::TypedChildTaskRequest::validate`] reads the
//! marker to distinguish an unverified claim from a real nested-delegation
//! attempt.

use std::future::Future;

/// Runtime identity of the child worker currently executing.
///
/// Values are created only by [`scope`], which the executor wraps around its
/// worker future. The struct is not serializable by design: it never travels
/// over the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChildExecutionContext {
    child_task_id: String,
    parent_task_id: String,
}

impl ChildExecutionContext {
    /// Creates the marker for one running child worker.
    pub(crate) fn new(child_task_id: impl Into<String>, parent_task_id: impl Into<String>) -> Self {
        Self {
            child_task_id: child_task_id.into(),
            parent_task_id: parent_task_id.into(),
        }
    }

    /// Returns the child task owned by this worker.
    pub(crate) fn child_task_id(&self) -> &str {
        &self.child_task_id
    }

    /// Returns the parent task that delegated this worker.
    pub(crate) fn parent_task_id(&self) -> &str {
        &self.parent_task_id
    }
}

tokio::task_local! {
    static CURRENT_CHILD_EXECUTION: ChildExecutionContext;
}

/// Returns the child execution context of the current task, if any.
///
/// A task that was not wrapped by [`scope`] has no context. In particular,
/// coordinator command handlers and IPC request handlers never observe one.
pub(crate) fn current() -> Option<ChildExecutionContext> {
    CURRENT_CHILD_EXECUTION.try_with(Clone::clone).ok()
}

/// Returns whether the current task is a Core-owned child worker.
pub(crate) fn is_child_execution() -> bool {
    CURRENT_CHILD_EXECUTION.try_with(|_| ()).is_ok()
}

/// Runs a future with the supplied child execution context installed.
///
/// This is the only way to install the marker. It is `pub(crate)` and used
/// solely by the registered child executor, never by a request handler.
pub(crate) async fn scope<F>(context: ChildExecutionContext, future: F) -> F::Output
where
    F: Future,
{
    CURRENT_CHILD_EXECUTION.scope(context, future).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::child_contracts::{ContractError, CorrelationContext, CorrelationId};
    use crate::child_runtime::{ChildRuntimeError, ChildTaskKind, ChildTaskRequest};

    fn runtime_request(parent_is_child: bool) -> ChildTaskRequest {
        ChildTaskRequest {
            child_task_id: "child-1".into(),
            parent_task_id: "task-1".into(),
            role: "researcher".into(),
            kind: ChildTaskKind::CodeSearch,
            reduced_context: vec!["inspect src".into()],
            max_output_bytes: 4096,
            requested_capabilities: vec!["workspace.read".into()],
            parent_is_child,
        }
    }

    fn typed_request(parent_is_child: bool) -> crate::child_contracts::TypedChildTaskRequest {
        let mut request = crate::child_contracts::TypedChildTaskRequest::new(
            "child-1",
            "task-1",
            "researcher",
            "inspect src",
            CorrelationContext::new(
                CorrelationId::new("task-1").expect("parent id"),
                CorrelationId::new("child-1").expect("child id"),
                1,
            ),
        )
        .expect("typed request");
        request.parent_is_child = parent_is_child;
        request
    }

    #[test]
    fn unmarked_child_claim_is_rejected_as_unverified() {
        assert!(!is_child_execution());
        assert_eq!(
            runtime_request(true).validate(),
            Err(ChildRuntimeError::ChildContextRequired)
        );
        assert_eq!(
            typed_request(true).validate(),
            Err(ContractError::ChildContextRequired)
        );
    }

    #[test]
    fn unmarked_top_level_request_still_validates() {
        assert!(!is_child_execution());
        assert!(runtime_request(false).validate().is_ok());
        assert!(typed_request(false).validate().is_ok());
    }

    #[tokio::test]
    async fn marker_never_leaks_out_of_its_task() {
        let context = ChildExecutionContext::new("child-1", "task-1");
        let inside = scope(context.clone(), async {
            assert!(is_child_execution());
            assert_eq!(current().as_ref(), Some(&context));
            assert!(runtime_request(false).validate().is_ok());
            runtime_request(true).validate()
        })
        .await;
        assert_eq!(inside, Err(ChildRuntimeError::NestedChildForbidden));
        assert!(!is_child_execution());
    }

    #[tokio::test]
    async fn spawned_task_does_not_inherit_the_marker() {
        let context = ChildExecutionContext::new("child-1", "task-1");
        let inherited = scope(context, async {
            tokio::spawn(async { is_child_execution() })
                .await
                .expect("spawned task")
        })
        .await;
        assert!(
            !inherited,
            "a spawned task must not inherit the child execution marker"
        );
    }
}
