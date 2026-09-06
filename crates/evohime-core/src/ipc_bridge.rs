include!("ipc_bridge_header.rs");
pub(crate) use crate::CoreReceiptSigner;
pub(crate) use crate::policy_gate;
#[path = "ipc_bridge_core_commands.rs"]
mod ipc_bridge_core_commands;
#[path = "ipc_bridge_ambient_workflow.rs"]
mod ipc_bridge_ambient_workflow;
#[path = "ipc_bridge_workspace_commands.rs"]
mod ipc_bridge_workspace_commands;
#[path = "ipc_bridge_memory_capabilities.rs"]
mod ipc_bridge_memory_capabilities;
#[path = "ipc_bridge_terminal_review.rs"]
mod ipc_bridge_terminal_review;
#[path = "ipc_bridge_goals_skills.rs"]
mod ipc_bridge_goals_skills;
#[path = "ipc_bridge_advanced_commands.rs"]
mod ipc_bridge_advanced_commands;
#[path = "ipc_bridge_projections.rs"]
mod ipc_bridge_projections;
pub(crate) use ipc_bridge_projections::*;
#[path = "ipc_bridge_extension_commands.rs"]
mod ipc_bridge_extension_commands;

#[cfg(test)]
include!("ipc_bridge_tests.rs");
