include!("ipc_bridge_header.rs");
pub(crate) use crate::CoreReceiptSigner;
pub(crate) use crate::policy_gate;
mod ipc_bridge_core_commands {
    use super::*;
    include!("ipc_bridge_core_commands.rs");
}
mod ipc_bridge_ambient_workflow {
    use super::*;
    include!("ipc_bridge_ambient_workflow.rs");
}
mod ipc_bridge_workspace_commands {
    use super::*;
    include!("ipc_bridge_workspace_commands.rs");
}
mod ipc_bridge_memory_capabilities {
    use super::*;
    include!("ipc_bridge_memory_capabilities.rs");
}
mod ipc_bridge_terminal_review {
    use super::*;
    include!("ipc_bridge_terminal_review.rs");
}
mod ipc_bridge_goals_skills {
    use super::*;
    include!("ipc_bridge_goals_skills.rs");
}
mod ipc_bridge_advanced_commands {
    use super::*;
    include!("ipc_bridge_advanced_commands.rs");
}
mod ipc_bridge_projections {
    use super::*;
    include!("ipc_bridge_projections.rs");
}
pub(crate) use ipc_bridge_projections::*;
mod ipc_bridge_extension_commands {
    use super::*;
    include!("ipc_bridge_extension_commands.rs");
}

#[cfg(test)]
include!("ipc_bridge_tests.rs");
