use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Lifecycle state of an action that may need human approval.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    /// The action is awaiting a decision.
    Pending,
    /// The action was approved and may be dispatched.
    Approved,
    /// The action was explicitly rejected.
    Rejected,
    /// The approval deadline passed without a decision.
    Expired,
    /// The action was cancelled before completion.
    Cancelled,
    /// The approved action is currently executing.
    Executing,
    /// The action completed successfully.
    Succeeded,
    /// Execution of the action failed.
    Failed,
    /// Policy denied the action before approval or execution.
    PolicyDenied,
}

/// Approval request and safe summary of its proposed effects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    /// Stable identifier of the request record.
    pub request_id: String,
    /// Approval identifier used to resolve this request.
    pub approval_id: String,
    /// Task that submitted the action.
    pub task_id: String,
    /// Workflow run containing the action, if applicable.
    pub run_id: String,
    /// Identifier of the tool being invoked.
    pub tool_id: String,
    /// Manifest digest used to bind approval to the reviewed tool version.
    pub manifest_hash: String,
    /// User-facing name of the proposed action.
    pub display_name: String,
    /// Redacted preview suitable for display to the approver.
    pub safe_preview: String,
    /// Resources that the action may read or change.
    pub affected_resources: Vec<String>,
    /// Summary of the effects the action may perform.
    pub side_effects: String,
    /// Permission required to run the action.
    pub required_permission: String,
    /// Summary of the action's effect on the run budget.
    pub budget_impact: String,
    /// Approval expiry time in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Current approval and execution state.
    pub status: ActionStatus,
    /// Optional reason supplied with the approval decision.
    pub decision_reason: Option<String>,
}

/// In-memory action approval records with idempotent decision handling.
#[derive(Debug, Default)]
pub struct ActionConsole {
    actions: HashMap<String, ActionRequest>,
    decisions: HashMap<String, (bool, Option<String>)>,
}

impl ActionConsole {
    /// Stores or replaces the request indexed by its approval identifier.
    pub fn insert(&mut self, action: ActionRequest) {
        self.actions.insert(action.approval_id.clone(), action);
    }
    /// Returns the action associated with an approval identifier.
    pub fn get(&self, approval_id: &str) -> Option<&ActionRequest> {
        self.actions.get(approval_id)
    }
    /// Applies an approval decision once, using an idempotency key to reject conflicts.
    ///
    /// A pending request whose deadline is at or before `now_ms` is marked expired.
    pub fn resolve(
        &mut self,
        approval_id: &str,
        idempotency_key: &str,
        granted: bool,
        reason: Option<String>,
        now_ms: u64,
    ) -> Result<ActionStatus, &'static str> {
        if idempotency_key.is_empty() {
            return Err("missing_idempotency_key");
        }
        if let Some((old, _)) = self.decisions.get(idempotency_key) {
            if *old == granted {
                return Ok(if granted {
                    ActionStatus::Approved
                } else {
                    ActionStatus::Rejected
                });
            }
            return Err("decision_conflict");
        }
        let action = self
            .actions
            .get_mut(approval_id)
            .ok_or("unknown_approval")?;
        if action.expires_at_ms <= now_ms && matches!(action.status, ActionStatus::Pending) {
            action.status = ActionStatus::Expired;
            return Err("expired");
        }
        if !matches!(action.status, ActionStatus::Pending) {
            return Err("terminal_state");
        }
        action.status = if granted {
            ActionStatus::Approved
        } else {
            ActionStatus::Rejected
        };
        action.decision_reason = reason.clone();
        self.decisions
            .insert(idempotency_key.to_owned(), (granted, reason));
        Ok(action.status.clone())
    }
}
