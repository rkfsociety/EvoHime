use crate::{pattern::glob_match, Permission, PermissionMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRule {
    pub permission: Permission,
    pub pattern: String,
    pub mode: PermissionMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PolicyRuleSet(Vec<PolicyRule>);

impl PolicyRuleSet {
    pub fn new(rules: Vec<PolicyRule>) -> Self {
        Self(rules)
    }

    pub fn defaults() -> Self {
        Self::new(vec![
            PolicyRule {
                permission: Permission::FilesystemRead,
                pattern: "*.env".into(),
                mode: PermissionMode::Deny,
            },
            PolicyRule {
                permission: Permission::FilesystemRead,
                pattern: "*.env.*".into(),
                mode: PermissionMode::Deny,
            },
        ])
    }

    pub fn rules(&self) -> &[PolicyRule] {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn resolve(&self, permission: Permission, subject: &str) -> Option<PermissionMode> {
        self.0
            .iter()
            .rev()
            .find(|rule| rule.permission == permission && glob_match(&rule.pattern, subject))
            .map(|rule| rule.mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_matching_rule_wins_and_permissions_are_isolated() {
        let set = PolicyRuleSet::new(vec![
            PolicyRule {
                permission: Permission::ShellExecute,
                pattern: "cargo *".into(),
                mode: PermissionMode::Ask,
            },
            PolicyRule {
                permission: Permission::ShellExecute,
                pattern: "cargo test".into(),
                mode: PermissionMode::Allow,
            },
            PolicyRule {
                permission: Permission::GitWrite,
                pattern: "cargo *".into(),
                mode: PermissionMode::Deny,
            },
        ]);
        assert_eq!(
            set.resolve(Permission::ShellExecute, "cargo test"),
            Some(PermissionMode::Allow)
        );
        assert_eq!(
            set.resolve(Permission::ShellExecute, "cargo build"),
            Some(PermissionMode::Ask)
        );
        assert_eq!(set.resolve(Permission::GitRead, "cargo build"), None);
    }

    #[test]
    fn defaults_protect_env_files_only() {
        let set = PolicyRuleSet::defaults();
        assert_eq!(
            set.resolve(Permission::FilesystemRead, ".env"),
            Some(PermissionMode::Deny)
        );
        assert_eq!(
            set.resolve(Permission::FilesystemRead, "backend/.env.local"),
            Some(PermissionMode::Deny)
        );
        assert_eq!(set.resolve(Permission::FilesystemRead, "src/main.rs"), None);
    }
}
