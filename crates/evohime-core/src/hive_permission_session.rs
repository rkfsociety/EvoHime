//! Own permission policy of the Hive world.
//!
//! The workspace permission mode selected in an ordinary chat is a global
//! setting of the permission engine. Hive tasks run in a dedicated permission
//! session whose modes take precedence over that global setting, so chats and
//! the Hive cannot change each other's rights while they run in parallel.

use evohime_permissions::{Permission, PermissionMode};

/// Fixed identifier of the Hive permission session.
pub(crate) const SESSION_ID: u128 = 0x4556_4f48_494d_4500_4849_5645_0000_0001;

/// Hive policy: reading is free; anything that mutates state or leaves the
/// workspace requires an explicit approval.
pub(crate) fn modes() -> [(Permission, PermissionMode); 8] {
    [
        (Permission::FilesystemRead, PermissionMode::Allow),
        (Permission::GitRead, PermissionMode::Allow),
        (Permission::MemorySearch, PermissionMode::Allow),
        (Permission::FilesystemWrite, PermissionMode::Ask),
        (Permission::ShellExecute, PermissionMode::Ask),
        (Permission::GitWrite, PermissionMode::Ask),
        (Permission::BrowserAccess, PermissionMode::Ask),
        (Permission::McpCall, PermissionMode::Ask),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hive_session_reads_are_allowed_and_mutations_need_approval() {
        let modes = modes();
        for (permission, mode) in modes {
            let expected = match permission {
                Permission::FilesystemRead | Permission::GitRead | Permission::MemorySearch => {
                    PermissionMode::Allow
                }
                _ => PermissionMode::Ask,
            };
            assert_eq!(mode, expected, "{permission:?}");
        }
    }
}
