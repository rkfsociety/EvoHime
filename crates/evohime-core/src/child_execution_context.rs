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
//! The marker carries both the worker depth (`1..=MAX_CHILD_DEPTH`) and whether
//! the queen's original request allowed this worker to delegate one extra hop.
//! Validation in [`crate::child_runtime::ChildTaskRequest::validate`] and
//! [`crate::child_contracts::TypedChildTaskRequest::validate`] reads the
//! marker to distinguish an unverified claim from a real nested-delegation
//! attempt and to reject anything deeper than one extra hop.

use std::future::Future;

/// Runtime identity of the child worker currently executing.
///
/// Values are created only by [`scope`], which the executor wraps around its
/// worker future. The struct is not serializable by design: it never travels
/// over the wire. It carries the depth of the running worker and whether the
/// queen's original request allowed this worker to delegate one extra hop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChildExecutionContext {
    child_task_id: String,
    parent_task_id: String,
    depth: u8,
    delegation_permitted: bool,
}

impl ChildExecutionContext {
    /// Creates the marker for one running child worker.
    pub(crate) fn new(
        child_task_id: impl Into<String>,
        parent_task_id: impl Into<String>,
        depth: u8,
        delegation_permitted: bool,
    ) -> Self {
        Self {
            child_task_id: child_task_id.into(),
            parent_task_id: parent_task_id.into(),
            depth,
            delegation_permitted,
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

    /// Returns the nesting depth of the running worker (1 or 2).
    pub(crate) fn depth(&self) -> u8 {
        self.depth
    }

    /// Returns whether the original request carried a delegation grant that
    /// still allows this worker to spawn one sub-agent.
    pub(crate) fn delegation_permitted(&self) -> bool {
        self.delegation_permitted
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
        assert!(current().is_none());
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
        assert!(current().is_none());
        assert!(runtime_request(false).validate().is_ok());
        assert!(typed_request(false).validate().is_ok());
    }

    #[tokio::test]
    async fn marker_never_leaks_out_of_its_task() {
        let context = ChildExecutionContext::new("child-1", "task-1", 1, false);
        let inside = scope(context.clone(), async {
            assert!(current().is_some());
            assert_eq!(current().as_ref(), Some(&context));
            assert_eq!(current().map(|value| value.depth()), Some(1));
            assert!(!current().is_some_and(|value| value.delegation_permitted()));
            assert!(runtime_request(false).validate().is_ok());
            assert!(runtime_request(true).validate().is_ok());
            typed_request(true).validate()
        })
        .await;
        assert!(inside.is_ok());
        assert!(current().is_none());
    }

    #[tokio::test]
    async fn depth_three_marker_rejects_nested_delegation() {
        let context = ChildExecutionContext::new("child-1", "task-1", 3, true);
        let result = scope(context, async {
            assert_eq!(
                runtime_request(true).validate(),
                Err(ChildRuntimeError::ChildDepthExceeded)
            );
            typed_request(true).validate()
        })
        .await;
        assert_eq!(result, Err(ContractError::ChildDepthExceeded));
    }

    #[tokio::test]
    async fn spawned_task_does_not_inherit_the_marker() {
        let context = ChildExecutionContext::new("child-1", "task-1", 1, false);
        let inherited = scope(context, async {
            tokio::spawn(async { current().is_some() })
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
