use crate::ToolManifest;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum number of toolkit versions held in the catalog.
pub const MAX_TOOLKITS: usize = 256;
/// Maximum supported versions for one toolkit identity.
pub const MAX_VERSIONS: usize = 32;

/// Availability and execution state of a discovered toolkit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolkitStatus {
    /// Discovered and valid, but not enabled for execution.
    Available,
    /// Explicitly enabled and eligible for execution.
    Enabled,
    /// Explicitly disabled and ineligible for execution.
    Disabled,
    /// Isolated after a security or integrity concern.
    Quarantined,
    /// Known to the catalog but unavailable in the current environment.
    Unavailable,
}

/// Manifest metadata for one discovered toolkit version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolkitEntry {
    /// Stable toolkit identity.
    pub toolkit_id: String,
    /// Version of this toolkit entry.
    pub version: String,
    /// Digest of the canonical tool manifest.
    pub manifest_hash: String,
    /// Source from which this toolkit was discovered.
    pub source: String,
    /// Optional digest of the containing package.
    pub package_hash: Option<String>,
    /// Optional package license identifier.
    pub license: Option<String>,
    /// Current enablement and quarantine state.
    pub status: ToolkitStatus,
    /// Core version constraint declared by the toolkit.
    pub compatible_core: String,
    /// Tool identifiers supplied by the toolkit.
    pub tools: Vec<String>,
}

/// Catalog validation, capacity, and execution eligibility errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ToolkitError {
    /// The catalog has reached its configured capacity.
    #[error("catalog limit exceeded")]
    Limit,
    /// The tool manifest failed validation or hashing.
    #[error("manifest is invalid: {0}")]
    Manifest(String),
    /// The requested toolkit has not been enabled.
    #[error("toolkit is not enabled")]
    NotEnabled,
    /// A quarantined toolkit cannot be enabled or executed.
    #[error("toolkit is quarantined")]
    Quarantined,
    /// The requested toolkit identity and version were not found.
    #[error("version is not available for rollback")]
    NoRollback,
}

/// In-memory catalog of discovered toolkit manifests and their status.
#[derive(Debug, Default, Clone)]
pub struct ToolkitCatalog {
    entries: Vec<ToolkitEntry>,
}

impl ToolkitCatalog {
    /// Creates an empty catalog.
    pub fn new() -> Self {
        Self::default()
    }
    /// Validates and adds a manifest as an available, disabled toolkit.
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
    /// Returns entries in discovery order.
    pub fn list(&self) -> &[ToolkitEntry] {
        &self.entries
    }
    /// Changes entry status while preventing quarantined entries from re-enabling.
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
    /// Returns an enabled entry or an error describing why it cannot execute.
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
