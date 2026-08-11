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

pub fn validate_build_scope(scope: &BuildScope, changes: &[ProposedChange]) -> Vec<ScopeViolation> {
    let mut violations = Vec::new();
    if !matches!(scope.risk_class.as_str(), "low" | "medium" | "high") {
        violations.push(ScopeViolation { path: String::new(), reason: "risk_class is invalid".into() });
    }
    if scope.timeout_ms == 0 || scope.timeout_ms > 300_000 {
        violations.push(ScopeViolation { path: String::new(), reason: "timeout_ms is outside bounded policy".into() });
    }
    if changes.len() > scope.max_files_changed {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "max_files_changed exceeded".into(),
        });
    }
    let total_bytes = changes.iter().map(|change| change.bytes_changed).sum::<usize>();
    if total_bytes > scope.max_bytes_changed {
        violations.push(ScopeViolation {
            path: String::new(),
            reason: "max_bytes_changed exceeded".into(),
        });
    }
    for change in changes {
        let normalized = change.relative_path.replace('\\', "/");
        if !scope.allowed_operations.is_empty()
            && !scope.allowed_operations.iter().any(|operation| operation == &change.operation)
        {
            violations.push(ScopeViolation { path: normalized.clone(), reason: "operation is not allowed".into() });
        }
        let path = Path::new(&normalized);
        if path.components().any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
            violations.push(ScopeViolation { path: normalized.clone(), reason: "path escapes workspace".into() });
            continue;
        }
        if scope.protected_paths.iter().any(|protected| normalized == *protected || normalized.starts_with(&format!("{protected}/"))) {
            violations.push(ScopeViolation { path: normalized.clone(), reason: "protected path".into() });
        }
        if !scope.allowed_paths.is_empty() && !scope.allowed_paths.iter().any(|allowed| normalized == *allowed || normalized.starts_with(&format!("{allowed}/"))) {
            violations.push(ScopeViolation { path: normalized.clone(), reason: "path is outside allowed_paths".into() });
        }
        let extension = path.extension().and_then(|value| value.to_str()).unwrap_or_default();
        if !scope.allowed_file_types.is_empty() && !scope.allowed_file_types.iter().any(|allowed| allowed.trim_start_matches('.') == extension) {
            violations.push(ScopeViolation { path: normalized.clone(), reason: "file type is not allowed".into() });
        }
        if change.creates && !scope.allow_create {
            violations.push(ScopeViolation { path: normalized.clone(), reason: "file creation is disabled".into() });
        }
        if change.deletes && !scope.allow_delete {
            violations.push(ScopeViolation { path: normalized, reason: "file deletion is disabled".into() });
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::{validate_build_scope, BuildScope, ProposedChange};

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
        let violations = validate_build_scope(&scope(), &[ProposedChange {
            relative_path: "../Cargo.toml".into(),
            operation: "create".into(),
            bytes_changed: 1,
            creates: true,
            deletes: false,
        }]);
        assert!(violations.iter().any(|item| item.reason == "path escapes workspace"));
    }

    #[test]
    fn accepts_bounded_allowed_text_change() {
        let violations = validate_build_scope(&scope(), &[ProposedChange {
            relative_path: "src/lib.rs".into(),
            operation: "write".into(),
            bytes_changed: 10,
            creates: false,
            deletes: false,
        }]);
        assert!(violations.is_empty());
    }

    #[test]
    fn rejects_operation_outside_effective_scope() {
        let violations = validate_build_scope(&scope(), &[ProposedChange {
            relative_path: "src/lib.rs".into(),
            operation: "delete".into(),
            bytes_changed: 0,
            creates: false,
            deletes: true,
        }]);
        assert!(violations.iter().any(|item| item.reason == "operation is not allowed"));
    }

    #[test]
    fn rejects_unbounded_risk_and_timeout_policy() {
        let mut invalid = scope();
        invalid.risk_class = "unknown".into();
        invalid.timeout_ms = 0;
        let violations = validate_build_scope(&invalid, &[]);
        assert!(violations.iter().any(|item| item.reason == "risk_class is invalid"));
        assert!(violations.iter().any(|item| item.reason == "timeout_ms is outside bounded policy"));
    }
}
