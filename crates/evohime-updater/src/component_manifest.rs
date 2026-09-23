use serde::{
    de::{Deserializer, Error as DeError},
    Deserialize, Serialize,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::path::Path;

/// Required schema identifier for component manifests.
pub const SCHEMA: &str = "evohime.component-manifest.v1";
/// Maximum number of components accepted in a manifest.
pub const MAX_COMPONENTS: usize = 32;
/// Maximum accepted artifact size in bytes.
pub const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;

fn default_product() -> String {
    "EvoHime".into()
}

fn default_release_id() -> String {
    "legacy-component-update".into()
}

fn default_release_commit() -> String {
    "0".repeat(40)
}

fn default_protocol() -> String {
    "desktop-ipc-v1".into()
}

fn deserialize_nullable_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::String(value) => Ok(vec![value]),
        serde_json::Value::Array(value) => serde_json::from_value(serde_json::Value::Array(value))
            .map_err(|error| D::Error::custom(format!("expected string array: {error}"))),
        value => Err(D::Error::custom(format!(
            "expected null, string, or string array, got {value}"
        ))),
    }
}

/// Immutable inventory of components included in a product release.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    /// Schema identifier; must equal [`SCHEMA`].
    pub schema: String,
    #[serde(default = "default_product")]
    /// Product name identified by the manifest.
    pub product: String,
    #[serde(default = "default_release_id")]
    /// Stable identifier of this release.
    pub release_id: String,
    /// Target operating system identifier.
    pub os: String,
    /// Target CPU architecture identifier.
    pub architecture: String,
    #[serde(default = "default_release_commit")]
    /// Source commit associated with the release.
    pub release_commit: String,
    /// Components included in the release.
    pub components: Vec<Component>,
}

/// Artifact metadata and compatibility requirements for one component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Component {
    /// Stable component identifier.
    pub id: String,
    /// Component version.
    pub version: String,
    /// Artifact filename in the release package.
    pub artifact: String,
    /// Relative destination path within the installation.
    pub path: String,
    /// Expected artifact size in bytes.
    pub size: u64,
    /// Expected SHA-256 digest in hexadecimal form.
    pub sha256: String,
    #[serde(default, deserialize_with = "deserialize_nullable_vec")]
    /// Component identifiers required by this component.
    pub dependencies: Vec<String>,
    /// Whether this component must be present in the installation.
    pub required: bool,
    #[serde(default = "default_protocol")]
    /// IPC or data protocol required by this component.
    pub protocol: String,
    /// Restart behavior required after replacing this component.
    pub restart: String,
}

/// Failure to parse or validate a component manifest.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    /// Input bytes are not a valid manifest JSON document.
    #[error("invalid manifest JSON: {0}")]
    Json(String),
    /// Manifest fields or dependency relationships violate the contract.
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

impl Manifest {
    /// Parses JSON bytes and validates the resulting manifest.
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let manifest: Self =
            serde_json::from_slice(bytes).map_err(|e| ManifestError::Json(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Checks platform, size, path, digest, and dependency invariants.
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

    /// Returns the component with the given stable identifier.
    pub fn component(&self, id: &str) -> Result<&Component, ManifestError> {
        self.components
            .iter()
            .find(|component| component.id == id)
            .ok_or_else(|| ManifestError::Invalid(format!("unknown component: {id}")))
    }

    /// Checks an in-memory artifact against its declared size and digest.
    pub fn artifact_matches(&self, component: &Component, bytes: &[u8]) -> bool {
        if bytes.len() as u64 != component.size {
            return false;
        }
        let digest = Sha256::digest(bytes);
        hex::encode(digest) == component.sha256.to_ascii_lowercase()
    }

    /// Verify an artifact without loading its complete contents into memory.
    pub fn artifact_matches_path(&self, component: &Component, path: &Path) -> io::Result<bool> {
        let file = std::fs::File::open(path)?;
        artifact_matches_reader(component, file)
    }
}

fn artifact_matches_reader<R: Read>(component: &Component, mut reader: R) -> io::Result<bool> {
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > component.size {
            return Ok(false);
        }
        digest.update(&buffer[..read]);
    }
    Ok(total == component.size
        && hex::encode(digest.finalize()) == component.sha256.to_ascii_lowercase())
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
    fn accepts_legacy_manifest_and_additive_metadata() {
        let mut value = serde_json::to_value(manifest()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("product");
        object.remove("release_id");
        object.remove("release_commit");
        object.insert("future_metadata".into(), serde_json::json!({"version": 2}));
        let component = object
            .get_mut("components")
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|components| components.first_mut())
            .and_then(serde_json::Value::as_object_mut)
            .unwrap();
        component.remove("protocol");
        component.insert("future_component_metadata".into(), serde_json::json!(true));

        let parsed = Manifest::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(parsed.product, "EvoHime");
        assert_eq!(parsed.release_id, "legacy-component-update");
        assert_eq!(parsed.release_commit, "0".repeat(40));
        assert_eq!(parsed.components[0].protocol, "desktop-ipc-v1");
    }

    #[test]
    fn accepts_legacy_dependency_shapes() {
        let mut value = serde_json::to_value(manifest().components[0].clone()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("dependencies".into(), serde_json::Value::Null);
        let parsed: Component = serde_json::from_value(value.clone()).unwrap();
        assert!(parsed.dependencies.is_empty());

        value
            .as_object_mut()
            .unwrap()
            .insert("dependencies".into(), serde_json::json!("core"));
        let parsed: Component = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.dependencies, vec!["core"]);
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
    fn verifies_artifact_from_reader_without_buffering_the_whole_file() {
        let m = manifest();
        assert!(artifact_matches_reader(&m.components[0], std::io::Cursor::new([0u8; 1])).unwrap());
        assert!(
            !artifact_matches_reader(&m.components[0], std::io::Cursor::new([0u8; 2])).unwrap()
        );
    }

    #[test]
    fn canonical_bytes_are_stable_and_unknown_fields_are_ignored() {
        let m = manifest();
        assert_eq!(m.canonical_bytes().unwrap(), m.canonical_bytes().unwrap());
        let json = String::from_utf8(m.canonical_bytes().unwrap())
            .unwrap()
            .to_string();
        let with_extra = json.trim_end_matches('}').to_owned() + ",\"extra\":true}";
        assert!(Manifest::parse(with_extra.as_bytes()).is_ok());
    }
}
