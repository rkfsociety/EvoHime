use crate::ToolError;
use evohime_permissions::Permission;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WorkspaceSandbox {
    root: PathBuf,
}

impl WorkspaceSandbox {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, ToolError> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|e| ToolError::Execution(format!("workspace root invalid: {e}")))?;
        if !root.is_dir() {
            return Err(ToolError::Execution(
                "workspace root is not a directory".into(),
            ));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve_existing(&self, path: &str) -> Result<PathBuf, ToolError> {
        self.resolve_existing_for_tool(path, "workspace")
    }

    pub fn resolve_existing_for_tool(&self, path: &str, tool: &str) -> Result<PathBuf, ToolError> {
        let candidate = self.root.join(path);
        let resolved = match candidate.canonicalize() {
            Ok(resolved) => resolved,
            Err(error) => {
                if let Some(recovered) = self.find_unique_suffix(path) {
                    return self.ensure_inside(recovered, Permission::FilesystemRead);
                }
                return Err(ToolError::NotFound {
                    tool: tool.to_string(),
                    path: path.to_string(),
                    hint: format!("; {error}; {}", self.recovery_hint(&candidate, path)),
                });
            }
        };
        self.ensure_inside(resolved, Permission::FilesystemRead)
    }

    pub fn resolve_for_write(&self, path: &str) -> Result<PathBuf, ToolError> {
        let candidate = self.root.join(path);
        let parent = candidate.parent().ok_or_else(|| ToolError::InvalidInput {
            tool: "filesystem.write".into(),
            message: "path has no parent".into(),
        })?;
        let mut existing_parent = parent;
        while !existing_parent.exists() {
            existing_parent = existing_parent
                .parent()
                .ok_or(ToolError::PermissionDenied(Permission::FilesystemWrite))?;
        }
        let canonical_parent = existing_parent
            .canonicalize()
            .map_err(|e| ToolError::Execution(format!("parent path invalid: {e}")))?;
        let suffix = parent
            .strip_prefix(existing_parent)
            .map_err(|_| ToolError::PermissionDenied(Permission::FilesystemWrite))?;
        let resolved = canonical_parent
            .join(suffix)
            .join(
                candidate
                    .file_name()
                    .ok_or_else(|| ToolError::InvalidInput {
                        tool: "filesystem.write".into(),
                        message: "path is empty".into(),
                    })?,
            );
        self.ensure_inside(resolved, Permission::FilesystemWrite)
    }

    fn ensure_inside(&self, path: PathBuf, permission: Permission) -> Result<PathBuf, ToolError> {
        if path.starts_with(&self.root) {
            Ok(path)
        } else {
            Err(ToolError::PermissionDenied(permission))
        }
    }

    fn recovery_hint(&self, candidate: &Path, requested: &str) -> String {
        let mut directory = candidate.parent().unwrap_or(&self.root);
        while !directory.exists() && directory != self.root {
            directory = directory.parent().unwrap_or(&self.root);
        }
        let mut entries = std::fs::read_dir(directory)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .take(32)
            .collect::<Vec<_>>();
        entries.sort_unstable();
        let relative_directory = directory
            .strip_prefix(&self.root)
            .ok()
            .and_then(|path| path.to_str())
            .filter(|path| !path.is_empty())
            .unwrap_or(".");
        let available = if entries.is_empty() {
            "(каталог пуст или недоступен)".to_string()
        } else {
            entries.join(", ")
        };
        format!(
            "запрошенный workspace-relative путь `{requested}` не найден; проверь путь через filesystem.list для `{relative_directory}`. Доступные записи: {available}"
        )
    }

    fn find_unique_suffix(&self, requested: &str) -> Option<PathBuf> {
        let requested = Path::new(requested);
        if requested.is_absolute() {
            return None;
        }
        let mut pending = vec![self.root.clone()];
        let mut matches = Vec::new();
        while let Some(directory) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_symlink() {
                    continue;
                }
                let is_match = path
                    .strip_prefix(&self.root)
                    .map(|relative| relative.ends_with(requested))
                    .unwrap_or(false);
                if is_match {
                    if let Ok(resolved) = path.canonicalize() {
                        matches.push(resolved);
                        if matches.len() > 1 {
                            return None;
                        }
                    }
                }
                if file_type.is_dir() {
                    pending.push(path);
                }
            }
        }
        matches.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolves_existing_inside_root_and_rejects_traversal() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        let sandbox = WorkspaceSandbox::new(dir.path()).unwrap();
        assert!(sandbox.resolve_existing("a.txt").is_ok());
        assert!(matches!(
            sandbox.resolve_existing(".."),
            Err(ToolError::PermissionDenied(Permission::FilesystemRead))
        ));
    }

    #[test]
    fn allows_new_file_with_existing_parent() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        let sandbox = WorkspaceSandbox::new(dir.path()).unwrap();
        assert!(sandbox
            .resolve_for_write("nested/new.txt")
            .unwrap()
            .ends_with(Path::new("nested").join("new.txt")));
    }

    #[test]
    fn invalid_path_error_contains_recovery_directory_and_entries() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("docs")).unwrap();
        fs::write(dir.path().join("docs/plan.md"), "plan").unwrap();
        let sandbox = WorkspaceSandbox::new(dir.path()).unwrap();
        let error = sandbox
            .resolve_existing("docs/missing/plan.md")
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("filesystem.list"));
        assert!(message.contains("docs"));
        assert!(message.contains("plan.md"));
    }

    #[test]
    fn resolves_unique_workspace_suffix_without_hiding_ambiguity() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("crates/demo/src")).unwrap();
        fs::write(dir.path().join("crates/demo/src/main.rs"), "main").unwrap();
        let sandbox = WorkspaceSandbox::new(dir.path()).unwrap();
        assert!(sandbox
            .resolve_existing("demo/src/main.rs")
            .unwrap()
            .ends_with(Path::new("crates/demo/src/main.rs")));

        fs::create_dir_all(dir.path().join("desktop/demo/src")).unwrap();
        fs::write(dir.path().join("desktop/demo/src/main.rs"), "main").unwrap();
        assert!(sandbox.resolve_existing("demo/src/main.rs").is_err());
    }
}
