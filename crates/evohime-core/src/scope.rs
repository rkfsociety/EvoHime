use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildScope {
    pub allowed_paths: Vec<String>,
    pub allowed_operations: Vec<String>,
    pub expected_outputs: Vec<String>,
    pub protected_paths: Vec<String>,
    pub allowed_file_types: Vec<String>,
    pub max_files_changed: usize,
    pub max_bytes_changed: usize,
    pub allow_create: bool,
    pub allow_delete: bool,
    pub allow_rename: bool,
    pub baseline_snapshot_id: Option<String>,
    pub acceptance_criteria: String,
    pub risk_class: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedChange {
    pub relative_path: String,
    pub operation: String,
    pub bytes_changed: usize,
    pub creates: bool,
    pub deletes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopeViolation {
    pub path: String,
    pub reason: String,
}

fn risk_rank(value: &str) -> Option<u8> {
    match value {
        "low" => Some(0),
        "medium" => Some(1),
        "high" => Some(2),
        _ => None,
    }
}

pub fn restrict_to_policy(
    policy: &BuildScope,
    requested: &BuildScope,
) -> Result<BuildScope, Vec<ScopeViolation>> {
    let mut violations = Vec::new();
    if requested.max_files_changed > policy.max_files_changed {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "max_files_changed exceeds persisted policy".into(),
        });
    }
    if requested.max_bytes_changed > policy.max_bytes_changed {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "max_bytes_changed exceeds persisted policy".into(),
        });
    }
    if requested.timeout_ms > policy.timeout_ms {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "timeout_ms exceeds persisted policy".into(),
        });
    }
    if match (
        risk_rank(&requested.risk_class),
        risk_rank(&policy.risk_class),
    ) {
        (Some(requested), Some(allowed)) => requested > allowed,
        _ => true,
    } {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "risk_class exceeds persisted policy".into(),
        });
    }
    if requested.allow_create && !policy.allow_create {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "file creation is disabled by persisted policy".into(),
        });
    }
    if requested.allow_delete && !policy.allow_delete {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "file deletion is disabled by persisted policy".into(),
        });
    }
    if requested.allow_rename && !policy.allow_rename {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "file rename is disabled by persisted policy".into(),
        });
    }
    if !policy.allowed_operations.is_empty()
        && requested
            .allowed_operations
            .iter()
            .any(|operation| !policy.allowed_operations.contains(operation))
    {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "requested operation is disabled by persisted policy".into(),
        });
    }
    if !policy.allowed_paths.is_empty()
        && requested.allowed_paths.iter().any(|path| {
            !policy
                .allowed_paths
                .iter()
                .any(|allowed| path == allowed || path.starts_with(&format!("{allowed}/")))
        })
    {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "requested path is outside persisted policy".into(),
        });
    }
    if !violations.is_empty() {
        return Err(violations);
    }
    let mut effective = requested.clone();
    if effective.allowed_operations.is_empty() {
        effective.allowed_operations = policy.allowed_operations.clone();
    }
    if effective.allowed_paths.is_empty() {
        effective.allowed_paths = policy.allowed_paths.clone();
    }
    if effective.allowed_file_types.is_empty() {
        effective.allowed_file_types = policy.allowed_file_types.clone();
    }
    effective.max_files_changed = effective.max_files_changed.min(policy.max_files_changed);
    effective.max_bytes_changed = effective.max_bytes_changed.min(policy.max_bytes_changed);
    effective.timeout_ms = effective.timeout_ms.min(policy.timeout_ms);
    effective.allow_create &= policy.allow_create;
    effective.allow_delete &= policy.allow_delete;
    effective.allow_rename &= policy.allow_rename;
    effective.protected_paths = policy
        .protected_paths
        .iter()
        .chain(requested.protected_paths.iter())
        .cloned()
        .collect();
    Ok(effective)
}

pub fn validate_build_scope(scope: &BuildScope, changes: &[ProposedChange]) -> Vec<ScopeViolation> {
    let mut violations = Vec::new();
    if !matches!(scope.risk_class.as_str(), "low" | "medium" | "high") {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "risk_class is invalid".into(),
        });
    }
    if scope.timeout_ms == 0 || scope.timeout_ms > 300_000 {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "timeout_ms is outside bounded policy".into(),
        });
    }
    if changes.len() > scope.max_files_changed {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "max_files_changed exceeded".into(),
        });
    }
    let total_bytes = changes
        .iter()
        .map(|change| change.bytes_changed)
        .sum::<usize>();
    if total_bytes > scope.max_bytes_changed {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "max_bytes_changed exceeded".into(),
        });
    }
    for change in changes {
        let normalized = change.relative_path.replace('\\', "/");
        if !scope.allowed_operations.is_empty()
            && !scope
                .allowed_operations
                .iter()
                .any(|operation| operation == &change.operation)
        {
            violations.push(ScopeViolation {
                path: normalized.clone(),
                reason: "operation is not allowed".into(),
            });
        }
        let path = Path::new(&normalized);
        if path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            violations.push(ScopeViolation {
                path: normalized.clone(),
                reason: "path escapes workspace".into(),
            });
            continue;
        }
        if scope.protected_paths.iter().any(|protected| {
            normalized == *protected || normalized.starts_with(&format!("{protected}/"))
        }) {
            violations.push(ScopeViolation {
                path: normalized.clone(),
                reason: "protected path".into(),
            });
        }
        if !scope.allowed_paths.is_empty()
            && !scope.allowed_paths.iter().any(|allowed| {
                normalized == *allowed || normalized.starts_with(&format!("{allowed}/"))
            })
        {
            violations.push(ScopeViolation {
                path: normalized.clone(),
                reason: "path is outside allowed_paths".into(),
            });
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !scope.allowed_file_types.is_empty()
            && !scope
                .allowed_file_types
                .iter()
                .any(|allowed| allowed.trim_start_matches('.') == extension)
        {
            violations.push(ScopeViolation {
                path: normalized.clone(),
                reason: "file type is not allowed".into(),
            });
        }
        if change.creates && !scope.allow_create {
            violations.push(ScopeViolation {
                path: normalized.clone(),
                reason: "file creation is disabled".into(),
            });
        }
        if change.deletes && !scope.allow_delete {
            violations.push(ScopeViolation {
                path: normalized,
                reason: "file deletion is disabled".into(),
            });
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::{restrict_to_policy, validate_build_scope, BuildScope, ProposedChange};

    fn scope() -> BuildScope {
        BuildScope {
            allowed_paths: vec!["src".into()],
            allowed_operations: vec!["write".into()],
            expected_outputs: vec!["updated source".into()],
            protected_paths: vec!["src/generated".into()],
            allowed_file_types: vec!["rs".into()],
            max_files_changed: 2,
            max_bytes_changed: 100,
            allow_create: false,
            allow_delete: false,
            allow_rename: false,
            baseline_snapshot_id: None,
            acceptance_criteria: "tests pass".into(),
            risk_class: "medium".into(),
            timeout_ms: 30_000,
        }
    }

    #[test]
    fn rejects_escape_protected_type_and_mutation_violations() {
        let violations = validate_build_scope(
            &scope(),
            &[ProposedChange {
                relative_path: "../Cargo.toml".into(),
                operation: "create".into(),
                bytes_changed: 1,
                creates: true,
                deletes: false,
            }],
        );
        assert!(violations
            .iter()
            .any(|item| item.reason == "path escapes workspace"));
    }

    #[test]
    fn accepts_bounded_allowed_text_change() {
        let violations = validate_build_scope(
            &scope(),
            &[ProposedChange {
                relative_path: "src/lib.rs".into(),
                operation: "write".into(),
                bytes_changed: 10,
                creates: false,
                deletes: false,
            }],
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn rejects_operation_outside_effective_scope() {
        let violations = validate_build_scope(
            &scope(),
            &[ProposedChange {
                relative_path: "src/lib.rs".into(),
                operation: "delete".into(),
                bytes_changed: 0,
                creates: false,
                deletes: true,
            }],
        );
        assert!(violations
            .iter()
            .any(|item| item.reason == "operation is not allowed"));
    }

    #[test]
    fn rejects_unbounded_risk_and_timeout_policy() {
        let mut invalid = scope();
        invalid.risk_class = "unknown".into();
        invalid.timeout_ms = 0;
        let violations = validate_build_scope(&invalid, &[]);
        assert!(violations
            .iter()
            .any(|item| item.reason == "risk_class is invalid"));
        assert!(violations
            .iter()
            .any(|item| item.reason == "timeout_ms is outside bounded policy"));
    }

    #[test]
    fn persisted_policy_cannot_be_widened_by_request() {
        let policy = scope();
        let mut requested = scope();
        requested.max_files_changed = 3;
        requested.timeout_ms = 60_000;
        let violations =
            restrict_to_policy(&policy, &requested).expect_err("policy must reject widening");
        assert!(violations
            .iter()
            .any(|item| item.reason == "max_files_changed exceeds persisted policy"));
        assert!(violations
            .iter()
            .any(|item| item.reason == "timeout_ms exceeds persisted policy"));
    }

    #[test]
    fn empty_requested_lists_inherit_persisted_restrictions() {
        let policy = scope();
        let mut requested = scope();
        requested.allowed_operations.clear();
        requested.allowed_paths.clear();
        let effective = restrict_to_policy(&policy, &requested).expect("request narrows policy");
        assert_eq!(effective.allowed_operations, policy.allowed_operations);
        assert_eq!(effective.allowed_paths, policy.allowed_paths);
    }
}
