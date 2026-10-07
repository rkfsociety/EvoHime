//! Owner-only runtime paths for Linux local IPC.

use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Stable paths shared by the Linux Core server and `eva` client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnixRuntimePaths {
    /// Private directory containing the socket and launch context.
    pub directory: PathBuf,
    /// Unix domain socket used by the CLI and Core.
    pub socket: PathBuf,
    /// Protected session context containing the CLI authentication secret.
    pub launch_context: PathBuf,
}

impl UnixRuntimePaths {
    /// Resolves and prepares this user's private EvoHime runtime directory.
    ///
    /// Uses `XDG_RUNTIME_DIR` when set and otherwise `$HOME/.cache`. The
    /// application directory is created with mode `0700` and existing
    /// directories are tightened to the same mode.
    pub fn current() -> io::Result<Self> {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "XDG_RUNTIME_DIR or HOME is required for Linux IPC",
                )
            })?;
        Self::prepare(&base.join("evohime"))
    }

    /// Creates runtime paths in a specific directory, primarily for tests.
    pub fn prepare(directory: &Path) -> io::Result<Self> {
        if !directory.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Linux runtime directory must be absolute",
            ));
        }
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        match builder.create(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }

        let metadata = std::fs::symlink_metadata(directory)?;
        if !metadata.file_type().is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Linux runtime path must be a real directory",
            ));
        }
        if metadata.mode() & 0o077 != 0 {
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
        }

        let paths = Self {
            directory: directory.to_path_buf(),
            socket: directory.join("core.sock"),
            launch_context: directory.join("session.json"),
        };
        if paths.socket.as_os_str().as_bytes().len() >= 108 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Linux runtime socket path exceeds the Unix domain socket limit",
            ));
        }
        Ok(paths)
    }
}

/// Reads an owner-only launch context and rejects links or overly broad modes.
pub fn read_private_launch_context(path: &Path) -> io::Result<crate::session::LaunchContext> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Linux launch context must be a private regular file",
        ));
    }
    crate::session::read_launch_context(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn prepares_private_runtime_paths() {
        let root = test_directory("runtime-paths");
        let paths =
            UnixRuntimePaths::prepare(&root.join("runtime")).expect("private runtime paths");
        assert_eq!(
            paths.socket.file_name().and_then(|name| name.to_str()),
            Some("core.sock")
        );
        let mode = std::fs::metadata(paths.directory)
            .expect("runtime metadata")
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_runtime_directory_symlinks() {
        let root = test_directory("runtime-symlink");
        let actual = root.join("actual");
        std::fs::create_dir(&actual).expect("actual directory");
        let link = root.join("runtime-link");
        symlink(&actual, &link).expect("directory symlink");
        assert_eq!(
            UnixRuntimePaths::prepare(&link).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_non_private_or_linked_contexts() {
        let root = test_directory("runtime-context");
        let path = root.join("session.json");
        std::fs::write(&path, b"{}").expect("context file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("broad permissions");
        assert_eq!(
            read_private_launch_context(&path).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );

        let linked = root.join("session-link.json");
        symlink(&path, &linked).expect("context symlink");
        assert_eq!(
            read_private_launch_context(&linked).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        let _ = std::fs::remove_dir_all(root);
    }

    fn test_directory(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("temporary test directory");
        path
    }
}
