use serde::Serialize;
use std::path::{Component, Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildScope {
    pub allowed_paths: Vec<String>,
    pub protected_paths: Vec<String>,
    pub allowed_file_types: Vec<String>,
    pub max_files_changed: usize,
    pub max_bytes_changed: usize,
    pub allow_create: bool,
    pub allow_delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposedChange {
    pub relative_path: String,
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
            protected_paths: vec!["src/generated".into()],
            allowed_file_types: vec!["rs".into()],
            max_files_changed: 2,
            max_bytes_changed: 100,
            allow_create: false,
            allow_delete: false,
        }
    }

    #[test]
    fn rejects_escape_protected_type_and_mutation_violations() {
        let violations = validate_build_scope(&scope(), &[ProposedChange {
            relative_path: "../Cargo.toml".into(),
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
            bytes_changed: 10,
            creates: false,
            deletes: false,
        }]);
        assert!(violations.is_empty());
    }
}
