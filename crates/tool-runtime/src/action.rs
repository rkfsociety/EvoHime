use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Cancelled,
    Executing,
    Succeeded,
    Failed,
    PolicyDenied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    pub request_id: String,
    pub approval_id: String,
    pub task_id: String,
    pub run_id: String,
    pub tool_id: String,
    pub manifest_hash: String,
    pub display_name: String,
    pub safe_preview: String,
    pub affected_resources: Vec<String>,
    pub side_effects: String,
    pub required_permission: String,
    pub budget_impact: String,
    pub expires_at_ms: u64,
    pub status: ActionStatus,
    pub decision_reason: Option<String>,
}

#[derive(Debug, Default)]
pub struct ActionConsole {
    actions: HashMap<String, ActionRequest>,
    decisions: HashMap<String, (bool, Option<String>)>,
}

impl ActionConsole {
    pub fn insert(&mut self, action: ActionRequest) {
        self.actions.insert(action.approval_id.clone(), action);
    }
    pub fn get(&self, approval_id: &str) -> Option<&ActionRequest> {
        self.actions.get(approval_id)
    }
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
