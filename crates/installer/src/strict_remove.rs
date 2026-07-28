//! Строгое удаление незавершённой установки с точным путём ошибки.

use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[derive(Debug, thiserror::Error)]
#[error("не удалось удалить {path}: {source}")]
pub struct StrictRemoveError {
    path: PathBuf,
    #[source]
    source: std::io::Error,
}

impl StrictRemoveError {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn source_error(&self) -> &std::io::Error {
        &self.source
    }

    fn at(path: &Path, source: std::io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            source,
        }
    }
}

pub fn remove_tree_once(root: &Path) -> Result<bool, StrictRemoveError> {
    if !root.exists() {
        return Ok(false);
    }
    remove_entry(root)?;
    Ok(true)
}

pub async fn remove_tree_with_retries(
    root: &Path,
    attempts: usize,
    delay: Duration,
) -> Result<bool, StrictRemoveError> {
    if attempts == 0 {
        return Err(StrictRemoveError::at(
            root,
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "число попыток удаления должно быть больше нуля",
            ),
        ));
    }

    let mut last_error = None;
    for attempt in 0..attempts {
        match remove_tree_once(root) {
            Ok(removed) => return Ok(removed || !root.exists()),
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < attempts {
            tokio::time::sleep(delay).await;
        }
    }
    Err(last_error.expect("attempts is known to be non-zero"))
}

fn remove_entry(path: &Path) -> Result<(), StrictRemoveError> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| StrictRemoveError::at(path, error))?;
    let is_reparse_point = metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;

    if metadata.is_dir() && !is_reparse_point {
        let entries =
            std::fs::read_dir(path).map_err(|error| StrictRemoveError::at(path, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| StrictRemoveError::at(path, error))?;
            remove_entry(&entry.path())?;
        }
        std::fs::remove_dir(path).map_err(|error| StrictRemoveError::at(path, error))
    } else if metadata.is_dir() {
        std::fs::remove_dir(path).map_err(|error| StrictRemoveError::at(path, error))
    } else {
        std::fs::remove_file(path).map_err(|error| StrictRemoveError::at(path, error))
    }
}
