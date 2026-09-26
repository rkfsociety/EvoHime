include!("ipc_bridge_header.rs");
pub(crate) use crate::policy_gate;
pub(crate) use crate::CoreReceiptSigner;
#[path = "ipc_bridge_advanced_commands.rs"]
mod ipc_bridge_advanced_commands;
#[path = "ipc_bridge_ambient_workflow.rs"]
mod ipc_bridge_ambient_workflow;
#[path = "ipc_bridge_core_commands.rs"]
mod ipc_bridge_core_commands;
#[path = "ipc_bridge_goals_skills.rs"]
mod ipc_bridge_goals_skills;
#[path = "ipc_bridge_memory_capabilities.rs"]
mod ipc_bridge_memory_capabilities;
#[path = "ipc_bridge_projections.rs"]
mod ipc_bridge_projections;
#[path = "ipc_bridge_terminal_review.rs"]
mod ipc_bridge_terminal_review;
#[path = "ipc_bridge_workspace_commands.rs"]
mod ipc_bridge_workspace_commands;
pub(crate) use ipc_bridge_projections::*;
#[path = "ipc_bridge_extension_commands.rs"]
mod ipc_bridge_extension_commands;
#[path = "ipc_bridge_local_model_adaptation_scheduler.rs"]
mod ipc_bridge_local_model_adaptation_scheduler;

impl IpcBridge {
    /// Dispatches one durable local-model adaptation waiting for resources.
    ///
    /// # Errors
    ///
    /// Returns an error when the queue cannot be read or the queued job cannot
    /// pass its normal Core validation and dispatch checks.
    pub async fn dispatch_next_waiting_adaptation(&self) -> Result<bool, String> {
        self.dispatch_next_waiting_adaptation_inner().await
    }
}

#[cfg(test)]
#[path = "ipc_bridge_tests.rs"]
mod ipc_bridge_tests;
