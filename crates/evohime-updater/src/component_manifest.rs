use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const SCHEMA: &str = "evohime.component-manifest.v1";
pub const MAX_COMPONENTS: usize = 32;
pub const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub product: String,
    pub release_id: String,
    pub os: String,
    pub architecture: String,
    pub release_commit: String,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub version: String,
    pub artifact: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub required: bool,
    pub protocol: String,
    pub restart: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("invalid manifest JSON: {0}")]
    Json(String),
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let manifest: Self =
            serde_json::from_slice(bytes).map_err(|e| ManifestError::Json(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema != SCHEMA
            || self.product != "EvoHime"
            || self.os != "windows"
            || self.architecture != "x64"
            || self.release_commit.len() != 40
            || !self
                .release_commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ManifestError::Invalid(
                "unsupported product, schema, platform or architecture".into(),
            ));
        }
        if self.components.is_empty() || self.components.len() > MAX_COMPONENTS {
            return Err(ManifestError::Invalid(
                "component count is outside bounds".into(),
            ));
        }
        let mut ids = HashSet::new();
        let mut by_id = HashMap::new();
        for component in &self.components {
            if !ids.insert(&component.id) {
                return Err(ManifestError::Invalid("duplicate component id".into()));
            }
            if component.id.is_empty()
                || component.id.len() > 64
                || component.version.is_empty()
                || component.version.len() > 64
                || component.artifact.is_empty()
                || component.artifact.len() > 260
            {
                return Err(ManifestError::Invalid(
                    "component identity is outside bounds".into(),
                ));
            }
            let path = Path::new(&component.path);
            let artifact = Path::new(&component.artifact);
            let unsafe_path = |value: &str, path: &Path| {
                path.is_absolute()
                    || value.contains("..")
                    || value.contains('\\')
                    || value.contains(':')
                    || value.starts_with('/')
            };
            if unsafe_path(&component.path, path) || unsafe_path(&component.artifact, artifact) {
                return Err(ManifestError::Invalid(format!(
                    "unsafe component path or artifact: {}",
                    component.path
                )));
            }
            if component.size == 0
                || component.size > MAX_ARTIFACT_BYTES
                || component.sha256.len() != 64
                || !component.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(ManifestError::Invalid(format!(
                    "invalid artifact bounds or hash for {}",
                    component.id
                )));
            }
            if component.protocol.len() > 64 || component.restart.len() > 32 {
                return Err(ManifestError::Invalid(
                    "component metadata is too long".into(),
                ));
            }
            by_id.insert(component.id.as_str(), component);
        }
        for component in &self.components {
            for dependency in &component.dependencies {
                if !by_id.contains_key(dependency.as_str()) {
                    return Err(ManifestError::Invalid(format!(
                        "missing dependency: {dependency}"
                    )));
                }
            }
        }
        fn visit(
            id: &str,
            by_id: &HashMap<&str, &Component>,
            visiting: &mut HashSet<String>,
            done: &mut HashSet<String>,
        ) -> bool {
            if done.contains(id) {
                return false;
            }
            if !visiting.insert(id.to_owned()) {
                return true;
            }
            let cycle = by_id[id]
                .dependencies
                .iter()
                .any(|d| visit(d, by_id, visiting, done));
            visiting.remove(id);
            done.insert(id.to_owned());
            cycle
        }
        let mut visiting = HashSet::new();
        let mut done = HashSet::new();
        if self
            .components
            .iter()
            .any(|c| visit(&c.id, &by_id, &mut visiting, &mut done))
        {
            return Err(ManifestError::Invalid("dependency cycle".into()));
        }
        Ok(())
    }

    /// Deterministic bytes used for release identity and journal binding.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ManifestError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| ManifestError::Json(error.to_string()))
    }

    pub fn component(&self, id: &str) -> Result<&Component, ManifestError> {
        self.components
            .iter()
            .find(|component| component.id == id)
            .ok_or_else(|| ManifestError::Invalid(format!("unknown component: {id}")))
    }

    pub fn artifact_matches(&self, component: &Component, bytes: &[u8]) -> bool {
        if bytes.len() as u64 != component.size {
            return false;
        }
        let digest = Sha256::digest(bytes);
        hex::encode(digest) == component.sha256.to_ascii_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        Manifest {
            schema: SCHEMA.into(),
            product: "EvoHime".into(),
            release_id: "r1".into(),
            os: "windows".into(),
            architecture: "x64".into(),
            release_commit: "a".repeat(40),
            components: vec![Component {
                id: "ui-bundle".into(),
                version: "1".into(),
                artifact: "ui.zip".into(),
                path: "ui/1.zip".into(),
                size: 1,
                sha256: "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d".into(),
                dependencies: vec![],
                required: true,
                protocol: "desktop-ipc-v1".into(),
                restart: "shell".into(),
            }],
        }
    }
    #[test]
    fn accepts_valid_manifest() {
        assert!(manifest().validate().is_ok());
    }
    #[test]
    fn rejects_escape_and_cycle() {
        let mut m = manifest();
        m.components[0].path = "../x".into();
        assert!(m.validate().is_err());
        m.components[0].path = "ui/1.zip".into();
        m.components[0].dependencies = vec!["ui-bundle".into()];
        assert!(m.validate().is_err());
    }
    #[test]
    fn verifies_hash_and_size() {
        let m = manifest();
        assert!(m.artifact_matches(&m.components[0], &[0]));
        assert!(!m.artifact_matches(&m.components[0], &[1]));
    }

    #[test]
    fn canonical_bytes_are_stable_and_unknown_fields_are_rejected() {
        let m = manifest();
        assert_eq!(m.canonical_bytes().unwrap(), m.canonical_bytes().unwrap());
        let json = format!(
            r#"{}"#,
            String::from_utf8(m.canonical_bytes().unwrap()).unwrap()
        );
        let invalid = json.trim_end_matches('}').to_owned() + ",\"extra\":true}";
        assert!(Manifest::parse(invalid.as_bytes()).is_err());
    }
}
