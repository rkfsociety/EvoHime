//! Bounded runtime policy for read-only child tasks.
//!
//! This module is intentionally standalone until the Core delegation runner is
//! wired. It validates the boundary before any child can receive context or
//! return a report: no nested children, elevation, writes, shell, or network
//! mutation are expressible as an accepted request.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    fmt,
};

/// Maximum character count for child and parent task identifiers.
pub const MAX_ID_CHARS: usize = 128;
/// Maximum character count for a child role label.
pub const MAX_ROLE_CHARS: usize = 64;
/// Maximum context, finding, or reduced-input item count.
pub const MAX_CONTEXT_ITEMS: usize = 32;
/// Maximum character count for one reduced context item.
pub const MAX_CONTEXT_ITEM_CHARS: usize = 2_048;
/// Maximum total serialized context bytes passed to one child.
pub const MAX_CONTEXT_BYTES: usize = 16 * 1024;
/// Maximum serialized child output size in bytes.
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024;
/// Maximum character count for one child report field.
pub const MAX_REPORT_CHARS: usize = 8_192;
/// Maximum number of evidence source references in one report.
pub const MAX_SOURCES: usize = 32;
/// Maximum character count for one evidence source reference.
pub const MAX_SOURCE_CHARS: usize = 512;
/// Maximum lifecycle events retained by one child journal.
pub const MAX_CHILD_EVENTS: usize = 256;

/// Read-only child task category supported by the runtime contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildTaskKind {
    /// Search source files and return references.
    CodeSearch,
    /// Review a threat model and return bounded findings.
    ThreatModelReview,
    /// Review a test plan without executing it.
    TestPlanReview,
    /// Draft or review documentation.
    Documentation,
    /// Summarize project onboarding information.
    Onboarding,
}

/// Completion quality of a child report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildReportStatus {
    /// Child completed the requested task.
    Complete,
    /// Child produced a bounded result but could not finish fully.
    Partial,
    /// Child rejected the task or could not provide an acceptable result.
    Rejected,
}

/// Core-owned lifecycle for a logical child job.  The UI may project these
/// values, but it cannot manufacture a transition.
/// Core-owned lifecycle state for a delegated child task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildLifecycleState {
    /// Child request was created.
    Created,
    /// Request is waiting in the execution queue.
    Queued,
    /// Child work is in progress.
    Running,
    /// Returned report is being validated.
    Validating,
    /// Validated report awaits parent acceptance.
    WaitingParentAcceptance,
    /// Parent accepted the result.
    Accepted,
    /// Parent rejected the result.
    Rejected,
    /// Child execution failed.
    Failed,
    /// Child was cancelled.
    Cancelled,
    /// Child exceeded its time limit.
    TimedOut,
    /// Child was aborted by its owner or runtime.
    Aborted,
}

/// Replay-safe journal entry for one child lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildLifecycleEvent {
    /// Deterministic child-and-sequence event identifier.
    pub event_id: String,
    /// Child task associated with the transition.
    pub child_task_id: String,
    /// Parent task that delegated the child.
    pub parent_task_id: String,
    /// Monotonic sequence within this child's journal.
    pub sequence: u64,
    /// Lifecycle state reached by this event.
    pub state: ChildLifecycleState,
    /// Optional bounded transition explanation.
    pub reason: Option<String>,
}

/// Invalid child lifecycle transition or attempt to advance a terminal state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildTransitionError {
    /// Requested transition is not allowed from the current state.
    InvalidTransition {
        /// Current lifecycle state.
        from: ChildLifecycleState,
        /// Requested lifecycle state.
        to: ChildLifecycleState,
    },
    /// Child is already in a terminal state.
    TerminalState,
}

impl fmt::Display for ChildTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid child transition: {from:?} -> {to:?}")
            }
            Self::TerminalState => write!(f, "child is already terminal"),
        }
    }
}

impl std::error::Error for ChildTransitionError {}

/// Small replay-safe lifecycle journal used by the Core dispatcher and IPC
/// projection.  At-least-once delivery is harmless because event ids are
/// deterministic and terminal states cannot be advanced again.
#[derive(Debug, Clone)]
pub struct ChildLifecycle {
    request: ChildTaskRequest,
    state: ChildLifecycleState,
    sequence: u64,
    events: VecDeque<ChildLifecycleEvent>,
}

impl ChildLifecycle {
    /// Validates a request and creates its initial lifecycle event.
    pub fn create(request: ChildTaskRequest) -> Result<Self, ChildRuntimeError> {
        request.validate()?;
        let mut lifecycle = Self {
            request,
            state: ChildLifecycleState::Created,
            sequence: 0,
            events: VecDeque::new(),
        };
        lifecycle.record(ChildLifecycleState::Created, None);
        Ok(lifecycle)
    }

    /// Returns the validated request owned by this lifecycle journal.
    pub fn request(&self) -> &ChildTaskRequest {
        &self.request
    }
    /// Returns the current child lifecycle state.
    pub fn state(&self) -> ChildLifecycleState {
        self.state
    }
    /// Returns retained events whose sequence is greater than the supplied cursor.
    pub fn events_after(&self, sequence: u64) -> Vec<ChildLifecycleEvent> {
        self.events
            .iter()
            .filter(|event| event.sequence > sequence)
            .cloned()
            .collect()
    }

    /// Applies an allowed transition and records its bounded reason.
    pub fn transition(
        &mut self,
        next: ChildLifecycleState,
        reason: Option<String>,
    ) -> Result<(), ChildTransitionError> {
        if is_terminal(self.state) {
            return Err(ChildTransitionError::TerminalState);
        }
        if !allowed_transition(self.state, next) {
            return Err(ChildTransitionError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        self.record(next, reason);
        Ok(())
    }

    fn record(&mut self, state: ChildLifecycleState, reason: Option<String>) {
        self.sequence += 1;
        let event = ChildLifecycleEvent {
            event_id: format!("{}:{}", self.request.child_task_id, self.sequence),
            child_task_id: self.request.child_task_id.clone(),
            parent_task_id: self.request.parent_task_id.clone(),
            sequence: self.sequence,
            state,
            reason: reason.map(|value| value.chars().take(512).collect()),
        };
        self.events.push_back(event);
        while self.events.len() > MAX_CHILD_EVENTS {
            self.events.pop_front();
        }
    }
}

fn is_terminal(state: ChildLifecycleState) -> bool {
    matches!(
        state,
        ChildLifecycleState::Accepted
            | ChildLifecycleState::Rejected
            | ChildLifecycleState::Failed
            | ChildLifecycleState::Cancelled
            | ChildLifecycleState::TimedOut
            | ChildLifecycleState::Aborted
    )
}

fn allowed_transition(from: ChildLifecycleState, to: ChildLifecycleState) -> bool {
    matches!(
        (from, to),
        (ChildLifecycleState::Created, ChildLifecycleState::Queued)
            | (ChildLifecycleState::Queued, ChildLifecycleState::Running)
            | (
                ChildLifecycleState::Running,
                ChildLifecycleState::Validating
            )
            | (ChildLifecycleState::Running, ChildLifecycleState::Cancelled)
            | (ChildLifecycleState::Running, ChildLifecycleState::TimedOut)
            | (ChildLifecycleState::Running, ChildLifecycleState::Failed)
            | (
                ChildLifecycleState::Validating,
                ChildLifecycleState::WaitingParentAcceptance
            )
            | (
                ChildLifecycleState::Validating,
                ChildLifecycleState::Rejected
            )
            | (ChildLifecycleState::Validating, ChildLifecycleState::Failed)
            | (
                ChildLifecycleState::WaitingParentAcceptance,
                ChildLifecycleState::Accepted
            )
            | (
                ChildLifecycleState::WaitingParentAcceptance,
                ChildLifecycleState::Rejected
            )
    )
}

/// Invalid child request/report, forbidden authority, or exceeded output bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildRuntimeError {
    /// Required identifier or text value is empty.
    EmptyField(&'static str),
    /// A text field exceeds its declared character bound.
    FieldTooLong {
        /// Name of the text field that exceeded its character bound.
        field: &'static str,
        /// Maximum permitted character count.
        max: usize,
    },
    /// A list exceeds its declared item count.
    TooManyItems {
        /// Name of the list that exceeded its item-count bound.
        field: &'static str,
        /// Maximum permitted number of items.
        max: usize,
    },
    /// Aggregate reduced context exceeds its byte bound.
    ContextTooLarge {
        /// Actual aggregate context size in bytes.
        actual: usize,
        /// Maximum accepted context size in bytes.
        max: usize,
    },
    /// Serialized report exceeds its byte bound.
    OutputTooLarge {
        /// Actual serialized report size in bytes.
        actual: usize,
        /// Maximum accepted report size in bytes.
        max: usize,
    },
    /// Requested capability is not read-only.
    ForbiddenCapability(String),
    /// Nested child delegation is prohibited.
    NestedChildForbidden,
    /// Returned report does not belong to the request's child identifier.
    TaskMismatch,
    /// Report contains duplicate evidence references.
    DuplicateSource,
    /// Report content appears to contain a secret or credential.
    SecretLikeContent,
}

impl fmt::Display for ChildRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::TooManyItems { field, max } => write!(f, "{field} exceeds {max} items"),
            Self::ContextTooLarge { actual, max } => {
                write!(f, "context is {actual} bytes, maximum is {max}")
            }
            Self::OutputTooLarge { actual, max } => {
                write!(f, "output is {actual} bytes, maximum is {max}")
            }
            Self::ForbiddenCapability(capability) => {
                write!(f, "forbidden child capability: {capability}")
            }
            Self::NestedChildForbidden => write!(f, "nested child delegation is forbidden"),
            Self::TaskMismatch => write!(f, "report child_task_id does not match request"),
            Self::DuplicateSource => write!(f, "report contains duplicate sources"),
            Self::SecretLikeContent => write!(f, "secret-like content is not allowed"),
        }
    }
}

impl std::error::Error for ChildRuntimeError {}

/// The accepted request has only read-oriented capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildTaskRequest {
    /// Stable child task identifier.
    pub child_task_id: String,
    /// Parent task identifier.
    pub parent_task_id: String,
    /// Bounded role label assigned to the child.
    pub role: String,
    /// Read-only child task category.
    pub kind: ChildTaskKind,
    /// Reduced, bounded context supplied to the child.
    pub reduced_context: Vec<String>,
    /// Maximum serialized output size accepted from the child.
    pub max_output_bytes: usize,
    /// Read-only capabilities requested for the child.
    pub requested_capabilities: Vec<String>,
    /// Whether the parent is itself a child; nested delegation is rejected.
    pub parent_is_child: bool,
}

impl ChildTaskRequest {
    /// Validates identifiers, context bounds, output bounds, and read-only capabilities.
    pub fn validate(&self) -> Result<(), ChildRuntimeError> {
        validate_text("child_task_id", &self.child_task_id, MAX_ID_CHARS)?;
        validate_text("parent_task_id", &self.parent_task_id, MAX_ID_CHARS)?;
        validate_text("role", &self.role, MAX_ROLE_CHARS)?;
        if self.parent_is_child {
            return Err(ChildRuntimeError::NestedChildForbidden);
        }
        if self.reduced_context.len() > MAX_CONTEXT_ITEMS {
            return Err(ChildRuntimeError::TooManyItems {
                field: "reduced_context",
                max: MAX_CONTEXT_ITEMS,
            });
        }
        let context_bytes = self
            .reduced_context
            .iter()
            .try_fold(0usize, |total, item| {
                validate_text("context_item", item, MAX_CONTEXT_ITEM_CHARS)?;
                Ok::<_, ChildRuntimeError>(total.saturating_add(item.len()))
            })?;
        if context_bytes > MAX_CONTEXT_BYTES {
            return Err(ChildRuntimeError::ContextTooLarge {
                actual: context_bytes,
                max: MAX_CONTEXT_BYTES,
            });
        }
        if self.max_output_bytes == 0 || self.max_output_bytes > MAX_OUTPUT_BYTES {
            return Err(ChildRuntimeError::OutputTooLarge {
                actual: self.max_output_bytes,
                max: MAX_OUTPUT_BYTES,
            });
        }
        for capability in &self.requested_capabilities {
            validate_text("capability", capability, MAX_ROLE_CHARS)?;
            if !is_read_only_capability(capability) {
                return Err(ChildRuntimeError::ForbiddenCapability(capability.clone()));
            }
        }
        Ok(())
    }

    /// Validates and serializes the request in deterministic field order.
    pub fn deterministic_json(&self) -> Result<String, ChildRuntimeError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| ChildRuntimeError::FieldTooLong {
            field: "serialized_request",
            max: MAX_CONTEXT_BYTES,
        })
    }
}

/// Bounded read-oriented result returned by a child task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildReport {
    /// Child task identifier this report answers.
    pub child_task_id: String,
    /// Completion quality for the task.
    pub status: ChildReportStatus,
    /// Bounded summary of the result.
    pub summary: String,
    /// Bounded findings supporting the summary.
    pub findings: Vec<String>,
    /// Unique references to evidence sources.
    pub sources: Vec<String>,
    /// Child confidence score from zero to one hundred.
    pub confidence_percent: u8,
}

impl ChildReport {
    /// Validates report fields, evidence uniqueness, secret screening, and serialized size.
    pub fn validate(&self) -> Result<(), ChildRuntimeError> {
        validate_text("child_task_id", &self.child_task_id, MAX_ID_CHARS)?;
        validate_text("summary", &self.summary, MAX_REPORT_CHARS)?;
        if self.findings.len() > MAX_CONTEXT_ITEMS {
            return Err(ChildRuntimeError::TooManyItems {
                field: "findings",
                max: MAX_CONTEXT_ITEMS,
            });
        }
        for finding in &self.findings {
            validate_text("finding", finding, MAX_REPORT_CHARS)?;
            reject_secret_like(finding)?;
        }
        if self.sources.len() > MAX_SOURCES {
            return Err(ChildRuntimeError::TooManyItems {
                field: "sources",
                max: MAX_SOURCES,
            });
        }
        let mut sources = BTreeSet::new();
        for source in &self.sources {
            validate_text("source", source, MAX_SOURCE_CHARS)?;
            reject_secret_like(source)?;
            if !sources.insert(source) {
                return Err(ChildRuntimeError::DuplicateSource);
            }
        }
        reject_secret_like(&self.summary)?;
        let serialized =
            serde_json::to_vec(self).map_err(|_| ChildRuntimeError::OutputTooLarge {
                actual: usize::MAX,
                max: MAX_OUTPUT_BYTES,
            })?;
        if serialized.len() > MAX_OUTPUT_BYTES {
            return Err(ChildRuntimeError::OutputTooLarge {
                actual: serialized.len(),
                max: MAX_OUTPUT_BYTES,
            });
        }
        Ok(())
    }
}

/// Validates the request and report and rejects a report for a different child task.
pub fn accept_report(
    request: &ChildTaskRequest,
    report: &ChildReport,
) -> Result<ChildReport, ChildRuntimeError> {
    request.validate()?;
    report.validate()?;
    if request.child_task_id != report.child_task_id {
        return Err(ChildRuntimeError::TaskMismatch);
    }
    Ok(report.clone())
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), ChildRuntimeError> {
    if value.trim().is_empty() {
        return Err(ChildRuntimeError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(ChildRuntimeError::FieldTooLong { field, max });
    }
    Ok(())
}

fn is_read_only_capability(capability: &str) -> bool {
    matches!(
        capability,
        "workspace.read" | "workspace.search" | "git.diff" | "git.status"
    )
}

fn reject_secret_like(value: &str) -> Result<(), ChildRuntimeError> {
    let lower = value.to_ascii_lowercase();
    if [
        "api_key",
        "apikey",
        "authorization",
        "bearer ",
        "password",
        "secret",
        "token",
        "sk-",
        "ghp_",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Err(ChildRuntimeError::SecretLikeContent);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ChildTaskRequest {
        ChildTaskRequest {
            child_task_id: "child-1".into(),
            parent_task_id: "task-1".into(),
            role: "researcher".into(),
            kind: ChildTaskKind::CodeSearch,
            reduced_context: vec!["inspect src".into()],
            max_output_bytes: 4096,
            requested_capabilities: vec!["workspace.read".into(), "git.diff".into()],
            parent_is_child: false,
        }
    }

    fn report() -> ChildReport {
        ChildReport {
            child_task_id: "child-1".into(),
            status: ChildReportStatus::Complete,
            summary: "found one relevant module".into(),
            findings: vec!["module is bounded".into()],
            sources: vec!["src/lib.rs:10".into()],
            confidence_percent: 90,
        }
    }

    #[test]
    fn accepts_read_only_request_and_report() {
        let accepted = accept_report(&request(), &report()).unwrap();
        assert_eq!(accepted.child_task_id, "child-1");
    }

    #[test]
    fn rejects_nested_or_mutating_capabilities() {
        let mut nested = request();
        nested.parent_is_child = true;
        assert_eq!(
            nested.validate(),
            Err(ChildRuntimeError::NestedChildForbidden)
        );

        let mut mutating = request();
        mutating.requested_capabilities = vec!["workspace.write".into()];
        assert!(matches!(
            mutating.validate(),
            Err(ChildRuntimeError::ForbiddenCapability(_))
        ));
    }

    #[test]
    fn bounds_context_and_output() {
        let mut oversized = request();
        oversized.reduced_context = vec!["x".repeat(MAX_CONTEXT_ITEM_CHARS + 1)];
        assert!(oversized.validate().is_err());

        oversized = request();
        oversized.max_output_bytes = MAX_OUTPUT_BYTES + 1;
        assert!(matches!(
            oversized.validate(),
            Err(ChildRuntimeError::OutputTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_secret_like_report_content_and_duplicate_sources() {
        let mut unsafe_report = report();
        unsafe_report.summary = "token must not leak".into();
        assert_eq!(
            unsafe_report.validate(),
            Err(ChildRuntimeError::SecretLikeContent)
        );

        unsafe_report = report();
        unsafe_report.sources.push("src/lib.rs:10".into());
        assert_eq!(
            unsafe_report.validate(),
            Err(ChildRuntimeError::DuplicateSource)
        );
    }

    #[test]
    fn rejects_mismatched_handoff_and_serializes_deterministically() {
        let mut mismatched = report();
        mismatched.child_task_id = "other".into();
        assert_eq!(
            accept_report(&request(), &mismatched),
            Err(ChildRuntimeError::TaskMismatch)
        );
        let first = request().deterministic_json().unwrap();
        assert_eq!(first, request().deterministic_json().unwrap());
    }

    #[test]
    fn lifecycle_is_core_owned_replay_safe_and_terminal() {
        let mut lifecycle = ChildLifecycle::create(request()).unwrap();
        lifecycle
            .transition(ChildLifecycleState::Queued, None)
            .unwrap();
        lifecycle
            .transition(ChildLifecycleState::Running, None)
            .unwrap();
        lifecycle
            .transition(ChildLifecycleState::Validating, None)
            .unwrap();
        lifecycle
            .transition(ChildLifecycleState::WaitingParentAcceptance, None)
            .unwrap();
        lifecycle
            .transition(ChildLifecycleState::Accepted, Some("parent_gate".into()))
            .unwrap();
        assert_eq!(lifecycle.state(), ChildLifecycleState::Accepted);
        assert_eq!(lifecycle.events_after(0).len(), 6);
        assert_eq!(lifecycle.events_after(3)[0].event_id, "child-1:4");
        assert_eq!(
            lifecycle.transition(ChildLifecycleState::Running, None),
            Err(ChildTransitionError::TerminalState)
        );
    }

    #[test]
    fn lifecycle_rejects_skipping_security_states() {
        let mut lifecycle = ChildLifecycle::create(request()).unwrap();
        assert!(matches!(
            lifecycle.transition(ChildLifecycleState::Accepted, None),
            Err(ChildTransitionError::InvalidTransition { .. })
        ));
    }
}
