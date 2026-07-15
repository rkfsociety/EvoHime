use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    FilesystemRead,
    FilesystemWrite,
    ShellExecute,
    GitRead,
    GitWrite,
    BrowserAccess,
    McpCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Ask,
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    Allowed,
    NeedsApproval,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub task_id: Uuid,
    pub tool_name: String,
    pub permission: Permission,
    pub scope: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalState {
    Pending,
    Granted,
    Denied,
}

#[derive(Debug, Clone)]
struct ApprovalRecord {
    request: ApprovalRequest,
    state: ApprovalState,
}

#[derive(Clone)]
pub struct PermissionEngine {
    modes: Arc<RwLock<HashMap<Permission, PermissionMode>>>,
    approvals: Arc<RwLock<HashMap<Uuid, ApprovalRecord>>>,
}

impl Default for PermissionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionEngine {
    pub fn new() -> Self {
        let mut modes = HashMap::new();
        modes.insert(Permission::FilesystemRead, PermissionMode::Allow);
        modes.insert(Permission::FilesystemWrite, PermissionMode::Ask);
        modes.insert(Permission::ShellExecute, PermissionMode::Ask);
        modes.insert(Permission::GitRead, PermissionMode::Allow);
        modes.insert(Permission::GitWrite, PermissionMode::Ask);
        Self {
            modes: Arc::new(RwLock::new(modes)),
            approvals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn mode(&self, permission: Permission) -> PermissionMode {
        self.modes
            .read()
            .await
            .get(&permission)
            .copied()
            .unwrap_or(PermissionMode::Ask)
    }

    pub async fn set_mode(&self, permission: Permission, mode: PermissionMode) {
        self.modes.write().await.insert(permission, mode);
    }

    pub async fn check(&self, permission: Permission) -> PermissionDecision {
        match self.mode(permission).await {
            PermissionMode::Allow => PermissionDecision::Allowed,
            PermissionMode::Ask => PermissionDecision::NeedsApproval,
            PermissionMode::Deny => PermissionDecision::Denied,
        }
    }

    pub async fn create_approval(
        &self,
        task_id: Uuid,
        tool_name: impl Into<String>,
        permission: Permission,
        scope: impl Into<String>,
    ) -> ApprovalRequest {
        let request = ApprovalRequest {
            id: Uuid::new_v4(),
            task_id,
            tool_name: tool_name.into(),
            permission,
            scope: scope.into(),
        };
        self.approvals.write().await.insert(
            request.id,
            ApprovalRecord {
                request: request.clone(),
                state: ApprovalState::Pending,
            },
        );
        request
    }

    pub async fn resolve(&self, id: Uuid, granted: bool) -> Option<ApprovalState> {
        let mut approvals = self.approvals.write().await;
        let record = approvals.get_mut(&id)?;
        if record.state != ApprovalState::Pending {
            return None;
        }
        record.state = if granted {
            ApprovalState::Granted
        } else {
            ApprovalState::Denied
        };
        Some(record.state)
    }

    pub async fn approval(&self, id: Uuid) -> Option<(ApprovalRequest, ApprovalState)> {
        self.approvals
            .read()
            .await
            .get(&id)
            .map(|r| (r.request.clone(), r.state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_executor::block_on;

    #[test]
    fn default_policy_allows_read_and_asks_for_write() {
        block_on(async {
            let engine = PermissionEngine::new();
            assert_eq!(
                engine.check(Permission::FilesystemRead).await,
                PermissionDecision::Allowed
            );
            assert_eq!(
                engine.check(Permission::FilesystemWrite).await,
                PermissionDecision::NeedsApproval
            );
        });
    }

    #[test]
    fn approval_is_one_shot() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let request = engine
                .create_approval(
                    task_id,
                    "filesystem.write",
                    Permission::FilesystemWrite,
                    "a.txt",
                )
                .await;
            assert_eq!(request.task_id, task_id);
            assert_eq!(
                engine.resolve(request.id, true).await,
                Some(ApprovalState::Granted)
            );
            assert_eq!(engine.resolve(request.id, false).await, None);
        });
    }
}
