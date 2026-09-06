include!("core_prelude.rs");
include!("core_root_prelude.rs");
mod core_protocol;
pub use core_protocol::*;
mod core_journal;
pub use core_journal::*;
mod core_lifecycle;
pub use core_lifecycle::{
    attach_permission_audit_sink, spawn_ambient_retention, spawn_approval_gc,
    spawn_model_provenance_retention, spawn_receipt_retention,
};
mod core_agent;
pub(crate) use core_agent::*;
pub use core_agent::{
    AgentRunError, ApprovalCoordinator, ModelAgent, RoutingApprovalRegistry, SelectedModel,
    TaskExecutor, ToolAgent,
};
mod core_coordinator;
pub use core_coordinator::TaskCoordinator;
mod bounded_tasks;
mod core_domains;
pub(crate) use core_domains::*;
pub mod adapter_contract;
pub mod automation;
pub mod automation_acceptance;
pub mod automation_runtime;
pub mod automation_scheduler;
pub mod automation_simulation;
#[cfg(test)]
mod core_tests;
pub mod target_contract;
