use crate::ToolManifest;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_TOOLKITS: usize = 256;
pub const MAX_VERSIONS: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolkitStatus {
    Available,
    Enabled,
    Disabled,
    Quarantined,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolkitEntry {
    pub toolkit_id: String,
    pub version: String,
    pub manifest_hash: String,
    pub source: String,
    pub package_hash: Option<String>,
    pub license: Option<String>,
    pub status: ToolkitStatus,
    pub compatible_core: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ToolkitError {
    #[error("catalog limit exceeded")]
    Limit,
    #[error("manifest is invalid: {0}")]
    Manifest(String),
    #[error("toolkit is not enabled")]
    NotEnabled,
    #[error("toolkit is quarantined")]
    Quarantined,
    #[error("version is not available for rollback")]
    NoRollback,
}

#[derive(Debug, Default, Clone)]
pub struct ToolkitCatalog {
    entries: Vec<ToolkitEntry>,
}

impl ToolkitCatalog {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn discover(
        &mut self,
        manifest: &ToolManifest,
        source: impl Into<String>,
    ) -> Result<ToolkitEntry, ToolkitError> {
        manifest
            .validate()
            .map_err(|e| ToolkitError::Manifest(e.to_string()))?;
        if self.entries.len() >= MAX_TOOLKITS {
            return Err(ToolkitError::Limit);
        }
        let entry = ToolkitEntry {
            toolkit_id: manifest.tool_id.clone(),
            version: manifest.version.clone(),
            manifest_hash: manifest
                .canonical_hash()
                .map_err(|e| ToolkitError::Manifest(e.to_string()))?,
            source: source.into(),
            package_hash: manifest.package_hash.clone(),
            license: manifest.license.clone(),
            status: ToolkitStatus::Available,
            compatible_core: manifest.compatible_core.clone(),
            tools: vec![manifest.tool_id.clone()],
        };
        self.entries.push(entry.clone());
        Ok(entry)
    }
    pub fn list(&self) -> &[ToolkitEntry] {
        &self.entries
    }
    pub fn set_status(
        &mut self,
        id: &str,
        version: &str,
        status: ToolkitStatus,
    ) -> Result<(), ToolkitError> {
        let e = self
            .entries
            .iter_mut()
            .find(|e| e.toolkit_id == id && e.version == version)
            .ok_or(ToolkitError::NoRollback)?;
        if matches!(e.status, ToolkitStatus::Quarantined)
            && matches!(status, ToolkitStatus::Enabled)
        {
            return Err(ToolkitError::Quarantined);
        }
        e.status = status;
        Ok(())
    }
    pub fn executable(&self, id: &str, version: &str) -> Result<&ToolkitEntry, ToolkitError> {
        let e = self
            .entries
            .iter()
            .find(|e| e.toolkit_id == id && e.version == version)
            .ok_or(ToolkitError::NoRollback)?;
        if !matches!(e.status, ToolkitStatus::Enabled) {
            return Err(if matches!(e.status, ToolkitStatus::Quarantined) {
                ToolkitError::Quarantined
            } else {
                ToolkitError::NotEnabled
            });
        }
        Ok(e)
    }
}
