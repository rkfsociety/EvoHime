include!("core_prelude.rs");
include!("core_root_prelude.rs");
mod core_protocol {
    use super::*;
    include!("core_protocol.rs");
}
pub use core_protocol::*;
mod core_journal {
    use super::*;
    include!("core_journal.rs");
}
pub use core_journal::*;
mod core_lifecycle {
    use super::*;
    include!("core_lifecycle.rs");
}
pub use core_lifecycle::{
    attach_permission_audit_sink, spawn_ambient_retention, spawn_approval_gc,
    spawn_model_provenance_retention, spawn_receipt_retention,
};
mod core_agent {
    use super::*;
    include!("core_agent.rs");
}
pub(crate) use core_agent::*;
pub use core_agent::{
    AgentRunError, ApprovalCoordinator, ModelAgent, RoutingApprovalRegistry, SelectedModel,
    TaskExecutor, ToolAgent,
};
mod core_coordinator {
    use super::*;
    include!("core_coordinator.rs");
}
pub use core_coordinator::TaskCoordinator;
mod core_domains {
    use super::*;
    include!("core_domains.rs");
}
pub(crate) use core_domains::*;
pub mod adapter_contract;
pub mod automation;
pub mod automation_acceptance;
pub mod automation_runtime;
pub mod automation_scheduler;
pub mod automation_simulation;
pub mod target_contract;
#[cfg(test)]
include!("core_tests.rs");
