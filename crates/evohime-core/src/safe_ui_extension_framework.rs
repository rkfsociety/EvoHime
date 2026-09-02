//! Core-owned declarative UI extension lifecycle (plan 76).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_MANIFEST: usize = 64 * 1024;
pub const MAX_CONTRIBUTIONS: usize = 128;
pub const MAX_SOURCES: usize = 64;
pub const MAX_ACTIONS: usize = 64;
pub const MAX_FILES: usize = 32;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Lifecycle {
    Discovered,
    InstalledDisabled,
    Enabled,
    Disabled,
    UpdateAvailable,
    Updating,
    Broken,
    Uninstalled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContributionKind {
    Page,
    ConversationPanel,
    SidebarItem,
    StatusCard,
    ArtifactVisualizer,
    Theme,
    SettingsSection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contribution {
    pub id: String,
    pub kind: ContributionKind,
    pub title: String,
    pub route_key: String,
    pub data_sources: Vec<String>,
    pub actions: Vec<String>,
    pub layout: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiExtensionManifest {
    pub schema_version: u32,
    pub id: String,
    pub display_name: String,
    pub version: u32,
    pub description: Option<String>,
    pub host_api_version: u32,
    pub contributions: Vec<Contribution>,
    pub required_projection_capabilities: Vec<String>,
    pub optional_projection_capabilities: Vec<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledUiExtension {
    pub manifest: UiExtensionManifest,
    pub lifecycle: Lifecycle,
    pub scope: String,
    pub resolved_revision: String,
    pub manifest_hash: String,
    pub trust_state: String,
    pub compatibility_state: String,
    pub revision: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid extension manifest: {0}")]
    Invalid(String),
    #[error("unsupported manifest version")]
    UnsupportedVersion,
    #[error("extension must be disabled after install")]
    InstallMustBeDisabled,
    #[error("unknown projection or action")]
    UnknownBinding,
    #[error("path traversal")]
    PathTraversal,
    #[error("stale revision")]
    StaleRevision,
    #[error("capability delta requires review")]
    CapabilityDelta,
}

fn bounded_text(value: &str, max_bytes: usize) -> Result<(), Error> {
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        Err(Error::Invalid("bounded text".to_string()))
    } else {
        Ok(())
    }
}

pub fn hash<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

pub fn validate(manifest: &UiExtensionManifest) -> Result<(), Error> {
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(Error::UnsupportedVersion);
    }
    bounded_text(&manifest.id, 128)?;
    bounded_text(&manifest.display_name, 128)?;
    if let Some(description) = manifest.description.as_deref() {
        bounded_text(description, 4 * 1024)?;
    }
    if manifest.contributions.len() > MAX_CONTRIBUTIONS
        || manifest.required_projection_capabilities.len() > MAX_SOURCES
        || manifest.optional_projection_capabilities.len() > MAX_SOURCES
    {
        return Err(Error::Invalid("manifest bounds".to_string()));
    }
    let manifest_bytes =
        serde_json::to_vec(manifest).map_err(|error| Error::Invalid(error.to_string()))?;
    if manifest_bytes.len() > MAX_MANIFEST {
        return Err(Error::Invalid("manifest bytes".to_string()));
    }
    let mut ids = BTreeSet::new();
    for contribution in &manifest.contributions {
        bounded_text(&contribution.id, 128)?;
        bounded_text(&contribution.title, 128)?;
        bounded_text(&contribution.route_key, 128)?;
        if !ids.insert(&contribution.id) {
            return Err(Error::Invalid("duplicate contribution".to_string()));
        }
        if contribution.data_sources.len() > MAX_SOURCES || contribution.actions.len() > MAX_ACTIONS
        {
            return Err(Error::Invalid("contribution bounds".to_string()));
        }
        let layout_bytes = serde_json::to_vec(&contribution.layout)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        if layout_bytes.len() > 256 * 1024 {
            return Err(Error::Invalid("layout bounds".to_string()));
        }
        if contribution.route_key.contains("..") || contribution.id.contains("..") {
            return Err(Error::PathTraversal);
        }
        if contribution
            .data_sources
            .iter()
            .chain(contribution.actions.iter())
            .any(|binding| {
                binding.contains("arbitrary")
                    || binding.contains("shell")
                    || binding.contains("filesystem")
            })
        {
            return Err(Error::UnknownBinding);
        }
    }
    Ok(())
}

pub fn install(
    manifest: UiExtensionManifest,
    scope: &str,
    resolved_revision: &str,
) -> Result<InstalledUiExtension, Error> {
    validate(&manifest)?;
    bounded_text(scope, 128)?;
    bounded_text(resolved_revision, 256)?;
    let manifest_hash = hash(&manifest);
    Ok(InstalledUiExtension {
        manifest,
        lifecycle: Lifecycle::InstalledDisabled,
        scope: scope.to_string(),
        resolved_revision: resolved_revision.to_string(),
        manifest_hash,
        trust_state: "built_in_declarative".to_string(),
        compatibility_state: "compatible".to_string(),
        revision: 1,
    })
}

pub fn transition(
    extension: &mut InstalledUiExtension,
    target: Lifecycle,
    expected_revision: u64,
) -> Result<(), Error> {
    if extension.revision != expected_revision {
        return Err(Error::StaleRevision);
    }
    let allowed = matches!(
        (&extension.lifecycle, &target),
        (
            Lifecycle::InstalledDisabled | Lifecycle::Disabled,
            Lifecycle::Enabled
        ) | (Lifecycle::Enabled, Lifecycle::Disabled)
    );
    if !allowed {
        return Err(Error::Invalid("invalid lifecycle transition".to_string()));
    }
    extension.lifecycle = target;
    extension.revision += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> UiExtensionManifest {
        UiExtensionManifest {
            schema_version: SCHEMA_VERSION,
            id: "org.example.ui".to_string(),
            display_name: "Example".to_string(),
            version: 1,
            description: None,
            host_api_version: 1,
            contributions: vec![Contribution {
                id: "page".to_string(),
                kind: ContributionKind::Page,
                title: "Page".to_string(),
                route_key: "page".to_string(),
                data_sources: vec!["CurrentRunStatus".to_string()],
                actions: vec![],
                layout: serde_json::json!({"type": "Text"}),
            }],
            required_projection_capabilities: vec![],
            optional_projection_capabilities: vec![],
            content_hash: String::new(),
        }
    }
    #[test]
    fn install_is_disabled_and_lifecycle_is_fenced() {
        let mut extension = install(manifest(), "workspace", "rev1").expect("install");
        assert_eq!(extension.lifecycle, Lifecycle::InstalledDisabled);
        transition(&mut extension, Lifecycle::Enabled, 1).expect("enable");
        assert_eq!(
            transition(&mut extension, Lifecycle::Disabled, 1),
            Err(Error::StaleRevision)
        );
    }
    #[test]
    fn rejects_unrestricted_binding() {
        let mut extension = manifest();
        extension.contributions[0]
            .actions
            .push("shell.exec".to_string());
        assert_eq!(validate(&extension), Err(Error::UnknownBinding));
    }
}
