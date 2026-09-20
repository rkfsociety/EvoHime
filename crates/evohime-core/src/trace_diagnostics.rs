use serde_json::{Map, Value};
use std::path::{Component, Path};

/// Bounded, path-free metadata for a failed filesystem tool call.
///
/// The raw model argument never crosses the trace boundary. These fields only
/// say how the argument was shaped and which safe workspace scope it referred
/// to, so a trace can distinguish an external absolute path from a bad
/// workspace-relative spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathTraceMetadata {
    pub(crate) path_form: &'static str,
    pub(crate) path_scope: &'static str,
    pub(crate) path_boundary_reason: &'static str,
}

impl PathTraceMetadata {
    pub(crate) fn insert_into(self, object: &mut Map<String, Value>) {
        object.insert("path_form".into(), Value::String(self.path_form.into()));
        object.insert("path_scope".into(), Value::String(self.path_scope.into()));
        object.insert(
            "path_boundary_reason".into(),
            Value::String(self.path_boundary_reason.into()),
        );
    }
}

/// Classifies a filesystem path without reading it and without returning the
/// path itself. The classifier is intentionally independent from the tool
/// error text: the same `permission_denied` can come from several sandbox
/// layers, while the model argument is still available at the telemetry site.
pub(crate) fn classify_tool_path(
    tool_name: &str,
    arguments: &str,
    workspace_root: &Path,
) -> Option<PathTraceMetadata> {
    if !tool_name.starts_with("filesystem.") {
        return None;
    }
    let value = serde_json::from_str::<Value>(arguments).ok()?;
    let path = value.get("path").and_then(Value::as_str)?.trim();
    if path.is_empty() {
        return Some(PathTraceMetadata {
            path_form: "empty",
            path_scope: "unknown",
            path_boundary_reason: "invalid_path",
        });
    }

    let requested = Path::new(path);
    let has_parent_traversal = requested
        .components()
        .any(|component| matches!(component, Component::ParentDir));
    let is_absolute = requested.is_absolute();

    if is_absolute {
        let path_scope = if normalized_components(requested)
            .as_deref()
            .zip(normalized_components(workspace_root).as_deref())
            .is_some_and(|(requested, root)| starts_with_components(requested, root))
        {
            "workspace"
        } else {
            "outside_workspace"
        };
        return Some(PathTraceMetadata {
            path_form: "absolute",
            path_scope,
            path_boundary_reason: "absolute_path_not_allowed",
        });
    }

    if has_parent_traversal {
        return Some(PathTraceMetadata {
            path_form: "relative",
            path_scope: "unknown",
            path_boundary_reason: "parent_traversal",
        });
    }

    if is_secret_path(requested) {
        return Some(PathTraceMetadata {
            path_form: "relative",
            path_scope: "workspace",
            path_boundary_reason: "secret_path",
        });
    }

    let path_form = if logical_namespace(path) {
        "logical"
    } else {
        "relative"
    };
    let path_scope = if path_form == "logical" {
        "workspace_namespace"
    } else {
        "workspace"
    };
    Some(PathTraceMetadata {
        path_form,
        path_scope,
        path_boundary_reason: "not_path_specific",
    })
}

fn logical_namespace(path: &str) -> bool {
    matches!(
        path.replace('\\', "/").split('/').next(),
        Some("uploads" | "workspace" | "outputs" | "scratch")
    )
}

fn is_secret_path(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(value) = component else {
            return false;
        };
        let value = value.to_string_lossy().to_ascii_lowercase();
        value == ".env" || value.starts_with(".env.")
    })
}

fn normalized_components(path: &Path) -> Option<Vec<String>> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => components.push(format!(
                "prefix:{}",
                prefix.as_os_str().to_string_lossy().to_ascii_lowercase()
            )),
            Component::RootDir => components.push("root".into()),
            Component::CurDir => {}
            Component::ParentDir => {
                if components
                    .last()
                    .is_some_and(|last| last != "root" && !last.starts_with("prefix:"))
                {
                    components.pop();
                } else {
                    components.push("parent".into());
                }
            }
            Component::Normal(value) => {
                components.push(value.to_string_lossy().to_ascii_lowercase())
            }
        }
    }
    (!components.is_empty()).then_some(components)
}

fn starts_with_components(path: &[String], root: &[String]) -> bool {
    path.len() >= root.len() && path.iter().zip(root).all(|(path, root)| path == root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\work\Bombox")
        } else {
            PathBuf::from("/work/Bombox")
        }
    }

    fn external_path() -> &'static str {
        if cfg!(windows) {
            r#"C:\Users\roman\Downloads\evohime-trace.md"#
        } else {
            "/tmp/evohime-trace.md"
        }
    }

    #[test]
    fn distinguishes_absolute_external_path_without_retaining_it() {
        let metadata = classify_tool_path(
            "filesystem.read",
            &serde_json::json!({"path": external_path()}).to_string(),
            &workspace(),
        )
        .expect("filesystem path metadata");
        assert_eq!(metadata.path_form, "absolute");
        assert_eq!(metadata.path_scope, "outside_workspace");
        assert_eq!(metadata.path_boundary_reason, "absolute_path_not_allowed");
    }

    #[test]
    fn distinguishes_absolute_path_inside_workspace() {
        let path = if cfg!(windows) {
            r"C:\work\Bombox\README.md"
        } else {
            "/work/Bombox/README.md"
        };
        let metadata = classify_tool_path(
            "filesystem.read",
            &serde_json::json!({"path": path}).to_string(),
            &workspace(),
        )
        .expect("filesystem path metadata");
        assert_eq!(metadata.path_form, "absolute");
        assert_eq!(metadata.path_scope, "workspace");
        assert_eq!(metadata.path_boundary_reason, "absolute_path_not_allowed");
    }

    #[test]
    fn identifies_traversal_secret_and_logical_paths() {
        let traversal = classify_tool_path(
            "filesystem.read",
            r#"{"path":"workspace/../secret.txt"}"#,
            &workspace(),
        )
        .unwrap();
        assert_eq!(traversal.path_boundary_reason, "parent_traversal");

        let secret = classify_tool_path(
            "filesystem.read",
            r#"{"path":"config/.env.local"}"#,
            &workspace(),
        )
        .unwrap();
        assert_eq!(secret.path_boundary_reason, "secret_path");

        let logical = classify_tool_path(
            "filesystem.read",
            r#"{"path":"uploads/trace.md"}"#,
            &workspace(),
        )
        .unwrap();
        assert_eq!(logical.path_form, "logical");
        assert_eq!(logical.path_scope, "workspace_namespace");
        assert_eq!(logical.path_boundary_reason, "not_path_specific");
    }

    #[test]
    fn ignores_non_filesystem_tools_and_malformed_arguments() {
        assert!(classify_tool_path("shell.execute", r#"{"path":"x"}"#, &workspace()).is_none());
        assert!(classify_tool_path("filesystem.read", "not-json", &workspace()).is_none());
    }
}
