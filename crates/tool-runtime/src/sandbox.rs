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
        let candidate = self.root.join(path);
        let resolved = candidate
            .canonicalize()
            .map_err(|e| ToolError::Execution(format!("path invalid: {e}")))?;
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
}
